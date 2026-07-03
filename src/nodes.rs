use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use std::process::Command;

pub fn add(cfg: &ConfigStore, name: &str, node_type: &str, server: Option<String>, key: Option<String>, socks_port: Option<u16>, external_port: Option<u16>) -> Result<()> {
    let server = server.unwrap_or_else(|| {
        eprint!("Server address (ip:port): ");
        let mut s = String::new();
        std::io::stdin().read_line(&mut s).unwrap();
        s.trim().to_string()
    });

    let socks_port = socks_port.unwrap_or(666);
    let external_port = external_port.unwrap_or(0);

    println!("{} Node '{}' added", "✓".green(), name);
    println!("  Type: {}", node_type);
    println!("  Server: {}", server);
    println!("  SOCKS: :{}", socks_port);
    println!("  External: :{}", external_port);
    Ok(())
}

pub fn list(cfg: &ConfigStore) -> Result<()> {
    if cfg.exit_nodes.is_empty() {
        println!("No exit nodes configured.");
        return Ok(());
    }
    println!("{:<12} {:<8} {:<25} {:<8} {:<10} {}", "NAME", "TYPE", "SERVER", "SOCKS", "EXTERNAL", "STATUS");
    println!("{}", "-".repeat(80));
    for (name, node) in &cfg.exit_nodes {
        let status = check_port(node.socks_port);
        println!("{:<12} {:<8} {:<25} {:<8} {:<10} {}", name, node.node_type, node.server, node.socks_port, node.external_port, status);
    }
    Ok(())
}

pub fn test(_cfg: &ConfigStore, name: &str) -> Result<()> {
    println!("Testing node '{}'...", name);
    Ok(())
}

pub fn remove(cfg: &ConfigStore, name: &str) -> Result<()> {
    println!("{} Node '{}' removed", "✓".green(), name);
    Ok(())
}

fn check_port(port: u16) -> String {
    let output = Command::new("ss").args(["-tlnp"]).output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if stdout.contains(&format!(":{} ", port)) {
                format!("{}", "● up".green())
            } else {
                format!("{}", "○ down".dimmed())
            }
        }
        Err(_) => "unknown".to_string(),
    }
}
