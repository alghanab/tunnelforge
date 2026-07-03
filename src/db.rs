use rusqlite::{Connection, params};
use anyhow::Result;
use std::path::PathBuf;
use chrono::{Utc, DateTime, Duration};

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub plan: String,           // kept for compat, defaults to ""
    pub created_at: String,
    pub expires_at: String,
    pub data_limit_bytes: i64,
    pub data_used_bytes: i64,
    pub max_devices: i64,       // now means max_ips
    pub status: String,
    pub connections: String,    // comma-separated connection names
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
                plan TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                data_limit_bytes INTEGER NOT NULL DEFAULT 0,
                data_used_bytes INTEGER DEFAULT 0,
                max_devices INTEGER NOT NULL DEFAULT 2,
                status TEXT DEFAULT 'active',
                notes TEXT DEFAULT '',
                connections TEXT DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS traffic_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                bytes_up INTEGER DEFAULT 0,
                bytes_down INTEGER DEFAULT 0
            );
        ")?;
        // Migrate: add connections column if missing
        let _ = conn.execute("ALTER TABLE users ADD COLUMN connections TEXT DEFAULT ''", []);
        Ok(Self { conn })
    }

    pub fn add_user(&self, username: &str, data_limit: i64, max_ips: i64, duration_hours: i64, connections: &str) -> Result<String> {
        let id = format!("user-{}", username);
        let now = Utc::now();
        let expires = now + Duration::hours(duration_hours);
        self.conn.execute(
            "INSERT INTO users (id, username, plan, created_at, expires_at, data_limit_bytes, max_devices, status, connections)
             VALUES (?1, ?2, '', ?3, ?4, ?5, ?6, 'active', ?7)",
            params![id, username, now.to_rfc3339(), expires.to_rfc3339(), data_limit, max_ips, connections],
        )?;
        Ok(id)
    }

    pub fn update_user(&self, username: &str, data_limit: Option<i64>, max_ips: Option<i64>,
                       duration_hours: Option<i64>, connections: Option<&str>, status: Option<&str>) -> Result<()> {
        let mut updates = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(dl) = data_limit {
            updates.push("data_limit_bytes = ?");
            values.push(Box::new(dl));
        }
        if let Some(mi) = max_ips {
            updates.push("max_devices = ?");
            values.push(Box::new(mi));
        }
        if let Some(hours) = duration_hours {
            // Extend from current expiry (or now if expired)
            let user = self.get_user(username)?;
            if let Some(u) = user {
                let current_exp: DateTime<Utc> = u.expires_at.parse().unwrap_or_else(|_| Utc::now());
                let base = if current_exp > Utc::now() { current_exp } else { Utc::now() };
                let new_exp = base + Duration::hours(hours);
                updates.push("expires_at = ?");
                values.push(Box::new(new_exp.to_rfc3339()));
            }
        }
        if let Some(conn) = connections {
            updates.push("connections = ?");
            values.push(Box::new(conn.to_string()));
        }
        if let Some(st) = status {
            updates.push("status = ?");
            values.push(Box::new(st.to_string()));
        }

        if updates.is_empty() { return Ok(()); }

        let sql = format!("UPDATE users SET {} WHERE username = ?",
            updates.join(", "));
        values.push(Box::new(username.to_string()));
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        self.conn.execute(&sql, params_ref.as_slice())?;
        Ok(())
    }

    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, plan, created_at, expires_at, data_limit_bytes, data_used_bytes, max_devices, status, COALESCE(connections,'') FROM users WHERE username = ?1"
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
                connections: row.get(9)?,
            })
        })?;
        match rows.next() {
            Some(Ok(user)) => Ok(Some(user)),
            _ => Ok(None),
        }
    }

    pub fn list_users(&self) -> Result<Vec<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, plan, created_at, expires_at, data_limit_bytes, data_used_bytes, max_devices, status, COALESCE(connections,'') FROM users ORDER BY username"
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
                connections: row.get(9)?,
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

    pub fn extend_expiry(&self, username: &str, hours: i64) -> Result<()> {
        let user = self.get_user(username)?;
        if let Some(u) = user {
            let expires: DateTime<Utc> = u.expires_at.parse().unwrap_or_else(|_| Utc::now());
            let base = if expires > Utc::now() { expires } else { Utc::now() };
            let new_expires = base + Duration::hours(hours);
            self.conn.execute("UPDATE users SET expires_at = ?1 WHERE username = ?2",
                params![new_expires.to_rfc3339(), username])?;
        }
        Ok(())
    }

    pub fn delete_user(&self, username: &str) -> Result<()> {
        self.conn.execute("DELETE FROM users WHERE username = ?1", params![username])?;
        Ok(())
    }

    pub fn delete_all_users(&self) -> Result<()> {
        self.conn.execute("DELETE FROM users", [])?;
        Ok(())
    }
}

fn db_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
        .join(".tunnelforge").join("users.db")
}
