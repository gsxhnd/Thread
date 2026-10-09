//! Tauri application entry point and command registry.
//!
//! Thread / 灰线 —— local-first API 调试工具的 Rust 侧：
//! 工作区文件（集合/请求/环境）是真相，本地 SQLite 只存历史等运行数据。

mod atomic_file;
mod commands;
mod sync_mutex;

// Public so integration tests (`tests/`) can exercise the service layer directly.
pub mod config;
pub mod db;
pub mod error;
pub mod services;
pub mod workspace;

use std::sync::Arc;

use config::ThreadConfigStore;
use db::{DbState, db_path};
use services::graphql::SubscriptionRegistry;
use services::grpc::ProtoCatalog;
use services::http::HttpTaskRegistry;
use services::websocket::WsRegistry;
use workspace::WorkspaceState;

/// Applies the translucent vibrancy material to a macOS window.
#[cfg(target_os = "macos")]
pub(crate) fn apply_macos_window_effects(window: &tauri::WebviewWindow) {
    use tauri::window::{Effect, EffectState, EffectsBuilder};

    let _ = window.set_effects(Some(
        EffectsBuilder::new()
            .effect(Effect::WindowBackground)
            .state(EffectState::Active)
            .radius(10.0)
            .build(),
    ));
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_macos_window_effects(_window: &tauri::WebviewWindow) {}

fn install_crypto_provider() {
    // hyper、tonic 与 tungstenite 共用这一份 rustls。重复安装会失败，忽略即可。
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("thread=info,warn")),
        )
        .try_init();
}

/// Re-opens the workspace that was active when the app last exited.
fn reopen_last_workspace(app: &tauri::AppHandle) {
    use tauri::Manager;

    let store = app.state::<Arc<ThreadConfigStore>>();
    let workspace = app.state::<Arc<WorkspaceState>>();
    let Some(path) = store.session().last_workspace_path else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    if !path.is_dir() {
        return;
    }
    match workspace.open(&path) {
        Ok(snapshot) => tracing::info!(
            workspace = %path.display(),
            requests = snapshot.index.request_count,
            "reopened last workspace"
        ),
        Err(error) => tracing::warn!(?error, "failed to reopen last workspace"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_crypto_provider();
    init_tracing();

    let config_store = Arc::new(ThreadConfigStore::new());
    let db = Arc::new(DbState::open(db_path()).unwrap_or_else(|error| {
        panic!("failed to open database at {:?}: {error}", db_path());
    }));
    let workspace = Arc::new(WorkspaceState::new());
    let http_tasks = Arc::new(HttpTaskRegistry::new());
    let websockets = Arc::new(WsRegistry::new());
    let graphql_subscriptions = Arc::new(SubscriptionRegistry::new());
    let proto_catalog = Arc::new(ProtoCatalog::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(config_store.clone())
        .manage(db)
        .manage(workspace.clone())
        .manage(http_tasks)
        .manage(websockets)
        .manage(graphql_subscriptions)
        .manage(proto_catalog)
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            if let Some(window) = {
                use tauri::Manager;
                app.get_webview_window("main")
            } {
                apply_macos_window_effects(&window);
            }

            workspace::watcher::spawn(app.handle().clone(), workspace.clone());
            reopen_last_workspace(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // App
            commands::app_paths,
            config::get_config_load_warnings,
            config::load_config_sections,
            config::set_config_section,
            config::app_settings_get,
            config::app_settings_set,
            config::session_get,
            config::session_forget_workspace,
            // Workspace
            commands::workspace::workspace_create,
            commands::workspace::workspace_init,
            commands::workspace::workspace_open,
            commands::workspace::workspace_current,
            commands::workspace::workspace_close,
            commands::workspace::workspace_refresh,
            commands::workspace::workspace_update_settings,
            commands::workspace::workspace_search,
            // Collection
            commands::collection::collection_create,
            commands::collection::collection_get,
            commands::collection::collection_update,
            commands::collection::collection_delete,
            // Request
            commands::request::request_create,
            commands::request::request_get,
            commands::request::request_update,
            commands::request::request_update_config,
            commands::request::request_delete,
            commands::request::request_duplicate,
            // Environment
            commands::environment::environment_list,
            commands::environment::environment_get,
            commands::environment::environment_create,
            commands::environment::environment_update,
            commands::environment::environment_delete,
            commands::environment::environment_activate,
            commands::environment::env_variable_set,
            commands::environment::env_variable_delete,
            commands::environment::env_variable_reveal,
            commands::environment::variables_preview,
            // HTTP
            commands::http::http_send,
            commands::http::http_cancel,
            commands::http::http_active_tasks,
            // GraphQL
            commands::graphql::graphql_send,
            commands::graphql::graphql_introspect,
            commands::graphql::graphql_subscribe,
            commands::graphql::graphql_unsubscribe,
            // WebSocket
            commands::websocket::ws_connect,
            commands::websocket::ws_send,
            commands::websocket::ws_disconnect,
            commands::websocket::ws_list_connections,
            // gRPC
            commands::grpc::grpc_load_proto,
            commands::grpc::grpc_reflect,
            commands::grpc::grpc_invoke,
            commands::grpc::grpc_open_stream,
            commands::grpc::grpc_stream_send,
            commands::grpc::grpc_stream_close,
            commands::grpc::grpc_cancel_stream,
            // History
            commands::history::history_list,
            commands::history::history_get,
            commands::history::history_delete,
            commands::history::history_clear,
            commands::history::history_save_to_collection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
