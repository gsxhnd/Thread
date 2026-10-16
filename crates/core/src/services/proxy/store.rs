//! Captured entries: the in-memory list for the current session and the opt-in SQLite copy.
//!
//! Memory holds the active session (bounded by entry count and total body bytes).
//! When persistence is enabled every finished entry is also written to `proxy_entries`,
//! with bodies stored as files under `proxy/bodies/`. Persisted data is local-only and
//! never written into a workspace.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{OptionalExtension, params};

use crate::db::{DbState, proxy_bodies_dir};
use crate::error::{AppError, AppResult};
use crate::services::proxy::capture::{CapturedBody, HeaderPair};
use crate::services::proxy::{ProxyEntryDetail, ProxyEntrySummary, ProxySessionSummary};

/// Upper bound on body bytes kept in memory across all entries of a session.
pub const MEMORY_BODY_BUDGET: u64 = 128 * 1024 * 1024;

/// Hard cap on rows read back in one list call (UI paging is client-side).
pub const LIST_LIMIT: i64 = 5_000;

/// Everything recorded for one captured exchange (or tunnel).
#[derive(Debug, Clone)]
pub struct StoredEntry {
    pub summary: ProxyEntrySummary,
    pub request_headers: Vec<HeaderPair>,
    pub response_headers: Vec<HeaderPair>,
    pub request_body: CapturedBody,
    pub response_body: CapturedBody,
}

impl StoredEntry {
    pub fn detail(&self) -> ProxyEntryDetail {
        ProxyEntryDetail {
            summary: self.summary.clone(),
            request_headers: self.request_headers.clone(),
            response_headers: self.response_headers.clone(),
            request_body: crate::services::proxy::capture::body_view(&self.request_body),
            response_body: crate::services::proxy::capture::body_view(&self.response_body),
        }
    }

    fn body_bytes(&self) -> u64 {
        (self.request_body.bytes.len() + self.response_body.bytes.len()) as u64
    }
}

/// Identity of the session an entry belongs to (needed to create the session row lazily).
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub workspace_path: Option<String>,
    pub port: u16,
    pub started_at: i64,
}

#[derive(Default)]
pub struct EntryStore {
    inner: Mutex<MemoryState>,
}

#[derive(Default)]
struct MemoryState {
    session_id: Option<String>,
    entries: VecDeque<Arc<StoredEntry>>,
    body_bytes: u64,
}

impl EntryStore {
    /// Starts a fresh in-memory list for `session_id`; the previous list is dropped.
    pub fn begin_session(&self, session_id: &str) {
        let mut state = self.inner.lock();
        state.session_id = Some(session_id.to_string());
        state.entries.clear();
        state.body_bytes = 0;
    }

    pub fn session_id(&self) -> Option<String> {
        self.inner.lock().session_id.clone()
    }

    /// Adds an entry if it belongs to the session currently held in memory.
    /// Returns `false` for late entries from an earlier session.
    pub fn push(&self, entry: Arc<StoredEntry>, max_entries: usize) -> bool {
        let mut state = self.inner.lock();
        if state.session_id.as_deref() != Some(entry.summary.session_id.as_str()) {
            return false;
        }
        state.body_bytes += entry.body_bytes();
        state.entries.push_back(entry);
        while state.entries.len() > max_entries.max(1)
            || state.body_bytes > MEMORY_BODY_BUDGET && state.entries.len() > 1
        {
            if let Some(evicted) = state.entries.pop_front() {
                state.body_bytes = state.body_bytes.saturating_sub(evicted.body_bytes());
            }
        }
        true
    }

    pub fn summaries(&self) -> Vec<ProxyEntrySummary> {
        self.inner
            .lock()
            .entries
            .iter()
            .map(|entry| entry.summary.clone())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<StoredEntry>> {
        self.inner
            .lock()
            .entries
            .iter()
            .find(|entry| entry.summary.id == id)
            .cloned()
    }

    pub fn clear(&self) -> usize {
        let mut state = self.inner.lock();
        let count = state.entries.len();
        state.entries.clear();
        state.body_bytes = 0;
        count
    }
}

const COLUMNS: &str = "id, session_id, captured_at, method, url, scheme, host, port, path,
    status_code, status_text, content_type, request_body_size, response_body_size,
    duration_ms, ttfb_ms, mode, decrypted, error, note";

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyEntrySummary> {
    let status_code: Option<i64> = row.get(9)?;
    let ttfb: Option<i64> = row.get(15)?;
    let decrypted: i64 = row.get(17)?;
    Ok(ProxyEntrySummary {
        id: row.get(0)?,
        session_id: row.get(1)?,
        captured_at: row.get(2)?,
        method: row.get(3)?,
        url: row.get(4)?,
        scheme: row.get(5)?,
        host: row.get(6)?,
        port: row.get::<_, i64>(7)? as u16,
        path: row.get(8)?,
        status_code: status_code.map(|code| code as u16),
        status_text: row.get(10)?,
        content_type: row.get(11)?,
        request_body_size: row.get::<_, i64>(12)? as u64,
        response_body_size: row.get::<_, i64>(13)? as u64,
        duration_ms: row.get::<_, i64>(14)? as u64,
        ttfb_ms: ttfb.map(|value| value as u64),
        mode: row.get(16)?,
        decrypted: decrypted != 0,
        error: row.get(18)?,
        note: row.get(19)?,
    })
}

/// Writes `session` (if missing) and `entry`, then trims the session to `max_entries`.
pub fn persist_entry(
    db: &DbState,
    session: &SessionRecord,
    entry: &StoredEntry,
    max_entries: usize,
) -> AppResult<()> {
    let summary = &entry.summary;
    let request_body_path = write_body(&summary.id, "req", &entry.request_body.bytes)?;
    let response_body_path = write_body(&summary.id, "res", &entry.response_body.bytes)?;
    let request_headers = serde_json::to_string(&entry.request_headers)?;
    let response_headers = serde_json::to_string(&entry.response_headers)?;

    let outcome = db.with_conn(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO proxy_sessions (id, workspace_path, port, started_at, stopped_at)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![
                session.id,
                session.workspace_path,
                session.port as i64,
                session.started_at
            ],
        )?;
        conn.execute(
            "INSERT INTO proxy_entries (
                id, session_id, captured_at, method, url, scheme, host, port, path,
                status_code, status_text, content_type, mode, decrypted, error, note,
                request_headers, response_headers, request_body_size, response_body_size,
                request_body_path, response_body_path, duration_ms, ttfb_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                       ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                summary.id,
                summary.session_id,
                summary.captured_at,
                summary.method,
                summary.url,
                summary.scheme,
                summary.host,
                summary.port as i64,
                summary.path,
                summary.status_code.map(|code| code as i64),
                summary.status_text,
                summary.content_type,
                summary.mode,
                summary.decrypted as i64,
                summary.error,
                summary.note,
                request_headers,
                response_headers,
                summary.request_body_size as i64,
                summary.response_body_size as i64,
                request_body_path,
                response_body_path,
                summary.duration_ms as i64,
                summary.ttfb_ms.map(|value| value as i64),
            ],
        )?;

        // Entries beyond the newest `max_entries` are dropped together with their body files.
        let mut stmt = conn.prepare(
            "SELECT id, request_body_path, response_body_path FROM proxy_entries
             WHERE session_id = ?1
             ORDER BY captured_at DESC, rowid DESC
             LIMIT -1 OFFSET ?2",
        )?;
        let stale = stmt
            .query_map(params![session.id, max_entries.max(1) as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, _, _) in &stale {
            conn.execute("DELETE FROM proxy_entries WHERE id = ?1", params![id])?;
        }
        Ok(stale)
    });

    match outcome {
        Ok(stale) => {
            for (_, request_path, response_path) in stale {
                remove_body_files(request_path, response_path);
            }
            Ok(())
        }
        Err(error) => {
            remove_body_files(request_body_path, response_body_path);
            Err(error)
        }
    }
}

/// Records a session row up front so that sessions without traffic are still listed.
pub fn create_session(db: &DbState, session: &SessionRecord) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO proxy_sessions (id, workspace_path, port, started_at, stopped_at)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![
                session.id,
                session.workspace_path,
                session.port as i64,
                session.started_at
            ],
        )?;
        Ok(())
    })
}

pub fn stop_session(db: &DbState, session_id: &str, stopped_at: i64) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE proxy_sessions SET stopped_at = ?2 WHERE id = ?1",
            params![session_id, stopped_at],
        )?;
        Ok(())
    })
}

pub fn list_sessions(db: &DbState, limit: i64) -> AppResult<Vec<ProxySessionSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.workspace_path, s.port, s.started_at, s.stopped_at,
                    (SELECT COUNT(*) FROM proxy_entries e WHERE e.session_id = s.id)
             FROM proxy_sessions s
             ORDER BY s.started_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ProxySessionSummary {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                port: row.get::<_, i64>(2)? as u16,
                started_at: row.get(3)?,
                stopped_at: row.get(4)?,
                entry_count: row.get::<_, i64>(5)? as u64,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

pub fn list_entries(db: &DbState, session_id: &str) -> AppResult<Vec<ProxyEntrySummary>> {
    db.with_conn(|conn| {
        let sql = format!(
            "SELECT {COLUMNS} FROM proxy_entries WHERE session_id = ?1
             ORDER BY captured_at ASC, rowid ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![session_id, LIST_LIMIT], summary_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

pub fn get_entry(db: &DbState, id: &str) -> AppResult<Option<StoredEntry>> {
    let row = db.with_conn(|conn| {
        let sql = format!(
            "SELECT {COLUMNS}, request_headers, response_headers, request_body_path,
                    response_body_path
             FROM proxy_entries WHERE id = ?1"
        );
        Ok(conn
            .query_row(&sql, params![id], |row| {
                Ok((
                    summary_from_row(row)?,
                    row.get::<_, String>(20)?,
                    row.get::<_, String>(21)?,
                    row.get::<_, Option<String>>(22)?,
                    row.get::<_, Option<String>>(23)?,
                ))
            })
            .optional()?)
    })?;

    let Some((summary, request_headers, response_headers, request_path, response_path)) = row
    else {
        return Ok(None);
    };
    let request_total = summary.request_body_size;
    let response_total = summary.response_body_size;
    Ok(Some(StoredEntry {
        summary,
        request_headers: serde_json::from_str(&request_headers).unwrap_or_default(),
        response_headers: serde_json::from_str(&response_headers).unwrap_or_default(),
        request_body: read_body(request_path.as_deref(), request_total),
        response_body: read_body(response_path.as_deref(), response_total),
    }))
}

/// Deletes all persisted entries of a session (keeps the session row).
pub fn clear_entries(db: &DbState, session_id: &str) -> AppResult<u64> {
    let paths = body_paths(
        db,
        "SELECT request_body_path, response_body_path FROM proxy_entries WHERE session_id = ?1",
        session_id,
    )?;
    let deleted = db.with_conn(|conn| {
        Ok(conn.execute(
            "DELETE FROM proxy_entries WHERE session_id = ?1",
            params![session_id],
        )? as u64)
    })?;
    for (request_path, response_path) in paths {
        remove_body_files(request_path, response_path);
    }
    Ok(deleted)
}

/// Deletes a persisted session with its entries and body files.
pub fn delete_session(db: &DbState, session_id: &str) -> AppResult<()> {
    let paths = body_paths(
        db,
        "SELECT request_body_path, response_body_path FROM proxy_entries WHERE session_id = ?1",
        session_id,
    )?;
    let removed = db.with_conn(|conn| {
        Ok(conn.execute(
            "DELETE FROM proxy_sessions WHERE id = ?1",
            params![session_id],
        )?)
    })?;
    if removed == 0 {
        return Err(AppError::NotFound(format!(
            "proxy session not found: {session_id}"
        )));
    }
    for (request_path, response_path) in paths {
        remove_body_files(request_path, response_path);
    }
    Ok(())
}

fn body_paths(
    db: &DbState,
    sql: &str,
    session_id: &str,
) -> AppResult<Vec<(Option<String>, Option<String>)>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

fn write_body(entry_id: &str, suffix: &str, bytes: &[u8]) -> AppResult<Option<String>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let path = proxy_bodies_dir().join(format!("{entry_id}.{suffix}"));
    std::fs::write(&path, bytes)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Reads a stored body. The file holds only the captured prefix, so `total` comes from the row.
fn read_body(path: Option<&str>, total: u64) -> CapturedBody {
    let Some(path) = path else {
        return CapturedBody::default();
    };
    match std::fs::read(path) {
        Ok(bytes) => CapturedBody { bytes, total },
        Err(_) => CapturedBody::default(),
    }
}

fn remove_body_files(request_path: Option<String>, response_path: Option<String>) {
    for path in [request_path, response_path].into_iter().flatten() {
        let _ = std::fs::remove_file(PathBuf::from(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::capture::HeaderPair;

    fn sample(session: &str, id: &str, captured_at: i64, body: &[u8]) -> StoredEntry {
        StoredEntry {
            summary: ProxyEntrySummary {
                id: id.to_string(),
                session_id: session.to_string(),
                captured_at,
                method: "GET".into(),
                url: "http://example.test/a?page=1".into(),
                scheme: "http".into(),
                host: "example.test".into(),
                port: 80,
                path: "/a".into(),
                status_code: Some(200),
                status_text: Some("OK".into()),
                content_type: Some("text/plain".into()),
                request_body_size: 0,
                response_body_size: body.len() as u64,
                duration_ms: 3,
                ttfb_ms: Some(1),
                mode: "http".into(),
                decrypted: false,
                error: None,
                note: None,
            },
            request_headers: vec![HeaderPair {
                key: "accept".into(),
                value: "*/*".into(),
            }],
            response_headers: vec![],
            request_body: CapturedBody::default(),
            response_body: CapturedBody {
                bytes: body.to_vec(),
                total: body.len() as u64,
            },
        }
    }

    #[test]
    fn memory_store_rejects_entries_from_other_sessions() {
        let store = EntryStore::default();
        store.begin_session("s1");
        assert!(store.push(Arc::new(sample("s1", "e1", 1, b"x")), 10));
        assert!(!store.push(Arc::new(sample("s0", "old", 1, b"x")), 10));
        assert_eq!(store.summaries().len(), 1);
    }

    #[test]
    fn memory_store_evicts_oldest_beyond_limit() {
        let store = EntryStore::default();
        store.begin_session("s1");
        for index in 0..5 {
            store.push(Arc::new(sample("s1", &format!("e{index}"), index, b"x")), 3);
        }
        let ids: Vec<String> = store.summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["e2", "e3", "e4"]);
        assert!(store.get("e0").is_none());
        assert!(store.get("e4").is_some());
    }

    #[test]
    fn persisted_entries_round_trip_and_trim() {
        let dir = std::env::temp_dir().join(format!("thread-proxy-store-{}", crate::db::new_id()));
        let db = DbState::open(dir.join("thread.db")).unwrap();
        let session = SessionRecord {
            id: "sess-1".into(),
            workspace_path: None,
            port: 9090,
            started_at: 1,
        };

        for index in 0..4 {
            let entry = sample("sess-1", &format!("p{index}"), index, b"body-bytes");
            persist_entry(&db, &session, &entry, 2).unwrap();
        }

        let listed = list_entries(&db, "sess-1").unwrap();
        let ids: Vec<&str> = listed.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["p2", "p3"],
            "older rows beyond the limit are trimmed"
        );

        let restored = get_entry(&db, "p3").unwrap().expect("entry exists");
        assert_eq!(restored.response_body.bytes, b"body-bytes");
        assert_eq!(restored.request_headers[0].key, "accept");
        assert!(get_entry(&db, "p0").unwrap().is_none());

        let sessions = list_sessions(&db, 10).unwrap();
        assert_eq!(sessions[0].entry_count, 2);

        stop_session(&db, "sess-1", 9).unwrap();
        assert_eq!(list_sessions(&db, 10).unwrap()[0].stopped_at, Some(9));

        delete_session(&db, "sess-1").unwrap();
        assert!(list_entries(&db, "sess-1").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
