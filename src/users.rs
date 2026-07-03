use anyhow::Result;
use colored::*;
use tabled::{Table, Tabled};
use crate::config::ConfigStore;
use crate::db::Database;

pub fn add(db: &Database, cfg: &ConfigStore, username: &str, plan: &str) -> Result<()> {
    let plan_cfg = cfg.plans.get(plan).ok_or_else(|| anyhow::anyhow!("Plan '{}' not found", plan))?;
    let data_bytes = parse_data(&plan_cfg.data_limit);
    let days = parse_duration(&plan_cfg.duration);
    let id = db.add_user(username, plan, data_bytes, plan_cfg.max_devices as i64, days)?;
    println!("{} User '{}' created ({})", "✓".green(), username, id);
    Ok(())
}

pub fn list(db: &Database, _cfg: &ConfigStore) -> Result<()> {
    let users = db.list_users()?;
    if users.is_empty() {
        println!("No users.");
        return Ok(());
    }
    println!("{:<12} {:<10} {:<12} {:<12} {:<12} {}", "USERNAME", "PLAN", "STATUS", "DATA USED", "LIMIT", "EXPIRES");
    println!("{}", "-".repeat(70));
    for u in &users {
        let data_used = format_bytes(u.data_used_bytes);
        let data_limit = format_bytes(u.data_limit_bytes);
        let status = match u.status.as_str() {
            "active" => format!("{}", "active".green()),
            "suspended" => format!("{}", "suspended".red()),
            _ => u.status.clone(),
        };
        println!("{:<12} {:<10} {:<12} {:<12} {:<12} {}", u.username, u.plan, status, data_used, data_limit, &u.expires_at[..10]);
    }
    Ok(())
}

pub fn show(db: &Database, cfg: &ConfigStore, username: &str) -> Result<()> {
    let user = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
    println!("User: {}", user.username);
    println!("  Plan: {}", user.plan);
    println!("  Status: {}", user.status);
    println!("  Data: {} / {}", format_bytes(user.data_used_bytes), format_bytes(user.data_limit_bytes));
    println!("  Expires: {}", user.expires_at);
    let protos = cfg.plans.get(&user.plan).map(|p| p.protocols.join(", ")).unwrap_or_default();
    println!("  Protocols: {}", protos);
    Ok(())
}

pub fn set_status(db: &Database, username: &str, status: &str) -> Result<()> {
    db.set_status(username, status)?;
    println!("{} User '{}' set to {}", "✓".green(), username, status);
    Ok(())
}

pub fn reset(db: &Database, username: &str, extend_days: Option<u32>, reset_data: bool) -> Result<()> {
    if let Some(days) = extend_days {
        db.extend_expiry(username, days as i64)?;
        println!("{} Extended {} by {} days", "✓".green(), username, days);
    }
    if reset_data {
        db.reset_data(username)?;
        println!("{} Reset data for {}", "✓".green(), username);
    }
    Ok(())
}

fn parse_data(s: &str) -> i64 {
    let s = s.trim().to_uppercase();
    if s.ends_with("GB") { s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1073741824.0 }
    else if s.ends_with("MB") { s[..s.len()-2].trim().parse::<f64>().unwrap_or(0.0) * 1048576.0 }
    else { s.parse().unwrap_or(0.0) } as i64
}

fn parse_duration(s: &str) -> i64 {
    let s = s.trim().to_lowercase();
    if s.ends_with('d') { s[..s.len()-1].parse().unwrap_or(30) }
    else { s.parse().unwrap_or(30) }
}

fn format_bytes(b: i64) -> String {
    if b >= 1073741824 { format!("{:.1}GB", b as f64 / 1073741824.0) }
    else if b >= 1048576 { format!("{:.1}MB", b as f64 / 1048576.0) }
    else { format!("{}B", b) }
}
