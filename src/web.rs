use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const DASHBOARD_HTML: &str = include_str!("dashboard.html");
const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>TunnelForge - Login</title>
<style>
body{font-family:'Inter',-apple-system,sans-serif;background:#08080c;color:#e8e8ed;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0}
.box{background:#101018;border:1px solid rgba(255,255,255,0.06);border-radius:12px;padding:32px;width:320px;text-align:center}
.box h1{font-size:1.2rem;margin-bottom:24px;display:flex;align-items:center;justify-content:center;gap:8px}
.box h1 span{color:#6366f1}
input{width:100%;padding:10px 14px;background:#08080c;border:1px solid rgba(255,255,255,0.06);border-radius:8px;color:#e8e8ed;font-size:0.9rem;margin-bottom:16px;outline:none;box-sizing:border-box}
input:focus{border-color:#6366f1}
button{width:100%;padding:10px;background:#6366f1;border:none;border-radius:8px;color:#fff;font-size:0.9rem;font-weight:600;cursor:pointer}
button:hover{background:#818cf8}
.err{color:#f87171;font-size:0.8rem;margin-bottom:12px;display:none}
</style></head><body>
<div class="box">
  <h1><span style="font-size:1.4rem">&#9889;</span> <span>Tunnel</span>Forge</h1>
  <div class="err" id="err">Invalid password</div>
  <form onsubmit="doLogin(event)">
    <input type="password" id="pw" placeholder="Password" autofocus>
    <button type="submit">Login</button>
  </form>
</div>
<script>
async function doLogin(e){
  e.preventDefault();
  const r=await fetch(location.pathname.replace(/\/+$/,'')+'/api/login',{method:'POST',headers:{'Content-Type':'application/x-www-form-urlencoded'},body:'password='+encodeURIComponent(document.getElementById('pw').value)});
  if(r.ok)location.reload();else document.getElementById('err').style.display='block';
}
</script></body></html>"#;

pub async fn start_web(db: &Database, port: u16, path: &str, password: Option<&str>) -> Result<()> {
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    let base = if path.is_empty() { String::new() } else { format!("/{}", path.trim_matches('/')) };
    let auth_required = password.is_some();
    let pw = password.unwrap_or("").to_string();

    if auth_required {
        println!("{} Web dashboard on http://{}{}/ (auth required)", "✓".green(), addr, base);
    } else {
        println!("{} Web dashboard on http://{}{}/", "✓".green(), addr, base);
    }

    loop {
        let (mut stream, _) = listener.accept().await?;
        let pw = pw.clone();
        let base = base.clone();

        let mut buf = vec![0u8; 8192];
        let n = match stream.read(&mut buf).await {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse method and path from request line
        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let _method = parts.next().unwrap_or("GET");
        let raw_uri = parts.next().unwrap_or("/");

        // Strip base path to get the route
        let route = if base.is_empty() {
            raw_uri.to_string()
        } else if raw_uri.starts_with(&base) {
            let rest = &raw_uri[base.len()..];
            if rest.is_empty() { "/".to_string() } else { rest.to_string() }
        } else {
            // Not under our base path
            let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found");
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        };

        // If accessing base without trailing slash, redirect
        if !base.is_empty() && raw_uri == base {
            let resp = format!("HTTP/1.1 302 Found\r\nLocation: {}/\r\nConnection: close\r\n\r\n", base);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        }

        // Auth check (skip for login page and login API)
        let needs_auth = !route.starts_with("/api/login");
        if auth_required && needs_auth && !check_auth(&request, &pw) {
            let html = LOGIN_HTML.to_string();
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        }

        // Route
        let (status, ctype, body) = if route == "/" || route == "/index.html" {
            ("200 OK", "text/html", DASHBOARD_HTML.to_string())
        } else if route == "/api/status" {
            let cfg = ConfigStore::load().unwrap_or_default();
            let users = db.list_users().unwrap_or_default();

            let nodes: Vec<_> = cfg.exit_nodes.iter().map(|(name, n)| {
                serde_json::json!({"name":name,"type":n.node_type,"server":n.server,"socks_port":n.socks_port,"external_port":n.external_port,"active":port_up(n.socks_port),"exit_ip":n.exit_ip})
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
        } else if route.starts_with("/api/link/") {
            let _username = &route[10..];
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
                    "mtproto" => links.push(format!(
                        "tg://proxy?server={}&port={}&secret={}",
                        ip, proto.port, proto.secret.as_deref().unwrap_or("")
                    )),
                    _ => {}
                }
            }
            ("200 OK", "application/json", serde_json::json!({"links":links}).to_string())
        } else if route == "/api/login" {
            // Handle login
            if let Some(body_start) = request.find("\r\n\r\n") {
                let body_str = &request[body_start + 4..];
                if body_str.contains(&format!("password={}", pw)) {
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: tf_token={}; Path=/; HttpOnly; SameSite=Strict\r\nConnection: close\r\n\r\n{{\"ok\":true}}",
                        pw
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
            }
            ("401 Unauthorized", "application/json", "{\"ok\":false}".to_string())
        } else {
            ("404 Not Found", "text/plain", "Not Found".to_string())
        };

        let resp = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            status, ctype, body.len(), body
        );
        let _ = stream.write_all(resp.as_bytes()).await;
    }
}

fn check_auth(request: &str, password: &str) -> bool {
    if password.is_empty() { return true; }
    for line in request.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("cookie:") && line.contains(&format!("tf_token={}", password)) {
            return true;
        }
        if lower.starts_with("authorization: basic ") {
            let encoded = line[21..].trim();
            if let Ok(decoded) = base64_decode(encoded) {
                if decoded == format!("admin:{}", password) || decoded == password {
                    return true;
                }
            }
        }
    }
    false
}

fn base64_decode(s: &str) -> std::result::Result<String, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
        .map_err(|_| ())
        .and_then(|b| String::from_utf8(b).map_err(|_| ()))
}

fn port_up(port: u16) -> bool {
    if port == 0 { return false; }
    std::process::Command::new("ss").args(["-tlnp"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{} ", port)))
        .unwrap_or(false)
}

fn pctenc(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}
