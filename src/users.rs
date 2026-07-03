use anyhow::Result;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;

pub fn add(db: &Database, cfg: &ConfigStore, username: &str, plan: &str) -> Result<()> {
    let plan_cfg = cfg.plans.get(plan).ok_or_else(|| anyhow::anyhow!("Plan '{}' not found", plan))?;
    let data_bytes = parse_data(&plan_cfg.data_limit);
    let days = parse_duration(&plan_cfg.duration);
    let id = db.add_user(username, plan, data_bytes, plan_cfg.max_devices as i64, days)?;
    println!("{} User '{}' created (plan: {}, data: {}, devices: {}, days: {})", "✓".green(), username, plan, plan_cfg.data_limit, plan_cfg.max_devices, days);
    Ok(())
}

pub fn list(db: &Database) -> Result<()> {
    let users = db.list_users()?;
    if users.is_empty() { println!("No users."); return Ok(()); }
    println!("{:<12} {:<14} {:<10} {:<12} {:<12} {}", "USERNAME", "PLAN", "STATUS", "DATA USED", "LIMIT", "EXPIRES");
    println!("{}", "-".repeat(74));
    for u in &users {
        let st = match u.status.as_str() { "active" => "active".green().to_string(), "suspended" => "suspended".red().to_string(), _ => u.status.clone() };
        println!("{:<12} {:<14} {:<10} {:<12} {:<12} {}", u.username, u.plan, st, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), &u.expires_at[..10]);
    }
    Ok(())
}

pub fn show(db: &Database, username: &str) -> Result<()> {
    let u = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
    println!("User: {}\n  Plan: {}\n  Status: {}\n  Data: {} / {}\n  Expires: {}", u.username, u.plan, u.status, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), u.expires_at);
    Ok(())
}

pub fn set_status(db: &Database, username: &str, status: &str) -> Result<()> {
    db.set_status(username, status)?;
    println!("{} User '{}' -> {}", "✓".green(), username, status);
    Ok(())
}

pub fn reset(db: &Database, username: &str, days: Option<u32>, reset_data: bool) -> Result<()> {
    if let Some(d) = days { db.extend_expiry(username, d as i64)?; println!("{} Extended {} by {}d", "✓".green(), username, d); }
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

fn parse_duration(s: &str) -> i64 {
    let s = s.trim().to_lowercase();
    if s.ends_with('d') { s[..s.len()-1].parse().unwrap_or(30) }
    else if s.ends_with('m') { s[..s.len()-1].parse::<i64>().unwrap_or(1) * 30 }
    else { s.parse().unwrap_or(30) }
}

fn fmt_b(b: i64) -> String {
    if b >= 1073741824 { format!("{:.1}GB", b as f64 / 1073741824.0) }
    else if b >= 1048576 { format!("{:.1}MB", b as f64 / 1048576.0) }
    else if b >= 1024 { format!("{:.1}KB", b as f64 / 1024.0) }
    else { format!("{}B", b) }
}
