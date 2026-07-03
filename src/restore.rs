use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use std::process::Command;

pub fn run() -> Result<()> {
    let cfg = ConfigStore::load()?;

    println!("{}", "═".repeat(60));
    println!("  TunnelForge Restore");
    println!("  Reading config and restoring services...");
    println!("{}", "═".repeat(60));
    println!();

    // Step 1: Detect what services exist on this system
    println!("{} Detecting services...", "🔍".cyan());
    let services = detect_services(&cfg);

    // Step 2: Fix missing dependencies
    println!("{} Checking dependencies...", "🔍".cyan());
    fix_dependencies(&cfg, &services);

    // Step 3: Start services in order
    println!();
    println!("{} Starting services...", "🚀".cyan());
    let mut started = 0;
    for svc in &services {
        started += start_service(svc);
    }

    // Step 4: Verify ports
    println!();
    println!("{} Verifying ports...", "✅".cyan());
    std::thread::sleep(std::time::Duration::from_secs(2));
    let expected_ports = collect_ports(&cfg, &services);
    let up_ports = verify_ports(&expected_ports);

    // Step 5: Summary
    println!();
    println!("{}", "═".repeat(60));
    println!("  Restore Complete");
    println!("{}", "═".repeat(60));
    println!();

    for (port, name) in &expected_ports {
        let status = if up_ports.contains(port) {
            "UP".green()
        } else {
            "DOWN".red()
        };
        println!("  {:<6} {:<30} {}", port, name, status);
    }

    let up_count = expected_ports.iter().filter(|(p, _)| up_ports.contains(p)).count();
    println!();
    println!("  {}/{} ports active", up_count, expected_ports.len());

    if up_count < expected_ports.len() {
        println!("  {} Some ports are down", "⚠".yellow());
        println!("  Try: proxy-iran logs  or  journalctl -u <service> -f");
    } else {
        println!("  {} All services restored!", "✓".green());
    }

    println!();
    println!("  Useful commands:");
    println!("    tunnelforge status       Show tunnel status");
    println!("    proxy-iran show          Show client configs");
    println!("    proxy-iran restart       Restart active tunnel");

    Ok(())
}

/// Detect what services exist on this system based on config + systemd
fn detect_services(cfg: &ConfigStore) -> Vec<String> {
    let mut services = Vec::new();

    // Core services that almost everyone has
    for svc in &["caddy", "nginx", "gost-https-proxy", "gost-socks5-proxy"] {
        if service_exists(svc) {
            services.push(svc.to_string());
        }
    }

    // Check for paqet tunnels based on exit_nodes
    for (name, node) in &cfg.exit_nodes {
        if node.node_type == "paqet" {
            let svc = format!("paqet-{}", name);
            if service_exists(&svc) {
                services.push(svc);
            }
        }
    }

    // Check for xray services
    for svc in &["xray", "xray-local", "xray-canada", "xray-london"] {
        if service_exists(svc) {
            services.push(svc.to_string());
        }
    }

    // Check for mtproto services
    for (name, proto) in &cfg.protocols {
        if proto.proto_type == "mtproto" {
            // Try common service naming patterns
            let candidates = vec![
                format!("mtproto-{}", name),
                format!("mtproto-{}", name.replace("mtproto-", "")),
                "mtproto-local".to_string(),
                "mtproto-canada".to_string(),
                "mtproto-london".to_string(),
            ];
            for svc in &candidates {
                if service_exists(svc) && !services.contains(svc) {
                    services.push(svc.clone());
                }
            }
        }
    }

    // Check for other common services
    for svc in &["mtproto-bridge", "v2ray", "sing-box", "hysteria", "tuic"] {
        if service_exists(svc) && !services.contains(&svc.to_string()) {
            services.push(svc.to_string());
        }
    }

    // Also check what's installed but maybe not in config
    let all_systemd = list_systemd_services();
    for svc in &all_systemd {
        if !services.contains(svc) {
            // Check if it looks like a proxy service
            let lower = svc.to_lowercase();
            if lower.contains("xray") || lower.contains("v2ray") || lower.contains("gost")
                || lower.contains("caddy") || lower.contains("nginx") || lower.contains("paqet")
                || lower.contains("mtproto") || lower.contains("sing") || lower.contains("hysteria")
                || lower.contains("proxy") || lower.contains("tunnel")
            {
                services.push(svc.clone());
            }
        }
    }

    println!("  Found {} services", services.len());
    services
}

/// Fix missing dependencies (mtproto binaries, configs, etc.)
fn fix_dependencies(cfg: &ConfigStore, services: &[String]) {
    for svc in services {
        if svc.contains("mtproto") {
            let tunnel = if svc.contains("london") { "london" }
                         else if svc.contains("canada") { "canada" }
                         else { "local" };
            fix_mtproto_deps(tunnel, cfg);
        }
    }
}

fn fix_mtproto_deps(tunnel: &str, cfg: &ConfigStore) {
    // Find the port for this tunnel's mtproto
    let mut port = if tunnel == "canada" { 8443 } else { 2096 };
    let mut secret = String::new();

    for (name, proto) in &cfg.protocols {
        if proto.proto_type == "mtproto" {
            if (tunnel == "london" && name.contains("london")) ||
               (tunnel == "canada" && name.contains("canada")) ||
               (tunnel == "local" && name.contains("local")) {
                port = proto.port;
                secret = proto.secret.clone().unwrap_or_default();
                break;
            }
        }
    }

    // Check mtproto binary
    let dir = format!("/tmp/mtprotoproxy-{}", tunnel);
    let binary = format!("{}/mtprotoproxy.py", dir);

    if !std::path::Path::new(&binary).exists() {
        println!("  ⚠ {} binary missing, downloading...", tunnel);
        let _ = Command::new("mkdir").args(["-p", &dir]).output();

        // Try copying from another tunnel
        let other = if tunnel == "london" { "canada" } else { "london" };
        let other_bin = format!("/tmp/mtprotoproxy-{}/mtprotoproxy.py", other);
        if std::path::Path::new(&other_bin).exists() {
            let _ = Command::new("cp").args([&other_bin, &binary]).output();
            println!("  ✓ Copied from {}", other);
        } else {
            let result = Command::new("curl")
                .args(["-sL", "https://github.com/alexbers/mtprotoproxy/raw/master/mtprotoproxy.py", "-o", &binary])
                .output();
            match result {
                Ok(o) if o.status.success() => println!("  ✓ Downloaded mtprotoproxy"),
                _ => println!("  ✗ Failed to download mtprotoproxy"),
            }
        }
    }

    // Check config
    let config_path = format!("{}/config.py", dir);
    if !std::path::Path::new(&config_path).exists() {
        let _ = Command::new("mkdir").args(["-p", &dir]).output();
        let secret = if secret.is_empty() {
            format!("ee{:032x}", rand::random::<u64>())
        } else { secret };
        let content = format!("PORT = {}\nUSERS = {{\n    \"tunnelforge\": \"{}\"\n}}\n", port, secret);
        let _ = std::fs::write(&config_path, content);
        println!("  ✓ Created {}/config.py", tunnel);
    }

    // Check proxychains config
    let socks_port = if tunnel == "canada" { 667 } else { 666 };
    let pc_path = format!("/tmp/proxychains-{}.conf", tunnel);
    if !std::path::Path::new(&pc_path).exists() {
        let content = format!("strict_chain\nproxy_dns\ntcp_read_time_out 15000\ntcp_write_time_out 15000\n\n[ProxyList]\nsocks5 127.0.0.1 {}\n", socks_port);
        let _ = std::fs::write(&pc_path, content);
        println!("  ✓ Created proxychains-{}.conf", tunnel);
    }
}

/// Start a systemd service
fn start_service(name: &str) -> i32 {
    let status = Command::new("systemctl")
        .args(["is-active", &format!("{}.service", name)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    if status == "active" {
        println!("  ✓ {} already running", name);
        return 0;
    }

    let result = Command::new("systemctl")
        .args(["start", &format!("{}.service", name)])
        .output();

    match result {
        Ok(o) if o.status.success() => {
            println!("  ✓ {} started", name);
            1
        }
        Ok(_) => {
            println!("  ✗ {} failed to start", name);
            0
        }
        Err(e) => {
            println!("  ✗ {} error: {}", name, e);
            0
        }
    }
}

/// Collect expected ports from config + services
fn collect_ports(cfg: &ConfigStore, services: &[String]) -> Vec<(u16, String)> {
    let mut ports = Vec::new();

    // Web server ports
    if services.iter().any(|s| s.contains("caddy") || s.contains("nginx")) {
        ports.push((443, "HTTPS (caddy/nginx)".to_string()));
        ports.push((80, "HTTP (caddy/nginx)".to_string()));
    }

    // Protocol ports from config
    for (name, proto) in &cfg.protocols {
        let label = format!("{} ({})", name, proto.proto_type);
        if !ports.iter().any(|(p, _)| *p == proto.port) {
            ports.push((proto.port, label));
        }
    }

    // GOST ports
    if services.iter().any(|s| s.contains("gost")) {
        for port in &[2087u16, 4444] {
            if !ports.iter().any(|(p, _)| p == port) {
                ports.push((*port, format!("GOST :{}", port)));
            }
        }
    }

    // Exit node external ports
    for (name, node) in &cfg.exit_nodes {
        if node.external_port > 0 {
            let label = format!("Paqet {} :{}", name, node.external_port);
            if !ports.iter().any(|(p, _)| *p == node.external_port) {
                ports.push((node.external_port, label));
            }
        }
    }

    // SOCKS ports from config
    for (name, node) in &cfg.exit_nodes {
        if node.socks_port > 0 {
            let label = format!("SOCKS {} :{}", name, node.socks_port);
            if !ports.iter().any(|(p, _)| *p == node.socks_port) {
                ports.push((node.socks_port, label));
            }
        }
    }

    ports
}

/// Verify which ports are actually listening
fn verify_ports(expected: &[(u16, String)]) -> Vec<u16> {
    let output = Command::new("ss")
        .args(["-tlnp"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    expected.iter()
        .filter(|(port, _)| output.contains(&format!(":{} ", port)))
        .map(|(port, _)| *port)
        .collect()
}

/// Check if a systemd service unit exists
fn service_exists(name: &str) -> bool {
    Command::new("systemctl")
        .args(["cat", &format!("{}.service", name)])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// List all systemd service units on the system
fn list_systemd_services() -> Vec<String> {
    Command::new("systemctl")
        .args(["list-unit-files", "--type=service", "--no-pager", "--no-legend"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    let name = line.split_whitespace().next()?;
                    let name = name.strip_suffix(".service").unwrap_or(name);
                    Some(name.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}
