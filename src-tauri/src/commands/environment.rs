//! Environment and variable commands.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::trash_dir;
use crate::error::{AppError, AppResult};
use crate::services::secrets;
use crate::services::variables::substitute;
use crate::workspace::{
    EnvironmentDetail, EnvironmentSummary, SharedWorkspace, WorkspaceSnapshot, ops,
};

use super::collection::TrashResult;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPathPayload {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentPayload {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironmentPayload {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVariablePayload {
    pub environment_path: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub secret: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVariablePayload {
    pub environment_path: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVariablesPayload {
    #[serde(default)]
    pub environment_path: Option<String>,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablePreview {
    pub resolved: String,
    pub missing: Vec<String>,
}

#[tauri::command]
pub async fn environment_list(
    workspace: State<'_, SharedWorkspace>,
) -> AppResult<Vec<EnvironmentSummary>> {
    Ok(workspace.index()?.environments)
}

#[tauri::command]
pub async fn environment_get(
    workspace: State<'_, SharedWorkspace>,
    payload: EnvironmentPathPayload,
) -> AppResult<EnvironmentDetail> {
    let root = workspace.root()?;
    let active = workspace.config()?.active_environment;
    ops::environment_detail(&root, &payload.path, active.as_deref())
}

#[tauri::command]
pub async fn environment_create(
    workspace: State<'_, SharedWorkspace>,
    payload: CreateEnvironmentPayload,
) -> AppResult<EnvironmentDetail> {
    let root = workspace.root()?;
    let (rel, _) = ops::create_environment(&root, &payload.name)?;
    workspace.refresh()?;
    let active = workspace.config()?.active_environment;
    ops::environment_detail(&root, &rel, active.as_deref())
}

#[tauri::command]
pub async fn environment_update(
    workspace: State<'_, SharedWorkspace>,
    payload: UpdateEnvironmentPayload,
) -> AppResult<EnvironmentDetail> {
    let root = workspace.root()?;
    ops::update_environment(&root, &payload.path, payload.name)?;
    workspace.refresh()?;
    let active = workspace.config()?.active_environment;
    ops::environment_detail(&root, &payload.path, active.as_deref())
}

#[tauri::command]
pub async fn environment_delete(
    workspace: State<'_, SharedWorkspace>,
    payload: EnvironmentPathPayload,
) -> AppResult<TrashResult> {
    let root = workspace.root()?;
    if payload.path.to_lowercase().ends_with("global.env.json") {
        return Err(AppError::Validation(
            "the global environment cannot be deleted".into(),
        ));
    }
    let trashed = ops::delete_environment(&root, &payload.path, &trash_dir())?;
    // Deactivating a deleted environment keeps the workspace consistent.
    if workspace.config()?.active_environment.as_deref() == Some(payload.path.as_str()) {
        workspace.update_config(|config| config.active_environment = None)?;
    }
    workspace.refresh()?;
    Ok(TrashResult {
        trashed_path: trashed,
    })
}

#[tauri::command]
pub async fn environment_activate(
    workspace: State<'_, SharedWorkspace>,
    payload: EnvironmentPathPayload,
) -> AppResult<WorkspaceSnapshot> {
    if payload.path.trim().is_empty() {
        return workspace.update_config(|config| {
            config.active_environment = None;
        });
    }
    let root = workspace.root()?;
    // Validate before writing the workspace file. Accept a path, stem, or name.
    let path = ops::resolve_environment_rel(&root, &payload.path)
        .ok_or_else(|| AppError::NotFound(format!("environment not found: {}", payload.path)))?;
    ops::read_environment(&root, &path)?;
    let snapshot = workspace.update_config(|config| {
        config.active_environment = Some(path);
    })?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn env_variable_set(
    workspace: State<'_, SharedWorkspace>,
    payload: SetVariablePayload,
) -> AppResult<EnvironmentDetail> {
    let root = workspace.root()?;
    ops::set_env_variable(
        &root,
        &payload.environment_path,
        &payload.key,
        payload.value,
        payload.enabled,
        payload.secret,
        payload.description,
    )?;
    workspace.refresh()?;
    let active = workspace.config()?.active_environment;
    ops::environment_detail(&root, &payload.environment_path, active.as_deref())
}

#[tauri::command]
pub async fn env_variable_delete(
    workspace: State<'_, SharedWorkspace>,
    payload: DeleteVariablePayload,
) -> AppResult<EnvironmentDetail> {
    let root = workspace.root()?;
    ops::delete_env_variable(&root, &payload.environment_path, &payload.key)?;
    workspace.refresh()?;
    let active = workspace.config()?.active_environment;
    ops::environment_detail(&root, &payload.environment_path, active.as_deref())
}

/// Explicit user action: reveals a stored secret value for inspection.
#[tauri::command]
pub async fn env_variable_reveal(
    workspace: State<'_, SharedWorkspace>,
    payload: DeleteVariablePayload,
) -> AppResult<Option<String>> {
    let root = workspace.root()?;
    Ok(secrets::get(&root, &payload.environment_path, &payload.key))
}

#[tauri::command]
pub async fn variables_preview(
    workspace: State<'_, SharedWorkspace>,
    payload: PreviewVariablesPayload,
) -> AppResult<VariablePreview> {
    let root = workspace.root()?;
    let config = workspace.config()?;
    let active = payload
        .environment_path
        .or(config.active_environment)
        .filter(|path| !path.trim().is_empty());
    let available = ops::resolve_variables(&root, active.as_deref())?;
    let mut missing = Vec::new();
    let resolved = substitute(&payload.text, &available, &mut missing);
    // Only the substituted text and the missing names cross the IPC boundary;
    // secret values never leave the Rust side.
    Ok(VariablePreview { resolved, missing })
}
