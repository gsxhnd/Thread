//! Versioned SQLite migrations.
//!
//! 以 `PRAGMA user_version` 作为版本真相（见 docs/03.database.md §4.2），
//! 另用 `schema_migrations` 表记录描述与时间便于排查。SQL 文件只增不改。
//!
//! 旧库若从未写过 `user_version`，版本为 0，会正常执行迁移 1。
//! 迁移只新增本地数据表，不改动库里已有的其他表。

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub sql: &'static str,
}

pub fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "init local data schema (history)",
        sql: include_str!("../../migrations/0001_init.sql"),
    }]
}

pub fn run(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at  INTEGER NOT NULL
        );",
    )?;

    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for migration in migrations() {
        if migration.version <= current {
            continue;
        }
        apply(conn, &migration)?;
    }

    Ok(())
}

fn apply(conn: &Connection, migration: &Migration) -> AppResult<()> {
    conn.execute_batch(migration.sql).map_err(|error| {
        AppError::Db(format!(
            "migration {} ({}) failed: {error}",
            migration.version, migration.description
        ))
    })?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_migrations (version, description, applied_at)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![
            migration.version,
            migration.description,
            crate::db::now_ms()
        ],
    )?;
    // PRAGMA does not accept bound parameters; the version is a trusted constant.
    conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            rusqlite::params![name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false)
    }

    #[test]
    fn applies_all_migrations_once() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, migrations().len());

        // Re-running is a no-op.
        run(&conn).unwrap();
        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(recorded as usize, migrations().len());
    }

    #[test]
    fn migration_leaves_existing_tables_untouched() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE extra (id TEXT PRIMARY KEY, note TEXT);
             INSERT INTO extra (id, note) VALUES ('e1', 'kept');",
        )
        .unwrap();

        run(&conn).unwrap();

        let note: String = conn
            .query_row("SELECT note FROM extra WHERE id = 'e1'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(note, "kept");
        assert!(table_exists(&conn, "history_entries"));
    }
}
