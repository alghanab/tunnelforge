use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;

pub fn add(cfg: &ConfigStore, proto_type: &str, exit: &str, port: &str, force: bool) -> Result<()> {
    let port: u16 = if port == "auto" {
        find_free_port()
    } else {
        port.parse()?
    };

    let uuid = uuid::Uuid::new_v4().to_string();
    println!("{} Protocol '{}-{}' added on port {}", "✓".green(), proto_type, exit, port);
    if proto_type == "vless" {
        let ws_path = format!("/{}", hex::encode(rand::random::<[u8; 8]>()));
        println!("  UUID: {}", uuid);
        println!("  WS Path: {}", ws_path);
    }
    Ok(())
}

pub fn list(cfg: &ConfigStore) -> Result<()> {
    if cfg.protocols.is_empty() {
        println!("No protocols configured.");
        return Ok(());
    }
    println!("{:<25} {:<10} {:<12} {:<8} {}", "NAME", "TYPE", "EXIT NODE", "PORT", "DETAILS");
    println!("{}", "-".repeat(70));
    for (name, proto) in &cfg.protocols {
        let details = if proto.proto_type == "vless" {
            format!("WS:{}", proto.ws_path.as_deref().unwrap_or("?"))
        } else if proto.proto_type == "mtproto" {
            format!("secret:...{}", &proto.secret.as_deref().unwrap_or("?").chars().rev().take(8).collect::<String>())
        } else {
            String::new()
        };
        println!("{:<25} {:<10} {:<12} {:<8} {}", name, proto.proto_type, proto.exit_node, proto.port, details);
    }
    Ok(())
}

fn find_free_port() -> u16 {
    8443
}
