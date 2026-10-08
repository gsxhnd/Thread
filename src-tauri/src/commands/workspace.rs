//! Workspace lifecycle commands: create / open / refresh / close.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::ThreadConfigStore;
use crate::error::{AppError, AppResult};
use crate::workspace::formats::{ProxyConfig, RequestSettings};
use crate::workspace::{SearchHit, SharedWorkspace, WorkspaceSnapshot, init_workspace};

pub const WORKSPACE_OPENED_EVENT: &str = "workspace://opened";
pub const WORKSPACE_CLOSED_EVENT: &str = "workspace://closed";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspacePayload {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenWorkspacePayload {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkspaceSettingsPayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub settings: Option<RequestSettings>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    #[serde(default)]
    pub active_environment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPayload {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn default_workspace_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Workspace".to_string())
}

fn record_recent(store: &ThreadConfigStore, path: &std::path::Path, name: &str) -> AppResult<()> {
    store
        .touch_recent_workspace(&path.to_string_lossy(), name)
        .map_err(AppError::Io)
}

/// Creates a new workspace directory (idempotent for existing files) and opens it.
#[tauri::command]
pub async fn workspace_create(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    store: State<'_, Arc<ThreadConfigStore>>,
    payload: CreateWorkspacePayload,
) -> AppResult<WorkspaceSnapshot> {
    let path = PathBuf::from(payload.path.trim());
    if path.as_os_str().is_empty() {
        return Err(AppError::Validation("workspace path is required".into()));
    }
    let name = payload
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| default_workspace_name(&path));

    init_workspace(&path, &name)?;
    let snapshot = workspace.open(&path)?;
    record_recent(&store, &path, &snapshot.index.name)?;

    let _ = app.emit(
        WORKSPACE_OPENED_EVENT,
        serde_json::json!({
            "path": path.to_string_lossy(),
            "name": snapshot.index.name,
        }),
    );
    Ok(snapshot)
}

/// Adopts an existing folder as a workspace by adding the missing metadata files.
#[tauri::command]
pub async fn workspace_init(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    store: State<'_, Arc<ThreadConfigStore>>,
    payload: CreateWorkspacePayload,
) -> AppResult<WorkspaceSnapshot> {
    let path = PathBuf::from(payload.path.trim());
    if !path.is_dir() {
        return Err(AppError::NotFound(format!(
            "folder does not exist: {}",
            path.display()
        )));
    }
    let name = payload
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| default_workspace_name(&path));

    init_workspace(&path, &name)?;
    let snapshot = workspace.open(&path)?;
    record_recent(&store, &path, &snapshot.index.name)?;
    let _ = app.emit(
        WORKSPACE_OPENED_EVENT,
        serde_json::json!({
            "path": path.to_string_lossy(),
            "name": snapshot.index.name,
        }),
    );
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_open(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    store: State<'_, Arc<ThreadConfigStore>>,
    payload: OpenWorkspacePayload,
) -> AppResult<WorkspaceSnapshot> {
    let path = PathBuf::from(payload.path.trim());
    let snapshot = workspace.open(&path)?;
    record_recent(&store, &path, &snapshot.index.name)?;
    let _ = app.emit(
        WORKSPACE_OPENED_EVENT,
        serde_json::json!({
            "path": path.to_string_lossy(),
            "name": snapshot.index.name,
        }),
    );
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_current(
    workspace: State<'_, SharedWorkspace>,
) -> AppResult<Option<WorkspaceSnapshot>> {
    if !workspace.is_open() {
        return Ok(None);
    }
    workspace.snapshot().map(Some)
}

#[tauri::command]
pub async fn workspace_close(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
) -> AppResult<()> {
    workspace.close();
    let _ = app.emit(WORKSPACE_CLOSED_EVENT, serde_json::json!({}));
    Ok(())
}

/// Re-scans the workspace (external edits, `git pull`, branch switch).
#[tauri::command]
pub async fn workspace_refresh(
    workspace: State<'_, SharedWorkspace>,
) -> AppResult<WorkspaceSnapshot> {
    workspace.refresh()?;
    workspace.snapshot()
}

#[tauri::command]
pub async fn workspace_update_settings(
    workspace: State<'_, SharedWorkspace>,
    payload: UpdateWorkspaceSettingsPayload,
) -> AppResult<WorkspaceSnapshot> {
    if let Some(name) = payload.name.as_deref() {
        crate::db::require_non_empty("workspace name", name)?;
    }
    workspace.update_config(|config| {
        if let Some(name) = payload.name.clone() {
            config.name = name;
        }
        if let Some(settings) = payload.settings.clone() {
            config.settings = settings;
        }
        if let Some(proxy) = payload.proxy.clone() {
            config.proxy = proxy;
        }
        if let Some(active) = payload.active_environment.clone() {
            config.active_environment = if active.trim().is_empty() {
                None
            } else {
                Some(active)
            };
        }
    })
}

#[tauri::command]
pub async fn workspace_search(
    workspace: State<'_, SharedWorkspace>,
    payload: SearchPayload,
) -> AppResult<Vec<SearchHit>> {
    let index = workspace.index()?;
    let limit = payload.limit.unwrap_or(50).clamp(1, 200);
    Ok(index.search(&payload.query, limit))
}
