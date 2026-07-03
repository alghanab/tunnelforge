use anyhow::Result;
use crate::config::ConfigStore;
use crate::db::Database;

pub fn generate(_db: &Database, cfg: &ConfigStore, username: &str, _format: &str) -> Result<()> {
    let ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
    let domain = cfg.vps.domain.as_deref().unwrap_or("");

    println!("Links for {}", username);
    println!();

    for (name, proto) in &cfg.protocols {
        match proto.proto_type.as_str() {
            "vless" => {
                let uuid = proto.uuid.as_deref().unwrap_or("");
                let ws = proto.ws_path.as_deref().unwrap_or("/");
                let sni = if !domain.is_empty() { domain } else { ip };
                let link = format!(
                    "vless://{}@{}:443?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                    uuid, ip, sni, domain, enc(ws), name
                );
                println!("VLESS ({}):", name);
                println!("  {}", link);
                println!();
            }
            "mtproto" => {
                let secret = proto.secret.as_deref().unwrap_or("");
                let link = format!("tg://proxy?server={}&port={}&secret={}", ip, proto.port, secret);
                println!("MTProto ({}):", name);
                println!("  {}", link);
                println!();
            }
            _ => {}
        }
    }
    Ok(())
}

fn enc(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}
