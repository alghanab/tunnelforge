use anyhow::Result;
use colored::*;
use crate::db::Database;

pub fn add(db: &Database, username: &str, data: &str, max_ips: i64, hours: i64, connections: &str) -> Result<()> {
    let data_bytes = parse_data(data);
    let _id = db.add_user(username, data_bytes, max_ips, hours, connections)?;
    println!("{} User '{}' created (data: {}, IPs: {}, hours: {}, connections: {})",
        "✓".green(), username, data, max_ips, hours, if connections.is_empty() { "none" } else { connections });
    Ok(())
}

pub fn list(db: &Database) -> Result<()> {
    let users = db.list_users()?;
    if users.is_empty() { println!("No users."); return Ok(()); }
    println!("{:<12} {:<10} {:<8} {:<10} {:<8} {:<20} {}", "USERNAME", "STATUS", "DATA", "LIMIT", "MAX_IPS", "CONNECTIONS", "EXPIRES");
    println!("{}", "-".repeat(90));
    for u in &users {
        let st = match u.status.as_str() { "active" => "active".green().to_string(), "suspended" => "suspended".red().to_string(), _ => u.status.clone() };
        let conns = if u.connections.len() > 20 { format!("{}...", &u.connections[..17]) } else { u.connections.clone() };
        println!("{:<12} {:<10} {:<8} {:<10} {:<8} {:<20} {}", u.username, st, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), u.max_devices, conns, &u.expires_at[..10]);
    }
    Ok(())
}

pub fn show(db: &Database, username: &str) -> Result<()> {
    let u = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
    println!("User: {}\n  Status: {}\n  Data: {} / {}\n  Max IPs: {}\n  Connections: {}\n  Expires: {}",
        u.username, u.status, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), u.max_devices,
        if u.connections.is_empty() { "none".to_string() } else { u.connections.clone() }, u.expires_at);
    Ok(())
}

pub fn set_status(db: &Database, username: &str, status: &str) -> Result<()> {
    db.set_status(username, status)?;
    println!("{} User '{}' -> {}", "✓".green(), username, status);
    Ok(())
}

pub fn reset(db: &Database, username: &str, hours: Option<u32>, reset_data: bool) -> Result<()> {
    if let Some(h) = hours { db.extend_expiry(username, h as i64)?; println!("{} Extended {} by {}h", "✓".green(), username, h); }
    if reset_data { db.reset_data(username)?; println!("{} Reset data for {}", "✓".green(), username); }
    Ok(())
}

fn parse_data(s: &str) -> i64 {
    let s = s.trim().to_uppercase();
    if s.ends_with("GB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1073741824.0) as i64 }
    else if s.ends_with("MB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1048576.0) as i64 }
    else if s.ends_with("KB") { (s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1024.0) as i64 }
    else { s.parse::<i64>().unwrap_or(0) }
}

fn fmt_b(b: i64) -> String {
    if b >= 1073741824 { format!("{:.1}GB", b as f64 / 1073741824.0) }
    else if b >= 1048576 { format!("{:.1}MB", b as f64 / 1048576.0) }
    else if b >= 1024 { format!("{:.1}KB", b as f64 / 1024.0) }
    else { format!("{}B", b) }
}
