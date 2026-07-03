use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;
use std::process::Command;

pub fn status(db: &Database, cfg: &ConfigStore) -> Result<()> {
    println!("{}", "═".repeat(60));
    println!("  TunnelForge Status");
    println!("{}", "═".repeat(60));
    println!();

    let ip = cfg.vps.ip.as_deref().unwrap_or("not set");
    let domain = cfg.vps.domain.as_deref().unwrap_or("not set");
    println!("  VPS IP:   {}", ip);
    println!("  Domain:   {}", domain);
    println!();

    if !cfg.exit_nodes.is_empty() {
        println!("Exit Nodes:");
        for (name, node) in &cfg.exit_nodes {
            let status = if check_port(node.socks_port) { "●".green() } else { "○".dimmed() };
            println!("  {} {}: {} (exit: {})", status, name, node.server, node.exit_ip.as_deref().unwrap_or("?"));
        }
        println!();
    }

    if !cfg.protocols.is_empty() {
        println!("Protocols:");
        for (name, proto) in &cfg.protocols {
            let status = if check_port(proto.port) { "●".green() } else { "○".dimmed() };
            println!("  {} {}: {} on :{}", status, name, proto.proto_type, proto.port);
        }
        println!();
    }

    let users = db.list_users().unwrap_or_default();
    let active = users.iter().filter(|u| u.status == "active").count();
    println!("Users: {} total, {} active", users.len(), active);
    Ok(())
}

pub fn map(cfg: &ConfigStore) -> Result<()> {
    println!("{}", "═".repeat(60));
    println!("  Connection Map");
    println!("{}", "═".repeat(60));
    println!();

    for (name, proto) in &cfg.protocols {
        let node_name = &proto.exit_node;
        let exit_ip = cfg.exit_nodes.get(node_name).and_then(|n| n.exit_ip.as_deref()).unwrap_or("?");
        println!("  {} ({} :{})", name.cyan(), proto.proto_type, proto.port);
        println!("    → exit: {} ({})", node_name, exit_ip);
        println!();
    }
    Ok(())
}

pub fn ports() -> Result<()> {
    println!("{}", "═".repeat(60));
    println!("  Port Scanner");
    println!("{}", "═".repeat(60));
    println!();

    let output = Command::new("ss").args(["-tlnp"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let addr = parts[3];
            let port = addr.rsplit(':').next().unwrap_or("?");
            let bind = if addr.contains("127.0.0") { "local" } else { "extern" };
            let proc = parts.get(5).unwrap_or(&"");
            println!("  {:<8} {:<8} {}", port, bind, proc);
        }
    }
    Ok(())
}

fn check_port(port: u16) -> bool {
    Command::new("ss").args(["-tlnp"]).output().map(|o| {
        String::from_utf8_lossy(&o.stdout).contains(&format!(":{} ", port))
    }).unwrap_or(false)
}
