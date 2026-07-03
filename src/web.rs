use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

const DASHBOARD_HTML: &str = include_str!("dashboard.html");

pub async fn start_web(db: &Database, port: u16) -> Result<()> {
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("{} Web dashboard on http://{}", "✓".green(), addr);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let cfg = ConfigStore::load().unwrap_or_default();
        let users = db.list_users().unwrap_or_default();

        let mut buf = vec![0u8; 4096];
        let n = stream.try_read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("/");

        let (status, ctype, body) = match path {
            "/" | "/index.html" => ("200 OK", "text/html", DASHBOARD_HTML.to_string()),
            "/api/status" => {
                let nodes: Vec<_> = cfg.exit_nodes.iter().map(|(name, n)| {
                    serde_json::json!({"name":name,"type":n.node_type,"server":n.server,"socks_port":n.socks_port,"active":port_up(n.socks_port),"exit_ip":n.exit_ip})
                }).collect();
                let protos: Vec<_> = cfg.protocols.iter().map(|(name, p)| {
                    serde_json::json!({"name":name,"type":p.proto_type,"exit_node":p.exit_node,"port":p.port,"active":port_up(p.port)})
                }).collect();
                let ulist: Vec<_> = users.iter().map(|u| {
                    serde_json::json!({"username":u.username,"plan":u.plan,"status":u.status,"data_used":u.data_used_bytes,"data_limit":u.data_limit_bytes,"expires":u.expires_at})
                }).collect();
                let plans: Vec<_> = cfg.plans.iter().map(|(name, p)| {
                    serde_json::json!({"name":name,"data_limit":p.data_limit,"duration":p.duration,"max_devices":p.max_devices,"protocols":p.protocols})
                }).collect();
                let data = serde_json::json!({"nodes":nodes,"protocols":protos,"users":ulist,"plans":plans});
                ("200 OK", "application/json", data.to_string())
            }
            p if p.starts_with("/api/link/") => {
                let username = &p[10..];
                let mut links = Vec::new();
                let ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
                let domain = cfg.vps.domain.as_deref().unwrap_or("");
                for (_, proto) in &cfg.protocols {
                    match proto.proto_type.as_str() {
                        "vless" => links.push(format!("vless://{}@{}:{}?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                            proto.uuid.as_deref().unwrap_or(""), ip, proto.port, proto.sni.as_deref().unwrap_or(domain), domain, pctenc(proto.ws_path.as_deref().unwrap_or("/")), proto.exit_node)),
                        "mtproto" => links.push(format!("tg://proxy?server={}&port={}&secret={}", ip, proto.port, proto.secret.as_deref().unwrap_or(""))),
                        _ => {}
                    }
                }
                ("200 OK", "application/json", serde_json::json!({"links":links}).to_string())
            }
            _ => ("404 Not Found", "text/plain", "Not Found".to_string()),
        };

        let resp = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}", status, ctype, body.len(), body);
        let _ = stream.write_all(resp.as_bytes()).await;
    }
}

fn port_up(port: u16) -> bool {
    std::process::Command::new("ss").args(["-tlnp"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{} ", port))).unwrap_or(false)
}

fn pctenc(s: &str) -> String {
    s.chars().map(|c| match c { 'A'..='Z'|'a'..='z'|'0'..='9'|'-'|'_'|'.'|'~' => c.to_string(), _ => format!("%{:02X}", c as u8) }).collect()
}
