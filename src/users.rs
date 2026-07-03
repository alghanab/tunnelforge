use anyhow::Result;
use colored::*;
use crate::db::Database;

pub fn add(db: &Database, username: &str, plan: &str) -> Result<()> {
    let id = db.add_user(username, plan, 50 * 1073741824, 2, 30)?;
    println!("{} User '{}' created ({})", "✓".green(), username, id);
    Ok(())
}
pub fn list(db: &Database) -> Result<()> {
    let users = db.list_users()?;
    if users.is_empty() { println!("No users."); return Ok(()); }
    println!("{:<12} {:<10} {:<12} {:<12} {:<12} {}", "USERNAME", "PLAN", "STATUS", "DATA USED", "LIMIT", "EXPIRES");
    println!("{}", "-".repeat(70));
    for u in &users {
        let st = match u.status.as_str() { "active" => "active".green().to_string(), "suspended" => "suspended".red().to_string(), _ => u.status.clone() };
        println!("{:<12} {:<10} {:<12} {:<12} {:<12} {}", u.username, u.plan, st, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), &u.expires_at[..10]);
    }
    Ok(())
}
pub fn show(db: &Database, username: &str) -> Result<()> {
    let u = db.get_user(username)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
    println!("User: {}\n  Plan: {}\n  Status: {}\n  Data: {} / {}\n  Expires: {}", u.username, u.plan, u.status, fmt_b(u.data_used_bytes), fmt_b(u.data_limit_bytes), u.expires_at);
    Ok(())
}
pub fn set_status(db: &Database, username: &str, status: &str) -> Result<()> { db.set_status(username, status)?; println!("{} User '{}' -> {}", "✓".green(), username, status); Ok(()) }
pub fn reset(db: &Database, username: &str, days: Option<u32>, reset_data: bool) -> Result<()> {
    if let Some(d) = days { db.extend_expiry(username, d as i64)?; println!("{} Extended {} by {}d", "✓".green(), username, d); }
    if reset_data { db.reset_data(username)?; println!("{} Reset data for {}", "✓".green(), username); }
    Ok(())
}
fn fmt_b(b: i64) -> String { if b >= 1073741824 { format!("{:.1}GB", b as f64 / 1073741824.0) } else if b >= 1048576 { format!("{:.1}MB", b as f64 / 1048576.0) } else { format!("{}B", b) } }
