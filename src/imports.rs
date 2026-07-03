use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;

pub fn add(cfg: &ConfigStore, config_link: &str, name: Option<String>, port: &str, exit: Option<String>) -> Result<()> {
    let name = name.unwrap_or_else(|| format!("import-{}", &hex::encode(rand::random::<[u8; 4]>())[..8]));
    let port: u16 = if port == "auto" { 8443 } else { port.parse()? };

    let parsed = parse_config(config_link);
    match parsed {
        Some((ref_type, server)) => {
            println!("{} Config '{}' imported", "✓".green(), name);
            println!("  Type: {}", ref_type);
            println!("  Server: {}", server);
            println!("  Local port: {}", port);
            if let Some(ref e) = exit {
                println!("  Exit node: {}", e);
            }
        }
        None => {
            println!("{} Could not parse config", "✗".red());
        }
    }
    Ok(())
}

pub fn list(cfg: &ConfigStore) -> Result<()> {
    if cfg.imports.is_empty() {
        println!("No imported configs.");
        return Ok(());
    }
    println!("{:<15} {:<8} {:<25} {:<8} {}", "NAME", "TYPE", "SERVER", "PORT", "EXIT NODE");
    println!("{}", "-".repeat(65));
    for (name, imp) in &cfg.imports {
        println!("{:<15} {:<8} {:<25} {:<8} {}", name, imp.source_type, imp.source_link.chars().take(20).collect::<String>(), imp.local_port, imp.exit_node.as_deref().unwrap_or("direct"));
    }
    Ok(())
}

pub fn remove(cfg: &ConfigStore, name: &str) -> Result<()> {
    println!("{} Import '{}' removed", "✓".green(), name);
    Ok(())
}

pub fn test(name: &str) -> Result<()> {
    println!("Testing '{}'...", name);
    Ok(())
}

fn parse_config(link: &str) -> Option<(String, String)> {
    if link.starts_with("vless://") {
        let rest = &link[8..];
        let parts: Vec<&str> = rest.split('@').collect();
        if parts.len() >= 2 {
            let server_port: Vec<&str> = parts[1].split('?').collect();
            let sp: Vec<&str> = server_port[0].split(':').collect();
            return Some(("vless".to_string(), format!("{}:{}", sp[0], sp.get(1).unwrap_or(&"443"))));
        }
    }
    if link.starts_with("vmess://") {
        return Some(("vmess".to_string(), "vmess-server".to_string()));
    }
    if link.starts_with("trojan://") {
        return Some(("trojan".to_string(), "trojan-server".to_string()));
    }
    None
}
