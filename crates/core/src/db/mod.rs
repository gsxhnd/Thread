//! SQLite local data layer.
//!
//! 本地数据目录保存**不可由工作区文件重建**的运行数据：请求历史、大响应体、
//! 机密变量值、回收站。集合/请求/环境永远以工作区文件为真相来源，不写入这里。

pub mod migrations;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrations::run(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.conn.lock();
        f(&conn)
    }
}

pub type SharedDb = Arc<DbState>;

#[cfg(test)]
static DATA_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Application data directory: `<platform data dir>/thread`.
pub fn data_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = DATA_DIR_OVERRIDE.get() {
        return dir.clone();
    }
    let dir = dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("thread");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Points the local data directory at a temporary folder. Test-only: the unit
/// tests must not write into the user's real application data directory.
#[cfg(test)]
pub fn test_data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("thread-test-data-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let _ = DATA_DIR_OVERRIDE.set(dir.clone());
        dir
    })
    .clone()
}

pub fn db_path() -> PathBuf {
    data_dir().join("thread.db")
}

/// Rolling tracing logs. Created on demand so startup can open the file.
pub fn log_dir() -> PathBuf {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Large response bodies are streamed here instead of the workspace (never committed to Git).
pub fn bodies_dir() -> PathBuf {
    let dir = data_dir().join("bodies");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Proxy capture data (CA material, persisted bodies). Never inside a workspace.
pub fn proxy_dir() -> PathBuf {
    let dir = data_dir().join("proxy");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Root CA certificate and private key. The private key never leaves this folder.
pub fn proxy_ca_dir() -> PathBuf {
    let dir = proxy_dir().join("ca");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Persisted (opt-in) request/response bodies of captured proxy entries.
pub fn proxy_bodies_dir() -> PathBuf {
    let dir = proxy_dir().join("bodies");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Secret environment variable values live here, outside any workspace.
pub fn secrets_dir() -> PathBuf {
    let dir = data_dir().join("secrets");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Deleted request/collection files are moved here instead of being removed.
pub fn trash_dir() -> PathBuf {
    let dir = data_dir().join("trash");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn require_non_empty(label: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{label} is required")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_database_and_applies_migrations() {
        let dir = std::env::temp_dir().join(format!("thread-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = DbState::open(dir.join("thread.db")).unwrap();
        let version: i64 = db
            .with_conn(|conn| Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?))
            .unwrap();
        assert!(version >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
