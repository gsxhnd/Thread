//! Request history commands (local data, never committed to Git).

use serde::Deserialize;
use tauri::State;

use crate::db::SharedDb;
use crate::error::AppResult;
use crate::services::history::{self, HistoryDetail, HistoryPage};
use crate::workspace::{RequestDetail, SharedWorkspace, ops};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListHistoryPayload {
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryIdPayload {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearHistoryPayload {
    #[serde(default)]
    pub before_timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveHistoryPayload {
    pub id: String,
    pub collection_path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearHistoryResult {
    pub deleted_count: i64,
}

#[tauri::command]
pub async fn history_list(
    workspace: State<'_, SharedWorkspace>,
    db: State<'_, SharedDb>,
    payload: ListHistoryPayload,
) -> AppResult<HistoryPage> {
    let root = workspace.root()?;
    history::list(
        &db,
        &root.to_string_lossy(),
        payload.protocol.as_deref(),
        payload.limit.unwrap_or(50),
        payload.offset.unwrap_or(0),
    )
}

#[tauri::command]
pub async fn history_get(
    db: State<'_, SharedDb>,
    payload: HistoryIdPayload,
) -> AppResult<HistoryDetail> {
    history::get(&db, &payload.id)
}

#[tauri::command]
pub async fn history_delete(db: State<'_, SharedDb>, payload: HistoryIdPayload) -> AppResult<()> {
    history::delete(&db, &payload.id)?;
    Ok(())
}

#[tauri::command]
pub async fn history_clear(
    workspace: State<'_, SharedWorkspace>,
    db: State<'_, SharedDb>,
    payload: ClearHistoryPayload,
) -> AppResult<ClearHistoryResult> {
    let root = workspace.root()?;
    let deleted_count = history::clear(&db, &root.to_string_lossy(), payload.before_timestamp)?;
    Ok(ClearHistoryResult { deleted_count })
}

/// Re-creates a workspace request file from a history snapshot.
#[tauri::command]
pub async fn history_save_to_collection(
    workspace: State<'_, SharedWorkspace>,
    db: State<'_, SharedDb>,
    payload: SaveHistoryPayload,
) -> AppResult<RequestDetail> {
    let root = workspace.root()?;
    let detail = history::get(&db, &payload.id)?;

    let snapshot = detail.request_snapshot;
    let protocol = snapshot
        .get("protocol")
        .and_then(|value| value.as_str())
        .unwrap_or("http")
        .to_string();
    let name = payload
        .name
        .or_else(|| {
            snapshot
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Restored request".to_string());

    let (rel, _) = ops::create_request(&root, &payload.collection_path, &name, &protocol)?;

    if let Some(document) = snapshot.get("document") {
        let mut document = document.clone();
        if let Some(object) = document.as_object_mut() {
            object.insert("name".into(), serde_json::Value::from(name.clone()));
        }
        ops::update_request_document(&root, &rel, document)?;
    }

    workspace.refresh()?;
    let index = workspace.index()?;
    ops::request_detail(&root, Some(&index), &rel)
}
