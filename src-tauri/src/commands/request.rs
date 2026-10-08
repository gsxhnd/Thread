//! Request CRUD commands.

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::db::trash_dir;
use crate::error::AppResult;
use crate::workspace::{RequestDetail, SharedWorkspace, ops};

use super::collection::TrashResult;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPathPayload {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestPayload {
    pub collection_path: String,
    pub name: String,
    pub protocol: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequestPayload {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequestConfigPayload {
    pub path: String,
    /// Full protocol document (protocol + name + protocol fields).
    pub document: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateRequestPayload {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[tauri::command]
pub async fn request_create(
    workspace: State<'_, SharedWorkspace>,
    payload: CreateRequestPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    let (rel, _) = ops::create_request(
        &root,
        &payload.collection_path,
        &payload.name,
        &payload.protocol,
    )?;
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &rel)
}

#[tauri::command]
pub async fn request_get(
    workspace: State<'_, SharedWorkspace>,
    payload: RequestPathPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &payload.path)
}

#[tauri::command]
pub async fn request_update(
    workspace: State<'_, SharedWorkspace>,
    payload: RenameRequestPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    ops::rename_request(&root, &payload.path, &payload.name)?;
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &payload.path)
}

/// Writes the protocol document back to the workspace file (atomic write).
#[tauri::command]
pub async fn request_update_config(
    workspace: State<'_, SharedWorkspace>,
    payload: UpdateRequestConfigPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    ops::update_request_document(&root, &payload.path, payload.document)?;
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &payload.path)
}

#[tauri::command]
pub async fn request_delete(
    workspace: State<'_, SharedWorkspace>,
    payload: RequestPathPayload,
) -> AppResult<TrashResult> {
    let root = workspace.root()?;
    let trashed = ops::delete_request(&root, &payload.path, &trash_dir())?;
    workspace.refresh()?;
    Ok(TrashResult {
        trashed_path: trashed,
    })
}

#[tauri::command]
pub async fn request_duplicate(
    workspace: State<'_, SharedWorkspace>,
    payload: DuplicateRequestPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    let (rel, _) = ops::duplicate_request(&root, &payload.path, payload.name)?;
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &rel)
}
