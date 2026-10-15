use crate::config::Config;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub const SCHEMA_SQL: &str = include_str!("../sql/schema.sql");
pub const SEED_SQL: &str = include_str!("../sql/seed.sql");

/// Reduced sys_oss table (avatar uploads only; object storage is out of scope).
pub const EXTRA_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sys_oss (
    oss_id        INTEGER PRIMARY KEY,
    tenant_id     TEXT DEFAULT '000000',
    file_name     TEXT DEFAULT '',
    original_name TEXT DEFAULT '',
    file_suffix   TEXT DEFAULT '',
    url           TEXT NOT NULL DEFAULT '',
    service       TEXT DEFAULT 'local',
    create_dept   INTEGER,
    create_by     INTEGER,
    create_time   TEXT,
    update_time   TEXT,
    update_by     INTEGER
);
";

/// Shared application state.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub cfg: Config,
    pub ids: crate::id::Ids,
    pub rsa_key: Option<rsa::RsaPrivateKey>,
    pub sessions: Mutex<std::collections::HashMap<String, crate::session::Session>>,
    pub captchas: Mutex<std::collections::HashMap<String, crate::session::Captcha>>,
    pub pwd_fails: Mutex<std::collections::HashMap<String, crate::session::PwdFail>>,
}

pub type Shared = Arc<AppState>;

pub fn open_db(cfg: &Config) -> Result<Connection, String> {
    let path = Path::new(&cfg.db_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create db dir failed: {e}"))?;
        }
    }
    let fresh = !path.exists();
    let conn =
        Connection::open(path).map_err(|e| format!("open sqlite {} failed: {e}", cfg.db_path))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=OFF; PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("pragma failed: {e}"))?;
    if fresh {
        conn.execute_batch(SCHEMA_SQL).map_err(|e| format!("schema failed: {e}"))?;
        conn.execute_batch(SEED_SQL).map_err(|e| format!("seed failed: {e}"))?;
        eprintln!("[ruoyi-rs] database initialized at {}", cfg.db_path);
    }
    // idempotent migrations for existing databases
    conn.execute_batch(EXTRA_SCHEMA).map_err(|e| format!("extra schema failed: {e}"))?;
    Ok(conn)
}

/// Read a config value from sys_config.
pub fn get_config(db: &Connection, key: &str) -> Option<String> {
    let mut stmt = db
        .prepare("SELECT config_value FROM sys_config WHERE config_key = ?")
        .ok()?;
    stmt.query_row([key], |r| r.get(0)).ok()
}
