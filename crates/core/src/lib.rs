//! Thread / 灰线 —— 领域层。
//!
//! 工作区文件（集合/请求/环境）是真相，本地 SQLite 只存历史、代理会话等运行数据。
//!
//! 这一层不认识任何 UI 框架：`AppCore` 持有全部共享状态，界面层（GPUI）直接
//! 调用它的方法，没有 IPC 序列化边界。需要跨线程时走 [`AppCore::spawn`]，
//! 它把 Tokio 的 future 挂到核心自己的 runtime 上，并返回一个 oneshot，
//! 界面层在自己的异步任务里 await 即可，不阻塞渲染线程。

mod atomic_file;
mod sync_mutex;

pub mod config;
pub mod context;
pub mod db;
pub mod error;
pub mod services;
pub mod workspace;

use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};

use parking_lot::Mutex as ParkingMutex;
use serde::{Deserialize, Serialize};

use config::ThreadConfigStore;
use db::{DbState, bodies_dir, db_path};
use services::graphql::SubscriptionRegistry;
use services::grpc::ProtoCatalog;
use services::history::{self, NewHistoryEntry};
use services::http::{
    EffectiveSettings, HttpRequestConfig, HttpResponse, HttpTaskRegistry, SendOptions, execute,
};
use services::proxy::{ProxyEngine, ProxyEntrySummary, ProxyEventSink, ProxyStatus};
use services::websocket::WsRegistry;
use workspace::watcher;
use workspace::{
    SharedWorkspace, WorkspaceSnapshot, WorkspaceState, files, index::WorkspaceIndex, layout, ops,
};

/// A capture-proxy event forwarded from the engine's threads.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // 事件量小，不引入 Box 间接层
pub enum ProxyEvent {
    Entry(ProxyEntrySummary),
    Status(ProxyStatus),
}

/// Live protocol events (WebSocket, GraphQL subscription, gRPC stream).
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LiveEvent {
    WsMessage(services::websocket::WsMessageEvent),
    WsStatus(services::websocket::WsStatusEvent),
    Graphql(services::graphql::SubscriptionEvent),
    Grpc(services::grpc::StreamEvent),
}

/// Fan-out sink: turns the engine's `ProxyEventSink` callbacks into a queue the
/// UI can drain. Cloned senders keep working even if the UI restarts its loop.
pub struct ChannelProxySink {
    sender: std::sync::mpsc::Sender<ProxyEvent>,
}

impl ProxyEventSink for ChannelProxySink {
    fn entry(&self, entry: &ProxyEntrySummary) {
        let _ = self.sender.send(ProxyEvent::Entry(entry.clone()));
    }

    fn status(&self, status: &ProxyStatus) {
        let _ = self.sender.send(ProxyEvent::Status(status.clone()));
    }
}

/// Owns every piece of shared state the application needs.
pub struct AppCore {
    /// Tokio runtime hosting the blocking/IO work (HTTP, proxy, WS, gRPC).
    ///
    /// The UI runs on its own thread; all Tokio-bound futures must be spawned
    /// here rather than on the UI executor, which has no reactor.
    pub runtime: tokio::runtime::Runtime,
    pub config: Arc<ThreadConfigStore>,
    pub db: Arc<DbState>,
    pub workspace: Arc<WorkspaceState>,
    pub http_tasks: Arc<HttpTaskRegistry>,
    pub websockets: Arc<WsRegistry>,
    pub graphql: Arc<SubscriptionRegistry>,
    pub protos: Arc<ProtoCatalog>,
    pub proxy: Arc<ProxyEngine>,
    /// Receiving end for capture-proxy events. Drain it from the UI loop.
    proxy_events: ParkingMutex<std::sync::mpsc::Receiver<ProxyEvent>>,
    live_tx: std::sync::mpsc::Sender<LiveEvent>,
    live_rx: ParkingMutex<std::sync::mpsc::Receiver<LiveEvent>>,
}

impl AppCore {
    pub fn new() -> AppResult<Self> {
        install_crypto_provider();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("thread-io")
            .build()
            .map_err(|error| AppError::Internal(format!("tokio runtime: {error}")))?;

        let config = Arc::new(ThreadConfigStore::new());
        let db = Arc::new(DbState::open(db_path())?);
        let workspace = Arc::new(WorkspaceState::new());
        let http_tasks = Arc::new(HttpTaskRegistry::new());
        let io = runtime.handle().clone();
        let websockets = Arc::new(WsRegistry::new());
        let graphql = Arc::new(SubscriptionRegistry::new());
        let protos = Arc::new(ProtoCatalog::new());
        websockets.bind_runtime(io.clone());
        graphql.bind_runtime(io.clone());
        protos.bind_runtime(io);

        let (sender, receiver) = std::sync::mpsc::channel();
        let proxy = Arc::new(ProxyEngine::new(
            db::proxy_ca_dir(),
            db.clone(),
            Arc::new(ChannelProxySink { sender }),
        ));
        let (live_tx, live_rx) = std::sync::mpsc::channel();

        Ok(Self {
            runtime,
            config,
            db,
            workspace,
            http_tasks,
            websockets,
            graphql,
            protos,
            proxy,
            proxy_events: ParkingMutex::new(receiver),
            live_tx,
            live_rx: ParkingMutex::new(live_rx),
        })
    }

    /// Runs a Tokio-bound future on the core runtime, returning a oneshot the UI
    /// can await from its own executor.
    pub fn spawn<F, T>(&self, future: F) -> tokio::sync::oneshot::Receiver<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _ = sender.send(future.await);
        });
        receiver
    }

    /// Blocks the calling thread on a Tokio-bound future. Only for startup and
    /// shutdown paths — never call this from the UI thread.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// Sender for WebSocket, GraphQL subscription and gRPC stream events.
    pub fn live_sender(&self) -> std::sync::mpsc::Sender<LiveEvent> {
        self.live_tx.clone()
    }

    /// Drains live protocol events accumulated since the last call.
    pub fn take_live_events(&self) -> Vec<LiveEvent> {
        let mut events = Vec::new();
        if let Some(receiver) = self.live_rx.try_lock() {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        }
        events
    }

    /// Drains capture-proxy events accumulated since the last call.
    pub fn take_proxy_events(&self) -> Vec<ProxyEvent> {
        let mut events = Vec::new();
        if let Some(receiver) = self.proxy_events.try_lock() {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        }
        events
    }

    /// Starts polling the open workspace for external modifications.
    pub fn watch_workspace<F>(&self, on_changed: F)
    where
        F: Fn(&WorkspaceIndex) + Send + 'static,
    {
        let state: Arc<WorkspaceState> = self.workspace.clone();
        let _guard = self.runtime.enter();
        watcher::spawn(state, on_changed);
    }

    /// Re-opens the workspace that was active when the app last exited.
    pub fn reopen_last_workspace(&self) -> Option<WorkspaceIndex> {
        let path = self.config.session().last_workspace_path?;
        let path = std::path::PathBuf::from(path);
        if !path.is_dir() {
            return None;
        }
        match self.workspace.open(&path) {
            Ok(snapshot) => {
                tracing::info!(
                    workspace = %path.display(),
                    requests = snapshot.index.request_count,
                    "reopened last workspace"
                );
                Some(snapshot.index)
            }
            Err(error) => {
                tracing::warn!(?error, "failed to reopen last workspace");
                None
            }
        }
    }

    /// Absolute paths of the config file, data directory and database.
    pub fn paths(&self) -> config::AppPaths {
        config::app_data_paths()
    }

    // -----------------------------------------------------------------------
    // Workspace
    // -----------------------------------------------------------------------

    /// Opens an existing workspace and records it in the recent list.
    pub fn open_workspace(&self, root: &Path) -> AppResult<WorkspaceSnapshot> {
        let snapshot = self.workspace.open(root)?;
        let _ = self
            .config
            .touch_recent_workspace(&root.to_string_lossy(), &snapshot.index.name);
        Ok(snapshot)
    }

    /// Initialises a folder as a workspace (idempotent) and opens it.
    pub fn create_workspace(&self, root: &Path, name: &str) -> AppResult<WorkspaceSnapshot> {
        workspace::init_workspace(root, name)?;
        self.open_workspace(root)
    }

    /// Sets (or clears) the active environment in `thread.workspace.json`.
    pub fn set_active_environment(&self, identifier: Option<&str>) -> AppResult<WorkspaceSnapshot> {
        self.workspace.update_config(|config| {
            config.active_environment = identifier
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        })
    }

    /// Sets the workspace's capture-proxy port.
    pub fn set_proxy_port(&self, port: u16) -> AppResult<WorkspaceSnapshot> {
        self.workspace
            .update_config(|config| config.proxy.port = port)
    }

    // -----------------------------------------------------------------------
    // HTTP
    // -----------------------------------------------------------------------

    /// Sends an HTTP request: resolves variables, executes, records history.
    pub async fn send_http(&self, payload: HttpSendPayload) -> AppResult<HttpResponse> {
        if payload.task_id.trim().is_empty() {
            return Err(AppError::Validation("taskId is required".into()));
        }
        let root = self.workspace.root()?;
        let app_settings = self.config.app_settings();

        let (collection_auth, request_name, request_document) =
            match payload.request_path.as_deref() {
                Some(path) => match layout::resolve_inside(&root, path) {
                    Ok(absolute) => {
                        let (_, meta) = ops::nearest_collection(&root, path);
                        let document = files::read_json_value(&absolute).ok();
                        let name = document
                            .as_ref()
                            .and_then(|value| value.get("name"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string);
                        (meta.as_ref().map(|meta| meta.auth.clone()), name, document)
                    }
                    // A draft request has no file yet; that must not block sending.
                    Err(_) => (None, None, None),
                },
                None => (None, None, None),
            };

        let config_environment = self
            .workspace
            .config()
            .ok()
            .and_then(|config| config.active_environment);
        let identifier: Option<String> = payload
            .environment_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                config_environment
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            });
        let explicit = payload
            .environment_path
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let active_rel = match identifier {
            Some(value) => match ops::resolve_environment_rel(&root, &value) {
                Some(rel) => Some(rel),
                None if explicit => {
                    return Err(AppError::NotFound(format!(
                        "environment not found: {value}"
                    )));
                }
                None => None,
            },
            None => None,
        };
        let variables = ops::resolve_variables(&root, active_rel.as_deref())?;

        let cancel = self.http_tasks.register(&payload.task_id);
        let options = SendOptions {
            task_id: payload.task_id.clone(),
            config: payload.config.clone(),
            variables,
            collection_auth: collection_auth.filter(|auth| !auth.is_none_type()),
            settings: EffectiveSettings::resolve(&payload.config.settings, None, &app_settings),
            bodies_dir: bodies_dir(),
        };

        let outcome = execute(options, Some(cancel)).await;
        self.http_tasks.unregister(&payload.task_id);
        let (response, resolved) = outcome?;

        if payload.save_history {
            let _ = self.save_history(
                &root,
                &payload,
                &response,
                &resolved,
                request_name.as_deref(),
                request_document,
            );
        }

        Ok(response)
    }

    /// Aborts an in-flight request. Returns true when one was found.
    pub fn cancel_http(&self, task_id: &str) -> bool {
        self.http_tasks.cancel(task_id)
    }

    /// Ids of requests currently in flight.
    pub fn active_http_tasks(&self) -> Vec<String> {
        self.http_tasks.active()
    }

    #[allow(clippy::too_many_arguments)]
    fn save_history(
        &self,
        root: &Path,
        payload: &HttpSendPayload,
        response: &HttpResponse,
        resolved: &services::http::ResolvedRequest,
        request_name: Option<&str>,
        request_document: Option<serde_json::Value>,
    ) -> AppResult<String> {
        let name = request_name
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let document = request_document.unwrap_or_else(|| {
            serde_json::to_value(&payload.config).unwrap_or(serde_json::Value::Null)
        });

        let request_snapshot = serde_json::json!({
            "protocol": "http",
            "name": name.clone(),
            "document": document,
            "resolved": {
                "method": resolved.method,
                "url": resolved.url,
                "headers": resolved.headers,
                "bodyKind": resolved.body_kind,
                "bodyPreview": resolved.body_preview,
            }
        })
        .to_string();

        let response_snapshot = serde_json::json!({
            "statusCode": response.status_code,
            "statusText": response.status_text,
            "headers": response.headers,
            "contentType": response.content_type,
            "bodySize": response.body_size,
            "timing": response.timing,
            "redirectCount": response.redirect_count,
            "url": response.url,
            "unresolvedVariables": response.unresolved_variables,
            "body": response.body,
        })
        .to_string();

        let entry = NewHistoryEntry {
            workspace_path: root.to_string_lossy().into_owned(),
            request_file_path: payload.request_path.clone(),
            protocol: "http".into(),
            name,
            method: Some(resolved.method.clone()),
            url: Some(resolved.url.clone()),
            request_snapshot,
            response_snapshot: Some(response_snapshot),
            response_body_path: response.body_file_path.clone(),
            status_code: Some(response.status_code as i64),
            duration_ms: Some(response.timing.total_ms.round() as i64),
        };

        let id = history::insert(&self.db, &entry)?;
        let limit = self.config.app_settings().history_limit;
        if limit > 0 {
            let _ = history::trim(&self.db, &root.to_string_lossy(), limit);
        }
        Ok(id)
    }
}

/// One HTTP send: the request to run plus where it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpSendPayload {
    /// Generated by the UI so the in-flight request can be cancelled.
    pub task_id: String,
    /// Workspace-relative request file path when sending a saved request.
    #[serde(default)]
    pub request_path: Option<String>,
    pub config: HttpRequestConfig,
    #[serde(default)]
    pub environment_path: Option<String>,
    #[serde(default = "default_save_history")]
    pub save_history: bool,
}

fn default_save_history() -> bool {
    true
}

/// Installs the process-wide rustls provider shared by hyper, tonic and tungstenite.
fn install_crypto_provider() {
    // 重复安装会失败，忽略即可。
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Sets up `tracing`.
///
/// `RUST_LOG` overrides the filter. The default keeps dependencies at `warn`
/// and this process at `info`. The binary crate is `thread` (the package is
/// `thread-app`); a `thread_app` directive would not match it.
/// Events go to stderr and to a daily file under the application data
/// directory (`logs/thread.log.<date>`). The file is written synchronously
/// and flushed per event so a crash still leaves the lines on disk.
pub fn init_tracing() {
    use tracing_subscriber::prelude::*;

    let log_dir = db::log_dir();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("warn,thread=info,thread_core=info")
    });
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let file_layer = open_log_file(&log_dir).map(|writer| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(writer)
    });

    let init_result = match file_layer {
        Some(file_layer) => tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer)
            .try_init(),
        None => tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .try_init(),
    };
    if init_result.is_ok() {
        tracing::info!(log_dir = %log_dir.display(), "logging started");
    }
}

struct DailyLog(Mutex<tracing_appender::rolling::RollingFileAppender>);

struct DailyLogWriter<'a>(
    std::sync::MutexGuard<'a, tracing_appender::rolling::RollingFileAppender>,
);

impl std::io::Write for DailyLogWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let file = &mut *self.0;
        let written = file.write(buf)?;
        file.flush()?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        (&mut *self.0).flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for DailyLog {
    type Writer = DailyLogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        DailyLogWriter(self.0.lock().unwrap_or_else(|poison| poison.into_inner()))
    }
}

fn open_log_file(log_dir: &std::path::Path) -> Option<DailyLog> {
    let appender = tracing_appender::rolling::daily(log_dir, "thread.log");
    Some(DailyLog(Mutex::new(appender)))
}

pub use error::{AppError, AppResult};

/// Convenience alias so callers do not need to import the workspace module.
pub type SharedWorkspaceHandle = SharedWorkspace;

/// Guard type re-exported for tests that construct a core without a UI.
pub type MutexHandle<T> = Mutex<T>;
