use anyhow::Result;
use crate::config::ConfigStore;
use crate::db::Database;

pub fn generate(db: &Database, cfg: &ConfigStore, username: &str, format: &str) -> Result<()> {
    let user = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;
    let vps_ip = cfg.vps.ip.as_deref().unwrap_or("YOUR_VPS_IP");
    let domain = cfg.vps.domain.as_deref().unwrap_or("vpn.example.com");

    let plan = cfg.plans.get(&user.plan);
    let proto_names: Vec<&String> = plan.map(|p| p.protocols.iter().collect()).unwrap_or_default();

    println!("Links for {}", username);
    println!("Plan: {} | Status: {}", user.plan, user.status);
    println!();

    for proto_name in &proto_names {
        if let Some(proto) = cfg.protocols.get(*proto_name) {
            match proto.proto_type.as_str() {
                "vless" if format == "all" || format == "v2rayng" || format == "sub" => {
                    let uuid = proto.uuid.as_deref().unwrap_or("uuid");
                    let ws_path = proto.ws_path.as_deref().unwrap_or("/");
                    let sni = proto.sni.as_deref().unwrap_or(domain);
                    println!("VLESS ({}):", proto_name);
                    println!("  vless://{}@{}:{}?encryption=none&security=tls&sni={}&fp=chrome&type=ws&host={}&path={}#{}",
                        uuid, vps_ip, proto.port, sni, domain, urlencoding::encode(ws_path), proto_name);
                    println!();
                }
                "mtproto" if format == "all" || format == "telegram" => {
                    let secret = proto.secret.as_deref().unwrap_or("secret");
                    println!("MTProto ({}):", proto_name);
                    println!("  tg://proxy?server={}&port={}&secret={}", vps_ip, proto.port, secret);
                    println!();
                }
                _ => {}
            }
        }
    }
    Ok(())
}
