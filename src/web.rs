use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const DASHBOARD_HTML: &str = include_str!("dashboard.html");

pub async fn start_web(db: &Database, port: u16, path: &str, password: Option<&str>) -> Result<()> {
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    let base = format!("/{}", path.trim_matches('/'));
    let auth_required = password.is_some();
    let password = password.unwrap_or("").to_string();

    if auth_required {
        println!("{} Web dashboard on http://{}{} (auth required)", "✓".green(), addr, base);
    } else {
        println!("{} Web dashboard on http://{}{}", "✓".green(), addr, base);
    }
    println!("  Dashboard: http://{}{}", addr, base);
    println!("  API:       http://{}/api/status", addr);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let password = password.clone();
        let base = base.clone();

        let mut buf = vec![0u8; 8192];
        let n = match stream.read(&mut buf).await {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        let first_line = request.lines().next().unwrap_or("");
        let raw_path = first_line.split_whitespace().nth(1).unwrap_or("/");

        // Strip base path prefix
        let path = if base != "/" && raw_path.starts_with(&base) {
            &raw_path[base.len()..]
        } else if raw_path == "/" && base != "/" {
            // Redirect to base path
            let resp = format!("HTTP/1.1 302 Found\r\nLocation: {}/\r\nConnection: close\r\n\r\n", base);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        } else if raw_path.starts_with("/api/") {
            raw_path
        } else {
            "/404"
        };

        // Auth check
        if auth_required && !check_auth(&request, &password) && path != "/404" {
            let login_html = LOGIN_HTML.replace("{{base}}", &base);
            let resp = format!("HTTP/1.1 401 Unauthorized\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", login_html.len(), login_html);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        }

        let (status, ctype, body) = match path {
            "/" | "/index.html" => ("200 OK", "text/html", DASHBOARD_HTML.to_string()),
            "/api/status" => {
                let cfg = ConfigStore::load().unwrap_or_default();
                let users = db.list_users().unwrap_or_default();

                let nodes: Vec<_> = cfg.exit_nodes.iter().map(|(name, n)| {
                    serde_json::json!({"name":name,"type":n.node_type,"server":n.server,"socks_port":n.socks_port,"active":port_up(n.socks_port),"exit_ip":n.exit_ip})
                }).collect();

                let protos: Vec<_> = cfg.protocols.iter().map(|(name, p)| {
                    serde_json::json!({"name":name,"type":p.proto_type,"exit_node":p.exit_node,"port":p.port,"active":port_up(p.port),"ws_path":p.ws_path,"uuid":p.uuid,"sni":p.sni,"secret":p.secret,"tls":p.tls})
                }).collect();

                let ulist: Vec<_> = users.iter().map(|u| {
                    serde_json::json!({"username":u.username,"plan":u.plan,"status":u.status,"data_used":u.data_used_bytes,"data_limit":u.data_limit_bytes,"expires":u.expires_at})
                }).collect();

                let plans: Vec<_> = cfg.plans.iter().map(|(name, p)| {
                    serde_json::json!({"name":name,"data_limit":p.data_limit,"duration":p.duration,"max_devices":p.max_devices,"protocols":p.protocols})
                }).collect();

                let data = serde_json::json!({
                    "nodes":nodes,"protocols":protos,"users":ulist,"plans":plans,
                    "vps_ip":cfg.vps.ip,"vps_domain":cfg.vps.domain,
                    "web_port":port,"web_path":base,"auth_enabled":auth_required
                });
                ("200 OK", "application/json", data.to_string())
            }
            p if p.starts_with("/api/link/") => {
                let _username = &p[10..];
                let cfg = ConfigStore::load().unwrap_or_default();
                let mut links = Vec::new();
                let ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
                let domain = cfg.vps.domain.as_deref().unwrap_or("");
                for (_, proto) in &cfg.protocols {
                    match proto.proto_type.as_str() {
                        "vless" => links.push(format!(
                            "vless://{}@{}:{}?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                            proto.uuid.as_deref().unwrap_or(""), ip, proto.port,
                            proto.sni.as_deref().unwrap_or(domain), domain,
                            pctenc(proto.ws_path.as_deref().unwrap_or("/")), proto.exit_node
                        )),
                        "mtproto" => links.push(format!("tg://proxy?server={}&port={}&secret={}", ip, proto.port, proto.secret.as_deref().unwrap_or(""))),
                        _ => {}
                    }
                }
                ("200 OK", "application/json", serde_json::json!({"links":links}).to_string())
            }
            "/api/login" => {
                // Handle login POST - return token
                if let Some(body_start) = request.find("\r\n\r\n") {
                    let body = &request[body_start+4..];
                    if body.contains(&format!("password={}", password)) {
                        let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: tf_token={}; Path=/; HttpOnly\r\nConnection: close\r\n\r\n{{\"ok\":true}}", password);
                        let _ = stream.write_all(resp.as_bytes()).await;
                        continue;
                    }
                }
                ("401 Unauthorized", "application/json", "{\"ok\":false}".to_string())
            }
            _ => ("404 Not Found", "text/plain", "Not Found".to_string()),
        };

        let resp = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}", status, ctype, body.len(), body);
        let _ = stream.write_all(resp.as_bytes()).await;
    }
}

fn check_auth(request: &str, password: &str) -> bool {
    if password.is_empty() { return true; }
    // Check cookie
    for line in request.lines() {
        if line.to_lowercase().starts_with("cookie:") {
            if line.contains(&format!("tf_token={}", password)) {
                return true;
            }
        }
    }
    // Check basic auth
    for line in request.lines() {
        if line.to_lowercase().starts_with("authorization: basic ") {
            let encoded = line[21..].trim();
            if let Ok(decoded) = base64_decode(encoded) {
                if decoded.ends_with(&format!(":{}", password)) || decoded == password {
                    return true;
                }
            }
        }
    }
    false
}

fn base64_decode(s: &str) -> Result<String, ()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| ())?;
    String::from_utf8(bytes).map_err(|_| ())
}

fn port_up(port: u16) -> bool {
    if port == 0 { return false; }
    std::process::Command::new("ss").args(["-tlnp"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{} ", port))).unwrap_or(false)
}

fn pctenc(s: &str) -> String {
    s.chars().map(|c| match c { 'A'..='Z'|'a'..='z'|'0'..='9'|'-'|'_'|'.'|'~' => c.to_string(), _ => format!("%{:02X}", c as u8) }).collect()
}

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>TunnelForge - Login</title>
<style>
body{font-family:-apple-system,sans-serif;background:#0a0a0f;color:#e0e0e0;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0}
.login{background:#12121a;border:1px solid #1e1e2e;border-radius:12px;padding:32px;width:320px;text-align:center}
.login h1{font-size:1.2rem;margin-bottom:24px;color:#fff}
.login h1 span{color:#7c6cf0}
input{width:100%;padding:10px 14px;background:#0a0a0f;border:1px solid #1e1e2e;border-radius:8px;color:#e0e0e0;font-size:0.9rem;margin-bottom:16px;outline:none;box-sizing:border-box}
input:focus{border-color:#7c6cf0}
button{width:100%;padding:10px;background:#7c6cf0;border:none;border-radius:8px;color:#fff;font-size:0.9rem;font-weight:600;cursor:pointer}
button:hover{background:#5a4bd1}
.error{color:#ff6b6b;font-size:0.8rem;margin-bottom:12px;display:none}
</style></head><body>
<div class="login">
  <h1>🔥 <span>Tunnel</span>Forge</h1>
  <div class="error" id="err">Invalid password</div>
  <form onsubmit="login(event)">
    <input type="password" id="pw" placeholder="Password" autofocus>
    <button type="submit">Login</button>
  </form>
</div>
<script>
async function login(e) {
  e.preventDefault();
  const pw = document.getElementById('pw').value;
  const base = '{{base}}';
  const r = await fetch(base + '/api/login', {
    method: 'POST',
    headers: {'Content-Type': 'application/x-www-form-urlencoded'},
    body: 'password=' + encodeURIComponent(pw)
  });
  if (r.ok) { window.location.href = base + '/'; }
  else { document.getElementById('err').style.display = 'block'; }
}
</script></body></html>"#;
