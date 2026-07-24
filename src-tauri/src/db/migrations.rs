use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub fn run(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );",
    )?;

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 1 {
        apply(conn, 1, "init schema", include_str!("../../migrations/0001_init.sql"))?;
    }

    Ok(())
}

fn apply(conn: &Connection, version: i64, description: &str, sql: &str) -> AppResult<()> {
    conn.execute_batch(sql)
        .map_err(|e| AppError::Db(format!("migration {version} failed: {e}")))?;
    conn.execute(
        "INSERT INTO _migrations (version, description, applied_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![version, description, crate::db::now_ms()],
    )?;
    Ok(())
}
