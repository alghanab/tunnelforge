use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Live state shared across the application
pub type LiveState = Arc<Mutex<LiveData>>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiveData {
    pub users: HashMap<String, UserLive>,
    pub ports: HashMap<u16, PortLive>,
    pub last_update: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserLive {
    pub active_ips: Vec<String>,
    pub active_connections: u32,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub rate_up: u64,   // bytes/sec
    pub rate_down: u64, // bytes/sec
    pub last_seen: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortLive {
    pub port: u16,
    pub connections: u32,
    pub source_ips: Vec<String>,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

/// Create a new shared live state
pub fn new_live_state() -> LiveState {
    Arc::new(Mutex::new(LiveData::default()))
}

/// Start the background monitoring task
pub fn start_monitor(live: LiveState) {
    std::thread::spawn(move || {
        // Set up iptables rules for traffic counting on first run
        setup_iptables();

        loop {
            std::thread::sleep(Duration::from_secs(10));
            if let Err(e) = update_live(&live) {
                eprintln!("[live] update error: {}", e);
            }
        }
    });
}

/// Set up iptables rules for byte counting on proxy ports
fn setup_iptables() {
    let ports = vec![443, 2053, 2087, 2096, 3389, 4444, 8443, 999];

    // Create our chain if it doesn't exist
    let _ = Command::new("iptables")
        .args(["-N", "TF_COUNT"])
        .output();

    // Flush existing rules in our chain
    let _ = Command::new("iptables")
        .args(["-F", "TF_COUNT"])
        .output();

    // Remove existing jump to our chain
    let _ = Command::new("iptables")
        .args(["-D", "INPUT", "-j", "TF_COUNT"])
        .output();

    // Add jump to our chain
    let _ = Command::new("iptables")
        .args(["-I", "INPUT", "-j", "TF_COUNT"])
        .output();

    // Add per-port rules (both directions)
    for port in &ports {
        // Incoming traffic (downloads for the client = uploads from server perspective)
        let _ = Command::new("iptables")
            .args(["-A", "TF_COUNT", "-p", "tcp", "--sport", &port.to_string(), "-j", "ACCEPT"])
            .output();
        // Outgoing traffic (uploads from client = downloads from server perspective)
        let _ = Command::new("iptables")
            .args(["-A", "TF_COUNT", "-p", "tcp", "--dport", &port.to_string(), "-j", "ACCEPT"])
            .output();
    }
}

/// Update live state by polling ss and iptables
fn update_live(live: &LiveState) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().to_rfc3339();

    // Get active connections from ss
    let port_data = poll_connections()?;

    // Get traffic data from iptables
    let traffic_data = poll_traffic();

    // Map ports to connection names and users
    let cfg = crate::config::ConfigStore::load().unwrap_or_default();
    let db = crate::db::Database::open()?;
    let users = db.list_users().unwrap_or_default();

    // Build port -> connection name mapping
    let port_to_conn = build_port_mapping(&cfg);

    // Build connection -> users mapping
    let conn_to_users = build_conn_user_mapping(&users);

    // Aggregate per-user live data
    let mut user_lives: HashMap<String, crate::live::UserLive> = HashMap::new();

    for (port, pl) in &port_data {
        let conn_name = match port_to_conn.get(port) {
            Some(n) => n.clone(),
            None => continue,
        };

        // Get traffic for this port
        let (bytes_in, bytes_out) = traffic_data.get(port).copied().unwrap_or((0, 0));

        // Find users who have this connection
        if let Some(usernames) = conn_to_users.get(&conn_name) {
            for username in usernames {
                let ul = user_lives.entry(username.clone()).or_default();

                // Add unique IPs
                for ip in &pl.source_ips {
                    if !ul.active_ips.contains(ip) {
                        ul.active_ips.push(ip.clone());
                    }
                }
                ul.active_connections += pl.connections;
                ul.bytes_up += bytes_out;
                ul.bytes_down += bytes_in;
                ul.is_active = pl.connections > 0;
                if pl.connections > 0 {
                    ul.last_seen = now.clone();
                }
            }
        }
    }

    // Calculate rates (compare with previous state)
    let mut state = live.lock().unwrap();
    for (username, ul) in &mut user_lives {
        if let Some(prev) = state.users.get(username) {
            let dt = 10.0; // seconds between polls
            if ul.bytes_down > prev.bytes_down {
                ul.rate_down = ((ul.bytes_down - prev.bytes_down) as f64 / dt) as u64;
            }
            if ul.bytes_up > prev.bytes_up {
                ul.rate_up = ((ul.bytes_up - prev.bytes_up) as f64 / dt) as u64;
            }
        }
    }

    // Update port data
    let mut port_lives = HashMap::new();
    for (port, pl) in &port_data {
        let (bytes_in, bytes_out) = traffic_data.get(port).copied().unwrap_or((0, 0));
        port_lives.insert(*port, PortLive {
            port: *port,
            connections: pl.connections,
            source_ips: pl.source_ips.clone(),
            bytes_in,
            bytes_out,
        });
    }

    state.users = user_lives;
    state.ports = port_lives;
    state.last_update = now;

    Ok(())
}

/// Parse ss output for active connections per port
fn poll_connections() -> Result<HashMap<u16, PortLive>, Box<dyn std::error::Error>> {
    let output = Command::new("ss")
        .args(["-tnp", "state", "established"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports: HashMap<u16, PortLive> = HashMap::new();
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        // Parse local address:port (index 2)
        let local = parts[2];
        let port = match local.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
            Some(p) => p,
            None => continue,
        };

        // Parse peer address:port (index 3)
        let peer = parts[3];
        // Handle IPv6-mapped addresses like [::ffff:1.2.3.4]
        let source_ip = if peer.starts_with('[') {
            // IPv6: extract between [ and ]:
            if let Some(bracket_end) = peer.find(']') {
                let ip_part = &peer[1..bracket_end];
                // Extract IPv4 from ::ffff:x.x.x.x
                if let Some(v4) = ip_part.strip_prefix("::ffff:") {
                    v4.to_string()
                } else {
                    ip_part.to_string()
                }
            } else {
                peer.to_string()
            }
        } else if let Some(pos) = peer.rfind(':') {
            peer[..pos].to_string()
        } else {
            peer.to_string()
        };

        let pl = ports.entry(port).or_default();
        pl.port = port;
        pl.connections += 1;
        if !source_ip.is_empty() && source_ip != "*" && !pl.source_ips.contains(&source_ip.to_string()) {
            pl.source_ips.push(source_ip.to_string());
        }
    }

    Ok(ports)
}

/// Get byte counters from iptables TF_COUNT chain
fn poll_traffic() -> HashMap<u16, (u64, u64)> {
    let mut traffic: HashMap<u16, (u64, u64)> = HashMap::new();

    let output = match Command::new("iptables")
        .args(["-L", "TF_COUNT", "-v", "-n", "-x", "--line-numbers"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return traffic,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports: HashMap<u16, PortLive> = HashMap::new();

    for line in stdout.lines().skip(2) { // skip header lines
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 { continue; }

        let bytes: u64 = parts[1].parse().unwrap_or(0);
        let port: u16 = match parts.iter().position(|&p| p == "spt:" || p == "dpt:") {
            Some(pos) => {
                let prefix = parts[pos];
                parts.get(pos + 1).and_then(|p| p.parse().ok()).unwrap_or(0)
            }
            None => continue,
        };

        if port == 0 { continue; }

        let entry = traffic.entry(port).or_insert((0, 0));
        if parts.contains(&"spt:") {
            // Source port = outgoing from server = bytes_out
            entry.1 += bytes;
        } else if parts.contains(&"dpt:") {
            // Dest port = incoming to server = bytes_in
            entry.0 += bytes;
        }
    }

    traffic
}

/// Build mapping: port -> connection name
fn build_port_mapping(cfg: &crate::config::ConfigStore) -> HashMap<u16, String> {
    let mut map = HashMap::new();

    for (name, proto) in &cfg.protocols {
        // Map the protocol's internal port
        map.insert(proto.port, name.clone());
    }

    // Map external ports from exit nodes
    for (name, node) in &cfg.exit_nodes {
        if node.external_port > 0 {
            for (proto_name, proto) in &cfg.protocols {
                if proto.exit_node == *name {
                    map.entry(node.external_port).or_insert_with(|| proto_name.clone());
                }
            }
        }
    }

    // Map port 443 (caddy) to all vless protocols
    // Since caddy routes all vless on 443, we can't distinguish per-protocol
    // but we can map 443 -> first vless protocol found
    for (name, proto) in &cfg.protocols {
        if proto.proto_type == "vless" {
            map.entry(443).or_insert_with(|| name.clone());
            break;
        }
    }

    map
}

/// Build mapping: connection name -> list of usernames
fn build_conn_user_mapping(users: &[crate::db::User]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for user in users {
        if user.connections.is_empty() { continue; }
        for conn in user.connections.split(',') {
            let conn = conn.trim();
            if !conn.is_empty() {
                map.entry(conn.to_string()).or_default().push(user.username.clone());
            }
        }
    }

    map
}
