//! Basic capture proxy (Phase 3).
//!
//! The proxy listens on `127.0.0.1` only and handles three kinds of traffic:
//! - plain HTTP requests in absolute form, forwarded and recorded;
//! - `CONNECT` tunnels, relayed byte for byte and recorded as opaque entries;
//! - HTTPS interception, used only when the user has acknowledged trusting the local CA
//!   and the client accepts the presented certificate. Hosts whose certificate is
//!   rejected are relayed without inspection for the rest of the session.
//!
//! The proxy never changes system proxy settings; clients are configured by the user.
//! Services here do not depend on Tauri: UI updates go through [`ProxyEventSink`].

pub mod ca;
pub mod capture;
pub mod server;
pub mod settings;
pub mod store;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::db::{DbState, new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::services::proxy::ca::CaManager;
use crate::services::proxy::capture::{BodyView, HeaderPair};

use crate::services::proxy::store::{EntryStore, SessionRecord, StoredEntry};

pub use ca::CaInfo;
pub use settings::CaptureSettings;

/// One row of the traffic list. Sensitive values are already masked when `mask_sensitive` is on.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEntrySummary {
    pub id: String,
    pub session_id: String,
    pub captured_at: i64,
    pub method: String,
    pub url: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub status_code: Option<u16>,
    pub status_text: Option<String>,
    pub content_type: Option<String>,
    pub request_body_size: u64,
    pub response_body_size: u64,
    pub duration_ms: u64,
    pub ttfb_ms: Option<u64>,
    /// `http` (plain forward), `tunnel` (opaque CONNECT) or `mitm` (decrypted HTTPS).
    pub mode: String,
    pub decrypted: bool,
    pub error: Option<String>,
    pub note: Option<String>,
}

/// Full view of one entry, used by the detail pane.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEntryDetail {
    #[serde(flatten)]
    pub summary: ProxyEntrySummary,
    pub request_headers: Vec<HeaderPair>,
    pub response_headers: Vec<HeaderPair>,
    pub request_body: BodyView,
    pub response_body: BodyView,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub session_id: Option<String>,
    pub port: Option<u16>,
    pub paused: bool,
}

impl ProxyStatus {
    pub fn idle() -> Self {
        Self {
            running: false,
            session_id: None,
            port: None,
            paused: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxySessionSummary {
    pub id: String,
    pub workspace_path: Option<String>,
    pub port: u16,
    pub started_at: i64,
    pub stopped_at: Option<i64>,
    pub entry_count: u64,
}

/// Receives UI-facing notifications. The command layer turns these into Tauri events.
pub trait ProxyEventSink: Send + Sync {
    fn entry(&self, entry: &ProxyEntrySummary);
    fn status(&self, status: &ProxyStatus);
}

/// State shared by every connection of one proxy session.
pub struct Shared {
    pub record: SessionRecord,
    pub settings: CaptureSettings,
    pub store: EntryStore,
    pub paused: AtomicBool,
    /// Hosts whose certificate the client rejected during this session.
    pub bypass: Mutex<HashSet<String>>,
    pub ca: Arc<CaManager>,
    pub db: Arc<DbState>,
    pub sink: Arc<dyn ProxyEventSink>,
    pub shutdown: watch::Receiver<bool>,
}

impl Shared {
    /// HTTPS is decrypted only when the user enabled it and confirmed the CA is trusted.
    pub fn mitm_enabled(&self) -> bool {
        self.settings.decrypt_https && self.settings.ca_trust_acknowledged
    }

    pub fn recording(&self) -> bool {
        !self.paused.load(Ordering::Relaxed)
    }

    fn status(&self) -> ProxyStatus {
        ProxyStatus {
            running: true,
            session_id: Some(self.record.id.clone()),
            port: Some(self.record.port),
            paused: self.paused.load(Ordering::Relaxed),
        }
    }
}

struct Running {
    shared: Arc<Shared>,
    shutdown: watch::Sender<bool>,
    accept: JoinHandle<()>,
}

pub struct ProxyEngine {
    running: tokio::sync::Mutex<Option<Running>>,
    /// The most recent session, kept after stop so its traffic can still be inspected.
    latest: Mutex<Option<Arc<Shared>>>,
    ca: Arc<CaManager>,
    db: Arc<DbState>,
    sink: Arc<dyn ProxyEventSink>,
}

impl ProxyEngine {
    pub fn new(ca_dir: PathBuf, db: Arc<DbState>, sink: Arc<dyn ProxyEventSink>) -> Self {
        Self {
            running: tokio::sync::Mutex::new(None),
            latest: Mutex::new(None),
            ca: Arc::new(CaManager::new(ca_dir)),
            db,
            sink,
        }
    }

    pub fn ca(&self) -> &Arc<CaManager> {
        &self.ca
    }

    /// Binds `127.0.0.1:port` (port `0` picks a free port) and starts accepting clients.
    pub async fn start(
        &self,
        port: u16,
        workspace_path: Option<String>,
        settings: CaptureSettings,
    ) -> AppResult<ProxyStatus> {
        let mut running = self.running.lock().await;
        if running.is_some() {
            return Err(AppError::Proxy("proxy is already running".into()));
        }
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|error| {
                AppError::Proxy(format!("cannot listen on 127.0.0.1:{port}: {error}"))
            })?;
        let bound = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(port);

        let settings = settings.normalized();
        let record = SessionRecord {
            id: new_id(),
            workspace_path,
            port: bound,
            started_at: now_ms(),
        };
        if settings.persist_sessions {
            store::create_session(&self.db, &record)?;
        }

        let store = EntryStore::default();
        store.begin_session(&record.id);
        let (shutdown, shutdown_rx) = watch::channel(false);
        let shared = Arc::new(Shared {
            record,
            settings,
            store,
            paused: AtomicBool::new(false),
            bypass: Mutex::new(HashSet::new()),
            ca: Arc::clone(&self.ca),
            db: Arc::clone(&self.db),
            sink: Arc::clone(&self.sink),
            shutdown: shutdown_rx,
        });

        let accept = tokio::spawn(server::accept_loop(listener, Arc::clone(&shared)));
        *self.latest.lock() = Some(Arc::clone(&shared));
        let status = shared.status();
        *running = Some(Running {
            shared,
            shutdown,
            accept,
        });
        drop(running);

        self.sink.status(&status);
        Ok(status)
    }

    /// Stops accepting and closes open connections. Safe to call when already stopped.
    pub async fn stop(&self) -> ProxyStatus {
        let taken = self.running.lock().await.take();
        let status = match taken {
            Some(running) => {
                let _ = running.shutdown.send(true);
                running.accept.abort();
                if running.shared.settings.persist_sessions
                    && let Err(error) =
                        store::stop_session(&self.db, &running.shared.record.id, now_ms())
                {
                    tracing::warn!(?error, "failed to mark proxy session as stopped");
                }
                ProxyStatus::idle()
            }
            None => ProxyStatus::idle(),
        };
        self.sink.status(&status);
        status
    }

    pub async fn status(&self) -> ProxyStatus {
        match self.running.lock().await.as_ref() {
            Some(running) => running.shared.status(),
            None => ProxyStatus::idle(),
        }
    }

    /// Pauses or resumes recording. Traffic keeps flowing while paused; nothing is recorded.
    pub async fn set_paused(&self, paused: bool) -> AppResult<ProxyStatus> {
        let guard = self.running.lock().await;
        let running = guard
            .as_ref()
            .ok_or_else(|| AppError::Validation("proxy is not running".into()))?;
        running.shared.paused.store(paused, Ordering::Relaxed);
        let status = running.shared.status();
        drop(guard);
        self.sink.status(&status);
        Ok(status)
    }

    /// Entries of `session_id` (or the latest session). Memory is used for the latest
    /// session; older sessions come from the database when they were persisted.
    pub fn entries(&self, session_id: Option<&str>) -> AppResult<Vec<ProxyEntrySummary>> {
        let latest = self.latest.lock().clone();
        match (latest, session_id) {
            (Some(shared), None) => Ok(shared.store.summaries()),
            (Some(shared), Some(id)) if shared.record.id == id => Ok(shared.store.summaries()),
            (_, Some(id)) => store::list_entries(&self.db, id),
            (None, None) => Ok(Vec::new()),
        }
    }

    pub fn entry(&self, id: &str) -> AppResult<Option<ProxyEntryDetail>> {
        if let Some(shared) = self.latest.lock().clone()
            && let Some(entry) = shared.store.get(id)
        {
            return Ok(Some(entry.detail()));
        }
        Ok(store::get_entry(&self.db, id)?.map(|entry| entry.detail()))
    }

    /// Removes captured traffic of a session without touching the session itself.
    pub fn clear(&self, session_id: &str) -> AppResult<()> {
        if let Some(shared) = self.latest.lock().clone()
            && shared.record.id == session_id
        {
            shared.store.clear();
        }
        store::clear_entries(&self.db, session_id)?;
        Ok(())
    }

    pub fn sessions(&self) -> AppResult<Vec<ProxySessionSummary>> {
        store::list_sessions(&self.db, 200)
    }

    /// Deletes a persisted session. The active session cannot be deleted while running.
    pub async fn delete_session(&self, session_id: &str) -> AppResult<()> {
        let active = self
            .running
            .lock()
            .await
            .as_ref()
            .is_some_and(|running| running.shared.record.id == session_id);
        if active {
            return Err(AppError::Validation(
                "stop the proxy before deleting its active session".into(),
            ));
        }
        if let Some(shared) = self.latest.lock().clone()
            && shared.record.id == session_id
        {
            shared.store.clear();
        }
        store::delete_session(&self.db, session_id)
    }
}

/// Whether a captured row should stay visible for the proxy list filters.
///
/// `status` accepts an exact code (`404`), a class (`2` → 2xx), or `err` for failed rows.
pub fn matches_traffic_filter(
    entry: &ProxyEntrySummary,
    host: &str,
    method: &str,
    status: &str,
) -> bool {
    let host = host.trim();
    if !host.is_empty()
        && !entry
            .host
            .to_ascii_lowercase()
            .contains(&host.to_ascii_lowercase())
    {
        return false;
    }
    let method = method.trim();
    if !method.is_empty() && !entry.method.eq_ignore_ascii_case(method) {
        return false;
    }
    let status = status.trim();
    if status.is_empty() {
        return true;
    }
    if status.eq_ignore_ascii_case("err") || status.eq_ignore_ascii_case("error") {
        return entry.error.is_some() || entry.status_code.is_none();
    }
    let Ok(code) = status.parse::<u16>() else {
        return false;
    };
    match entry.status_code {
        Some(actual) if status.len() <= 1 => actual / 100 == code,
        Some(actual) => actual == code,
        None => false,
    }
}

/// Publishes a finished entry: memory, optional database, then the UI sink.
pub(crate) fn publish(shared: &Shared, entry: StoredEntry) {
    let summary = entry.summary.clone();
    if shared.settings.persist_sessions
        && let Err(error) = store::persist_entry(
            &shared.db,
            &shared.record,
            &entry,
            shared.settings.max_entries,
        )
    {
        tracing::warn!(?error, "failed to persist proxy entry");
    }
    shared
        .store
        .push(Arc::new(entry), shared.settings.max_entries);
    shared.sink.entry(&summary);
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    fn entry(
        method: &str,
        host: &str,
        status: Option<u16>,
        error: Option<&str>,
    ) -> ProxyEntrySummary {
        ProxyEntrySummary {
            id: "1".into(),
            session_id: "s".into(),
            captured_at: 0,
            method: method.into(),
            url: format!("http://{host}/"),
            scheme: "http".into(),
            host: host.into(),
            port: 80,
            path: "/".into(),
            status_code: status,
            status_text: None,
            content_type: None,
            request_body_size: 0,
            response_body_size: 0,
            duration_ms: 0,
            ttfb_ms: None,
            mode: "http".into(),
            decrypted: false,
            error: error.map(str::to_string),
            note: None,
        }
    }

    #[test]
    fn filters_by_host_method_and_status_class() {
        let row = entry("GET", "api.example", Some(204), None);
        assert!(matches_traffic_filter(&row, "example", "get", "2"));
        assert!(!matches_traffic_filter(&row, "other", "GET", ""));
        assert!(!matches_traffic_filter(&row, "", "POST", ""));
        assert!(matches_traffic_filter(
            &entry("POST", "h", None, Some("reset")),
            "",
            "",
            "err"
        ));
        assert!(matches_traffic_filter(&row, "", "", "204"));
    }
}
