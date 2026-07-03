use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;
use crate::tester;
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

        let mut buf = vec![0u8; 65536];
        let n = match stream.read(&mut buf).await {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let method = parts.next().unwrap_or("GET");
        let raw_uri = parts.next().unwrap_or("/");

        // Parse query string for GET params
        let (route, query_str) = if let Some(pos) = raw_uri.find('?') {
            (raw_uri[..pos].to_string(), &raw_uri[pos + 1..])
        } else {
            (raw_uri.to_string(), "")
        };

        let route = if base.is_empty() {
            route
        } else if route.starts_with(&base) {
            let rest = &route[base.len()..];
            if rest.is_empty() { "/".to_string() } else { rest.to_string() }
        } else {
            let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found");
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        };

        if !base.is_empty() && raw_uri == base {
            let resp = format!("HTTP/1.1 302 Found\r\nLocation: {}/\r\nConnection: close\r\n\r\n", base);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        }

        let needs_auth = !route.starts_with("/api/login");
        if auth_required && needs_auth && !check_auth(&request, &pw) {
            let html = LOGIN_HTML.to_string();
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
            let _ = stream.write_all(resp.as_bytes()).await;
            continue;
        }

        // Extract body for POST/PUT requests
        let body_str = request.find("\r\n\r\n").map(|pos| &request[pos + 4..]).unwrap_or("");

        let (status, ctype, body) = if route == "/" || route == "/index.html" {
            ("200 OK", "text/html", DASHBOARD_HTML.to_string())

        // ─── Status ───────────────────────────────────
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
            let imports: Vec<_> = cfg.imports.iter().map(|(name, imp)| {
                serde_json::json!({"name":name,"source_type":imp.source_type,"source_link":imp.source_link,"local_port":imp.local_port,"exit_node":imp.exit_node})
            }).collect();
            let data = serde_json::json!({
                "nodes":nodes,"protocols":protos,"users":ulist,"plans":plans,"imports":imports,
                "vps_ip":cfg.vps.ip,"vps_domain":cfg.vps.domain,
                "web_port":port,"web_path":base,"auth_enabled":auth_required
            });
            ("200 OK", "application/json", data.to_string())

        // ─── Users CRUD ───────────────────────────────
        } else if route == "/api/users" && method == "POST" {
            // Create user: {"username":"x","plan":"y"}
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let username = p["username"].as_str().unwrap_or("");
                    let plan = p["plan"].as_str().unwrap_or("");
                    if username.is_empty() || plan.is_empty() {
                        json_err("400 Bad Request", "username and plan required")
                    } else {
                        let cfg = ConfigStore::load().unwrap_or_default();
                        match cfg.plans.get(plan) {
                            Some(plan_cfg) => {
                                let data_bytes = parse_data(&plan_cfg.data_limit);
                                let days = parse_duration(&plan_cfg.duration);
                                match db.add_user(username, plan, data_bytes, plan_cfg.max_devices as i64, days) {
                                    Ok(_) => json_ok("201 Created", serde_json::json!({"ok":true,"username":username})),
                                    Err(e) => json_err("400 Bad Request", &e.to_string()),
                                }
                            }
                            None => json_err("400 Bad Request", &format!("Plan '{}' not found", plan)),
                        }
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        } else if route.starts_with("/api/users/") && method == "DELETE" {
            let username = &route[11..];
            if username.is_empty() {
                json_err("400 Bad Request", "username required")
            } else {
                match db.set_status(username, "deleted") {
                    Ok(_) => json_ok("200 OK", serde_json::json!({"ok":true})),
                    Err(e) => json_err("400 Bad Request", &e.to_string()),
                }
            }

        } else if route.starts_with("/api/users/") && method == "PUT" {
            let username = &route[11..];
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let action = p["action"].as_str().unwrap_or("");
                    let result = match action {
                        "enable" => db.set_status(username, "active"),
                        "disable" => db.set_status(username, "suspended"),
                        "reset_data" => db.reset_data(username),
                        "extend" => {
                            let days = p["days"].as_i64().unwrap_or(30);
                            db.extend_expiry(username, days)
                        }
                        _ => {
                            // Direct field updates
                            if let Some(status) = p["status"].as_str() {
                                db.set_status(username, status)?;
                            }
                            if p["reset_data"].as_bool().unwrap_or(false) {
                                db.reset_data(username)?;
                            }
                            if let Some(days) = p["extend_days"].as_i64() {
                                db.extend_expiry(username, days)?;
                            }
                            Ok(())
                        }
                    };
                    match result {
                        Ok(_) => json_ok("200 OK", serde_json::json!({"ok":true})),
                        Err(e) => json_err("400 Bad Request", &e.to_string()),
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        // ─── Connections CRUD ─────────────────────────
        } else if route == "/api/connections" && method == "GET" {
            let cfg = ConfigStore::load().unwrap_or_default();
            let imports: Vec<_> = cfg.imports.iter().map(|(name, imp)| {
                serde_json::json!({"name":name,"source_type":imp.source_type,"source_link":imp.source_link,"local_port":imp.local_port,"exit_node":imp.exit_node})
            }).collect();
            let protos: Vec<_> = cfg.protocols.iter().map(|(name, p)| {
                serde_json::json!({"name":name,"type":p.proto_type,"exit_node":p.exit_node,"port":p.port,"active":port_up(p.port)})
            }).collect();
            ("200 OK", "application/json", serde_json::json!({"imports":imports,"protocols":protos}).to_string())

        } else if route == "/api/connections" && method == "POST" {
            // Add connection: {"name":"x","uri":"vless://...","type":"vless"}
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let name = p["name"].as_str().unwrap_or("");
                    let uri = p["uri"].as_str().unwrap_or("");
                    let conn_type = p["type"].as_str().unwrap_or("direct");
                    if name.is_empty() || uri.is_empty() {
                        json_err("400 Bad Request", "name and uri required")
                    } else if tester::parse_single_config(uri, 0).is_none() {
                        json_err("400 Bad Request", "Could not parse config URI")
                    } else {
                        let mut cfg = ConfigStore::load().unwrap_or_default();
                        cfg.imports.insert(name.to_string(), crate::config::ImportConfig {
                            name: name.to_string(),
                            source_type: conn_type.to_string(),
                            source_link: uri.to_string(),
                            local_port: 0,
                            local_socks_port: 0,
                            exit_node: p["exit_node"].as_str().map(|s| s.to_string()),
                        });
                        match cfg.save() {
                            Ok(_) => json_ok("201 Created", serde_json::json!({"ok":true,"name":name})),
                            Err(e) => json_err("500 Internal Server Error", &e.to_string()),
                        }
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        } else if route.starts_with("/api/connections/") && method == "DELETE" {
            let name = urldecode(&route[16..]);
            let mut cfg = ConfigStore::load().unwrap_or_default();
            let removed = cfg.imports.remove(&name).is_some();
            let _ = cfg.save();
            if removed {
                json_ok("200 OK", serde_json::json!({"ok":true}))
            } else {
                json_err("404 Not Found", "Connection not found")
            }

        } else if route.starts_with("/api/connections/") && method == "PUT" {
            let name = urldecode(&route[16..]);
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let mut cfg = ConfigStore::load().unwrap_or_default();
                    // Remove old entry if name changed
                    let new_name = p["name"].as_str().unwrap_or(&name);
                    if new_name != name {
                        cfg.imports.remove(&name);
                    }
                    let uri = p["uri"].as_str().unwrap_or("");
                    if !uri.is_empty() && tester::parse_single_config(uri, 0).is_none() {
                        json_err("400 Bad Request", "Could not parse config URI")
                    } else {
                        cfg.imports.insert(new_name.to_string(), crate::config::ImportConfig {
                            name: new_name.to_string(),
                            source_type: p["type"].as_str().unwrap_or("direct").to_string(),
                            source_link: uri.to_string(),
                            local_port: p["local_port"].as_u64().unwrap_or(0) as u16,
                            local_socks_port: 0,
                            exit_node: p["exit_node"].as_str().map(|s| s.to_string()),
                        });
                        match cfg.save() {
                            Ok(_) => json_ok("200 OK", serde_json::json!({"ok":true})),
                            Err(e) => json_err("500 Internal Server Error", &e.to_string()),
                        }
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        // ─── Plans CRUD ───────────────────────────────
        } else if route == "/api/plans" && method == "GET" {
            let cfg = ConfigStore::load().unwrap_or_default();
            let plans: Vec<_> = cfg.plans.iter().map(|(name, p)| {
                serde_json::json!({"name":name,"data_limit":p.data_limit,"duration":p.duration,"max_devices":p.max_devices,"protocols":p.protocols})
            }).collect();
            ("200 OK", "application/json", serde_json::json!({"plans":plans}).to_string())

        } else if route == "/api/plans" && method == "POST" {
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let name = p["name"].as_str().unwrap_or("");
                    if name.is_empty() {
                        json_err("400 Bad Request", "name required")
                    } else {
                        let mut cfg = ConfigStore::load().unwrap_or_default();
                        cfg.plans.insert(name.to_string(), crate::config::PlanConfig {
                            name: name.to_string(),
                            data_limit: p["data_limit"].as_str().unwrap_or("50GB").to_string(),
                            duration: p["duration"].as_str().unwrap_or("30d").to_string(),
                            max_devices: p["max_devices"].as_u64().unwrap_or(2) as u32,
                            protocols: p["protocols"].as_array()
                                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                .unwrap_or_default(),
                        });
                        match cfg.save() {
                            Ok(_) => json_ok("201 Created", serde_json::json!({"ok":true})),
                            Err(e) => json_err("500 Internal Server Error", &e.to_string()),
                        }
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        } else if route.starts_with("/api/plans/") && method == "DELETE" {
            let name = urldecode(&route[10..]);
            let mut cfg = ConfigStore::load().unwrap_or_default();
            cfg.plans.remove(&name);
            let _ = cfg.save();
            json_ok("200 OK", serde_json::json!({"ok":true}))

        // ─── VPS Config ───────────────────────────────
        } else if route == "/api/config" && method == "PUT" {
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(p) => {
                    let mut cfg = ConfigStore::load().unwrap_or_default();
                    if let Some(ip) = p["vps_ip"].as_str() { cfg.vps.ip = Some(ip.to_string()); }
                    if let Some(domain) = p["vps_domain"].as_str() { cfg.vps.domain = Some(domain.to_string()); }
                    match cfg.save() {
                        Ok(_) => json_ok("200 OK", serde_json::json!({"ok":true})),
                        Err(e) => json_err("500 Internal Server Error", &e.to_string()),
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        // ─── Tester ───────────────────────────────────
        } else if route == "/api/tester" && method == "POST" {
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(parsed) if parsed.get("configs").and_then(|c| c.as_str()).is_some() => {
                    let configs_text = parsed["configs"].as_str().unwrap_or("");
                    let do_http = parsed["http"].as_bool().unwrap_or(false);
                    let sort_by = parsed["sort"].as_str().unwrap_or("");
                    let configs = tester::parse_bulk_configs(configs_text);
                    if configs.is_empty() {
                        json_err("200 OK", "No valid configs found")
                    } else {
                        let mut results = tester::test_bulk(&configs, do_http, 10).await;
                        tester::lookup_geo(&mut results).await;
                        tester::sort_results(&mut results, tester::SortBy::from_str(sort_by));
                        let data = serde_json::json!({"results": results, "total": results.len(),
                            "healthy": results.iter().filter(|r| r.status == "healthy" || r.status == "slow").count()});
                        ("200 OK", "application/json", data.to_string())
                    }
                }
                Ok(_) => json_err("400 Bad Request", "Missing configs field"),
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        } else if route == "/api/tester/add" && method == "POST" {
            match serde_json::from_str::<serde_json::Value>(body_str) {
                Ok(parsed) => {
                    if let Some(conns) = parsed.get("connections").and_then(|c| c.as_array()) {
                        let mut cfg = ConfigStore::load().unwrap_or_default();
                        let mut added = 0;
                        for conn in conns {
                            let name = conn["name"].as_str().unwrap_or("");
                            let uri = conn["uri"].as_str().unwrap_or("");
                            let conn_type = conn["type"].as_str().unwrap_or("direct");
                            if name.is_empty() || uri.is_empty() { continue; }
                            if tester::parse_single_config(uri, 0).is_some() {
                                cfg.imports.insert(name.to_string(), crate::config::ImportConfig {
                                    name: name.to_string(),
                                    source_type: conn_type.to_string(),
                                    source_link: uri.to_string(),
                                    local_port: 0,
                                    local_socks_port: 0,
                                    exit_node: None,
                                });
                                added += 1;
                            }
                        }
                        let _ = cfg.save();
                        ("200 OK", "application/json", serde_json::json!({"added":added,"total":cfg.imports.len()}).to_string())
                    } else {
                        json_err("400 Bad Request", "Missing connections field")
                    }
                }
                Err(e) => json_err("400 Bad Request", &format!("Invalid JSON: {}", e)),
            }

        } else if route.starts_with("/api/link/") {
            let _username = &route[10..];
            let cfg = ConfigStore::load().unwrap_or_default();
            let mut links = Vec::new();
            let ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
            let domain = cfg.vps.domain.as_deref().unwrap_or("");
            for (_, proto) in &cfg.protocols {
                match proto.proto_type.as_str() {
                    "vless" => {
                        let sni = if !domain.is_empty() { domain } else { ip };
                        links.push(format!("vless://{}@{}:443?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                            proto.uuid.as_deref().unwrap_or(""), ip, sni, domain, pctenc(proto.ws_path.as_deref().unwrap_or("/")), proto.exit_node));
                    }
                    "mtproto" => links.push(format!("tg://proxy?server={}&port={}&secret={}",
                        ip, proto.port, proto.secret.as_deref().unwrap_or(""))),
                    _ => {}
                }
            }
            ("200 OK", "application/json", serde_json::json!({"links":links}).to_string())

        } else if route == "/api/login" {
            if let Some(body_start) = request.find("\r\n\r\n") {
                let body_str = &request[body_start + 4..];
                if body_str.contains(&format!("password={}", pw)) {
                    let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: tf_token={}; Path=/; HttpOnly; SameSite=Strict\r\nConnection: close\r\n\r\n{{\"ok\":true}}", pw);
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
            }
            ("401 Unauthorized", "application/json", "{\"ok\":false}".to_string())

        } else {
            ("404 Not Found", "text/plain", "Not Found".to_string())
        };

        let resp = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}", status, ctype, body.len(), body);
        let _ = stream.write_all(resp.as_bytes()).await;
    }
}

// ─── Helpers ────────────────────────────────────────────

fn json_ok(status: &str, data: serde_json::Value) -> (&'static str, &'static str, String) {
    (if status.starts_with("201") { "201 Created" } else { "200 OK" }, "application/json", data.to_string())
}

fn json_err(status: &str, msg: &str) -> (&'static str, &'static str, String) {
    let s = match status {
        "400 Bad Request" => "400 Bad Request",
        "404 Not Found" => "404 Not Found",
        "500 Internal Server Error" => "500 Internal Server Error",
        _ => "200 OK",
    };
    (s, "application/json", serde_json::json!({"error":msg}).to_string())
}

fn check_auth(request: &str, password: &str) -> bool {
    if password.is_empty() { return true; }
    for line in request.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("cookie:") && line.contains(&format!("tf_token={}", password)) { return true; }
        if lower.starts_with("authorization: basic ") {
            let encoded = line[21..].trim();
            if let Ok(decoded) = base64_decode(encoded) {
                if decoded == format!("admin:{}", password) || decoded == password { return true; }
            }
        }
    }
    false
}

fn base64_decode(s: &str) -> std::result::Result<String, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| ()).and_then(|b| String::from_utf8(b).map_err(|_| ()))
}

fn port_up(port: u16) -> bool {
    if port == 0 { return false; }
    std::process::Command::new("ss").args(["-tlnp"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{} ", port)))
        .unwrap_or(false)
}

fn pctenc(s: &str) -> String {
    s.chars().map(|c| match c { 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(), _ => format!("%{:02X}", c as u8) }).collect()
}

fn urldecode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00"), 16) {
                result.push(byte as char); i += 3; continue;
            }
        }
        result.push(if bytes[i] == b'+' { ' ' } else { bytes[i] as char });
        i += 1;
    }
    result
}

fn parse_data(s: &str) -> i64 {
    let s = s.trim().to_uppercase();
    if s.ends_with("GB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1073741824.0) as i64 }
    else if s.ends_with("MB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1048576.0) as i64 }
    else if s.ends_with("KB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1024.0) as i64 }
    else { s.parse::<i64>().unwrap_or(0) }
}

fn parse_duration(s: &str) -> i64 {
    let s = s.trim().to_lowercase();
    if s.ends_with('d') { s[..s.len()-1].parse().unwrap_or(30) }
    else if s.ends_with('m') { s[..s.len()-1].parse::<i64>().unwrap_or(1) * 30 }
    else { s.parse().unwrap_or(30) }
}
