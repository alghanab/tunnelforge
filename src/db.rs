use rusqlite::{Connection, params};
use anyhow::Result;
use std::path::PathBuf;
use chrono::{Utc, DateTime, Duration};

pub struct Database {
    conn: Connection,
}

#[derive(Debug)]
pub struct User {
    pub id: String,
    pub username: String,
    pub plan: String,
    pub created_at: String,
    pub expires_at: String,
    pub data_limit_bytes: i64,
    pub data_used_bytes: i64,
    pub max_devices: i64,
    pub status: String,
}

impl Database {
    pub fn open() -> Result<Self> {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                plan TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                data_limit_bytes INTEGER NOT NULL,
                data_used_bytes INTEGER DEFAULT 0,
                max_devices INTEGER NOT NULL,
                status TEXT DEFAULT 'active',
                notes TEXT DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS traffic_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                bytes_up INTEGER DEFAULT 0,
                bytes_down INTEGER DEFAULT 0
            );
        ")?;
        Ok(Self { conn })
    }

    pub fn add_user(&self, username: &str, plan: &str, data_limit: i64, max_devices: i64, duration_days: i64) -> Result<String> {
        let id = format!("user-{}", username);
        let now = Utc::now();
        let expires = now + Duration::days(duration_days);
        self.conn.execute(
            "INSERT OR IGNORE INTO users (id, username, plan, created_at, expires_at, data_limit_bytes, max_devices, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active')",
            params![id, username, plan, now.to_rfc3339(), expires.to_rfc3339(), data_limit, max_devices],
        )?;
        Ok(id)
    }

    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, plan, created_at, expires_at, data_limit_bytes, data_used_bytes, max_devices, status FROM users WHERE username = ?1"
        )?;
        let mut rows = stmt.query_map(params![username], |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                plan: row.get(2)?,
                created_at: row.get(3)?,
                expires_at: row.get(4)?,
                data_limit_bytes: row.get(5)?,
                data_used_bytes: row.get(6)?,
                max_devices: row.get(7)?,
                status: row.get(8)?,
            })
        })?;
        match rows.next() {
            Some(Ok(user)) => Ok(Some(user)),
            _ => Ok(None),
        }
    }

    pub fn list_users(&self) -> Result<Vec<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, plan, created_at, expires_at, data_limit_bytes, data_used_bytes, max_devices, status FROM users ORDER BY username"
        )?;
        let users = stmt.query_map([], |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                plan: row.get(2)?,
                created_at: row.get(3)?,
                expires_at: row.get(4)?,
                data_limit_bytes: row.get(5)?,
                data_used_bytes: row.get(6)?,
                max_devices: row.get(7)?,
                status: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    pub fn set_status(&self, username: &str, status: &str) -> Result<()> {
        self.conn.execute("UPDATE users SET status = ?1 WHERE username = ?2", params![status, username])?;
        Ok(())
    }

    pub fn reset_data(&self, username: &str) -> Result<()> {
        self.conn.execute("UPDATE users SET data_used_bytes = 0 WHERE username = ?1", params![username])?;
        Ok(())
    }

    pub fn extend_expiry(&self, username: &str, days: i64) -> Result<()> {
        let user = self.get_user(username)?;
        if let Some(u) = user {
            let expires: DateTime<Utc> = u.expires_at.parse().unwrap_or_else(|_| Utc::now());
            let new_expires = expires + Duration::days(days);
            self.conn.execute("UPDATE users SET expires_at = ?1 WHERE username = ?2",
                params![new_expires.to_rfc3339(), username])?;
        }
        Ok(())
    }
}

fn db_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
        .join(".tunnelforge").join("users.db")
}
