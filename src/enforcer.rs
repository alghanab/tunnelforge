use anyhow::Result;
use chrono::Utc;
use colored::*;
use crate::config::ConfigStore;
use crate::db::Database;

pub fn run(db: &Database, _cfg: &ConfigStore, dry_run: bool) -> Result<()> {
    let users = db.list_users()?;
    let now = Utc::now();
    let mode = if dry_run { "DRY RUN" } else { "LIVE" };

    println!("Enforcement Check ({})", mode);
    println!("Checking {} users...", users.len());
    println!();

    let mut actions = 0;
    for user in &users {
        if user.status != "active" { continue; }

        let expires: chrono::DateTime<chrono::Utc> = user.expires_at.parse().unwrap_or(now);
        if now > expires {
            println!("  {} {}: DISABLED — expired on {}", "✗".red(), user.username, &user.expires_at[..10]);
            if !dry_run { db.set_status(&user.username, "suspended")?; }
            actions += 1;
            continue;
        }

        if user.data_used_bytes >= user.data_limit_bytes {
            println!("  {} {}: DISABLED — data limit exceeded", "✗".red(), user.username);
            if !dry_run { db.set_status(&user.username, "suspended")?; }
            actions += 1;
        }
    }

    if actions == 0 {
        println!("{} All users within limits", "✓".green());
    }
    Ok(())
}
