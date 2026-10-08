//! Request history stored in the local SQLite database.
//!
//! 历史与响应体属于**本地运行数据**：写入应用数据目录，不进入工作区、不进 Git
//! （见 docs/03.database.md 与 docs/07.roadmap.md 阶段 1）。
//! 大响应体的落盘文件随历史条目一起删除。

use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::db::{DbState, new_id, now_ms};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub id: String,
    pub protocol: String,
    pub name: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
    pub request_file_path: Option<String>,
    pub has_body_file: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDetail {
    #[serde(flatten)]
    pub summary: HistorySummary,
    pub workspace_path: String,
    pub request_snapshot: Value,
    pub response_snapshot: Option<Value>,
    /// Inline response body when it was small enough; large bodies live on disk.
    pub response_body: Option<String>,
    pub response_body_file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub items: Vec<HistorySummary>,
    pub total: i64,
}

pub struct NewHistoryEntry {
    pub workspace_path: String,
    pub request_file_path: Option<String>,
    pub protocol: String,
    pub name: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub request_snapshot: String,
    pub response_snapshot: Option<String>,
    pub response_body_path: Option<String>,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
}

pub fn insert(db: &DbState, entry: &NewHistoryEntry) -> AppResult<String> {
    let id = new_id();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO history_entries (
                id, workspace_path, request_file_path, protocol, name, method, url,
                request_snapshot, response_snapshot, response_body_path,
                status_code, duration_ms, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                entry.workspace_path,
                entry.request_file_path,
                entry.protocol,
                entry.name,
                entry.method,
                entry.url,
                entry.request_snapshot,
                entry.response_snapshot,
                entry.response_body_path,
                entry.status_code,
                entry.duration_ms,
                now_ms(),
            ],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn list(
    db: &DbState,
    workspace_path: &str,
    protocol: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<HistoryPage> {
    let limit = limit.clamp(1, 500);
    let offset = offset.max(0);
    db.with_conn(|conn| {
        let total: i64 = match protocol {
            Some(protocol) => conn.query_row(
                "SELECT COUNT(*) FROM history_entries WHERE workspace_path = ?1 AND protocol = ?2",
                params![workspace_path, protocol],
                |row| row.get(0),
            )?,
            None => conn.query_row(
                "SELECT COUNT(*) FROM history_entries WHERE workspace_path = ?1",
                params![workspace_path],
                |row| row.get(0),
            )?,
        };

        let mut statement = conn.prepare(
            "SELECT id, protocol, name, method, url, status_code, duration_ms, created_at,
                    request_file_path, response_body_path
             FROM history_entries
             WHERE workspace_path = ?1 AND (?2 IS NULL OR protocol = ?2)
             ORDER BY created_at DESC
             LIMIT ?3 OFFSET ?4",
        )?;
        let rows =
            statement.query_map(params![workspace_path, protocol, limit, offset], |row| {
                let body_path: Option<String> = row.get(9)?;
                Ok(HistorySummary {
                    id: row.get(0)?,
                    protocol: row.get(1)?,
                    name: row.get(2)?,
                    method: row.get(3)?,
                    url: row.get(4)?,
                    status_code: row.get(5)?,
                    duration_ms: row.get(6)?,
                    created_at: row.get(7)?,
                    request_file_path: row.get(8)?,
                    has_body_file: body_path.is_some(),
                })
            })?;
        let items = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(HistoryPage { items, total })
    })
}

pub fn get(db: &DbState, id: &str) -> AppResult<HistoryDetail> {
    let row = db.with_conn(|conn| {
        let result = conn
            .query_row(
                "SELECT id, workspace_path, request_file_path, protocol, name, method, url,
                        request_snapshot, response_snapshot, response_body_path,
                        status_code, duration_ms, created_at
                 FROM history_entries WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                        row.get::<_, i64>(12)?,
                    ))
                },
            )
            .optional()?;
        Ok(result)
    })?;

    let Some((
        id,
        workspace_path,
        request_file_path,
        protocol,
        name,
        method,
        url,
        request_snapshot,
        response_snapshot,
        response_body_path,
        status_code,
        duration_ms,
        created_at,
    )) = row
    else {
        return Err(AppError::NotFound(format!("history entry not found: {id}")));
    };

    let response_body = match response_body_path.as_deref() {
        Some(path) if Path::new(path).exists() => std::fs::read_to_string(path).ok(),
        _ => None,
    };

    Ok(HistoryDetail {
        summary: HistorySummary {
            id,
            protocol,
            name,
            method,
            url,
            status_code,
            duration_ms,
            created_at,
            request_file_path,
            has_body_file: response_body_path.is_some(),
        },
        workspace_path,
        request_snapshot: serde_json::from_str(&request_snapshot)
            .unwrap_or(Value::String(request_snapshot)),
        response_snapshot: response_snapshot
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok()),
        response_body,
        response_body_file_path: response_body_path,
    })
}

/// Deletes one entry and its response body file. Returns the removed body path.
pub fn delete(db: &DbState, id: &str) -> AppResult<Option<String>> {
    let body_path = db.with_conn(|conn| {
        let existing: Option<Option<String>> = conn
            .query_row(
                "SELECT response_body_path FROM history_entries WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(body_path) => {
                conn.execute("DELETE FROM history_entries WHERE id = ?1", params![id])?;
                Ok(body_path)
            }
            None => Err(AppError::NotFound(format!("history entry not found: {id}"))),
        }
    })?;

    if let Some(path) = body_path.as_deref() {
        let _ = std::fs::remove_file(path);
    }
    Ok(body_path)
}

/// Clears entries, optionally only those older than `before_timestamp`.
pub fn clear(db: &DbState, workspace_path: &str, before: Option<i64>) -> AppResult<i64> {
    let body_paths: Vec<String> = db.with_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT response_body_path FROM history_entries
             WHERE workspace_path = ?1 AND response_body_path IS NOT NULL
               AND (?2 IS NULL OR created_at < ?2)",
        )?;
        let rows = statement.query_map(params![workspace_path, before], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;

    let deleted = db.with_conn(|conn| {
        let count = conn.execute(
            "DELETE FROM history_entries
             WHERE workspace_path = ?1 AND (?2 IS NULL OR created_at < ?2)",
            params![workspace_path, before],
        )?;
        Ok(count as i64)
    })?;

    for path in body_paths {
        let _ = std::fs::remove_file(path);
    }
    Ok(deleted)
}

/// Keeps only the newest `keep` entries (0 disables trimming).
pub fn trim(db: &DbState, workspace_path: &str, keep: i64) -> AppResult<i64> {
    if keep <= 0 {
        return Ok(0);
    }
    let body_paths: Vec<String> = db.with_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT response_body_path FROM history_entries
             WHERE workspace_path = ?1 AND response_body_path IS NOT NULL
               AND id NOT IN (
                   SELECT id FROM history_entries WHERE workspace_path = ?1
                   ORDER BY created_at DESC LIMIT ?2
               )",
        )?;
        let rows =
            statement.query_map(params![workspace_path, keep], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;

    let deleted = db.with_conn(|conn| {
        let count = conn.execute(
            "DELETE FROM history_entries
             WHERE workspace_path = ?1 AND id NOT IN (
                 SELECT id FROM history_entries WHERE workspace_path = ?1
                 ORDER BY created_at DESC LIMIT ?2
             )",
            params![workspace_path, keep],
        )?;
        Ok(count as i64)
    })?;

    for path in body_paths {
        let _ = std::fs::remove_file(path);
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbState;
    use std::path::PathBuf;

    fn temp_db(tag: &str) -> (DbState, PathBuf) {
        let dir = std::env::temp_dir().join(format!("thread-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = DbState::open(dir.join("thread.db")).unwrap();
        (db, dir)
    }

    fn entry(workspace: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            workspace_path: workspace.to_string(),
            request_file_path: Some("api/list.http.json".into()),
            protocol: "http".into(),
            name: Some("List Users".into()),
            method: Some("GET".into()),
            url: Some("https://example.com/users".into()),
            request_snapshot: r#"{"protocol":"http","name":"List Users"}"#.into(),
            response_snapshot: Some(r#"{"statusCode":200}"#.into()),
            response_body_path: None,
            status_code: Some(200),
            duration_ms: Some(42),
        }
    }

    #[test]
    fn inserts_lists_and_reads_back_entries() {
        let (db, dir) = temp_db("crud");
        let id = insert(&db, &entry("/tmp/ws")).unwrap();
        insert(&db, &entry("/tmp/ws")).unwrap();
        insert(&db, &entry("/tmp/other")).unwrap();

        let page = list(&db, "/tmp/ws", None, 10, 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);

        let filtered = list(&db, "/tmp/ws", Some("grpc"), 10, 0).unwrap();
        assert_eq!(filtered.total, 0);

        let detail = get(&db, &id).unwrap();
        assert_eq!(detail.summary.method.as_deref(), Some("GET"));
        assert_eq!(detail.request_snapshot["protocol"], "http");
        assert_eq!(detail.response_snapshot.unwrap()["statusCode"], 200);

        assert_eq!(get(&db, "missing").unwrap_err().code(), "NOT_FOUND");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deletes_entry_and_body_file() {
        let (db, dir) = temp_db("delete");
        let body_path = dir.join("body.bin");
        std::fs::write(&body_path, "large body").unwrap();
        let mut new_entry = entry("/tmp/ws");
        new_entry.response_body_path = Some(body_path.to_string_lossy().into_owned());
        let id = insert(&db, &new_entry).unwrap();

        let detail = get(&db, &id).unwrap();
        assert!(detail.summary.has_body_file);
        assert_eq!(detail.response_body.as_deref(), Some("large body"));

        delete(&db, &id).unwrap();
        assert!(!body_path.exists());
        assert!(get(&db, &id).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clears_and_trims_history() {
        let (db, dir) = temp_db("clear");
        for _ in 0..5 {
            insert(&db, &entry("/tmp/ws")).unwrap();
        }
        assert_eq!(trim(&db, "/tmp/ws", 2).unwrap(), 3);
        assert_eq!(list(&db, "/tmp/ws", None, 10, 0).unwrap().total, 2);

        assert_eq!(clear(&db, "/tmp/ws", None).unwrap(), 2);
        assert_eq!(list(&db, "/tmp/ws", None, 10, 0).unwrap().total, 0);

        insert(&db, &entry("/tmp/ws")).unwrap();
        let future = now_ms() + 60_000;
        assert_eq!(clear(&db, "/tmp/ws", Some(future)).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
