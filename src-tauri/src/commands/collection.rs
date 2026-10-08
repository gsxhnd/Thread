//! Collection (folder) commands.

use serde::Deserialize;
use tauri::State;

use crate::db::trash_dir;
use crate::error::AppResult;
use crate::workspace::formats::{AuthSpec, KeyValue};
use crate::workspace::{CollectionDetail, SharedWorkspace, ops};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionPayload {
    #[serde(default)]
    pub parent_path: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollectionPayload {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub auth: Option<AuthSpec>,
    #[serde(default)]
    pub headers: Option<Vec<KeyValue>>,
    #[serde(default)]
    pub pre_script: Option<String>,
    #[serde(default)]
    pub post_script: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPathPayload {
    pub path: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    pub trashed_path: String,
}

#[tauri::command]
pub async fn collection_create(
    workspace: State<'_, SharedWorkspace>,
    payload: CreateCollectionPayload,
) -> AppResult<CollectionDetail> {
    let root = workspace.root()?;
    let (rel, _) = ops::create_collection(
        &root,
        payload.parent_path.as_deref(),
        &payload.name,
        payload.description,
    )?;
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::collection_detail(&root, &rel, Some(&index))
}

#[tauri::command]
pub async fn collection_get(
    workspace: State<'_, SharedWorkspace>,
    payload: CollectionPathPayload,
) -> AppResult<CollectionDetail> {
    let root = workspace.root()?;
    let index = workspace.index()?;
    ops::collection_detail(&root, &payload.path, Some(&index))
}

#[tauri::command]
pub async fn collection_update(
    workspace: State<'_, SharedWorkspace>,
    payload: UpdateCollectionPayload,
) -> AppResult<CollectionDetail> {
    let root = workspace.root()?;
    let meta = ops::update_collection(
        &root,
        &payload.path,
        payload.name.clone(),
        payload.description.clone(),
    )?;
    if payload.auth.is_some()
        || payload.headers.is_some()
        || payload.pre_script.is_some()
        || payload.post_script.is_some()
    {
        let mut updated = meta;
        if let Some(auth) = payload.auth {
            updated.auth = auth;
        }
        if let Some(headers) = payload.headers {
            updated.headers = headers;
        }
        if let Some(script) = payload.pre_script {
            updated.pre_script = script;
        }
        if let Some(script) = payload.post_script {
            updated.post_script = script;
        }
        let path = crate::workspace::layout::resolve_inside(&root, &payload.path)?
            .join(crate::workspace::layout::COLLECTION_FILE);
        crate::workspace::files::write_json(&path, &updated)?;
    }
    workspace.refresh()?;
    let index = workspace.index()?;
    ops::collection_detail(&root, &payload.path, Some(&index))
}

#[tauri::command]
pub async fn collection_delete(
    workspace: State<'_, SharedWorkspace>,
    payload: CollectionPathPayload,
) -> AppResult<TrashResult> {
    let root = workspace.root()?;
    let trashed = ops::delete_collection(&root, &payload.path, &trash_dir())?;
    workspace.refresh()?;
    Ok(TrashResult {
        trashed_path: trashed,
    })
}
