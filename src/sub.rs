use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;

pub fn generate(db: &Database, cfg: &ConfigStore, username: &str, to_file: Option<&str>) -> Result<()> {
    let user = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;
    if user.status != "active" { println!("{} User '{}' is {}", "✗".red(), username, user.status); return Ok(()); }

    let ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
    let domain = cfg.vps.domain.as_deref().unwrap_or("vpn.example.com");
    let plan = cfg.plans.get(&user.plan);
    let proto_names: Vec<&String> = plan.map(|p| p.protocols.iter().collect()).unwrap_or_default();
    let mut links = Vec::new();

    for pn in &proto_names {
        if let Some(proto) = cfg.protocols.get(*pn) {
            match proto.proto_type.as_str() {
                "vless" => links.push(format!("vless://{}@{}:443?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                    proto.uuid.as_deref().unwrap_or(""), ip, proto.sni.as_deref().unwrap_or(domain), domain, enc(proto.ws_path.as_deref().unwrap_or("/")), pn)),
                "vmess" => {
                    let v = serde_json::json!({"v":"2","ps":pn,"add":ip,"port":proto.port,"id":proto.uuid.as_deref().unwrap_or(""),"aid":0,"net":"ws","type":"none","host":domain,"path":proto.ws_path.as_deref().unwrap_or("/"),"tls":"tls","sni":domain});
                    links.push(format!("vmess://{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, v.to_string().as_bytes())));
                }
                "mtproto" => links.push(format!("tg://proxy?server={}&port={}&secret={}", ip, proto.port, proto.secret.as_deref().unwrap_or(""))),
                _ => {}
            }
        }
    }

    let content = links.join("\n");
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content.as_bytes());

    if let Some(path) = to_file {
        std::fs::write(path, &encoded)?;
        println!("{} Subscription written to {}", "✓".green(), path);
    } else {
        println!("{}", encoded);
    }
    println!("\n{} {} links for {}", "✓".green(), links.len(), username);
    Ok(())
}

fn enc(s: &str) -> String { s.chars().map(|c| match c { 'A'..='Z'|'a'..='z'|'0'..='9'|'-'|'_'|'.'|'~' => c.to_string(), _ => format!("%{:02X}", c as u8) }).collect() }
