//! WebSocket commands. Frames are emitted as `ws://message` and `ws://status`.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::{AppError, AppResult};
use crate::services::variables::substitute;
use crate::services::websocket::{self, WsConnect, WsOutgoing, WsRegistry};
use crate::workspace::SharedWorkspace;
use crate::workspace::formats::KeyValue;

use super::context;
use crate::config::ThreadConfigStore;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectPayload {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub environment_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsSendPayload {
    pub connection_id: String,
    pub message: String,
    #[serde(default)]
    pub is_binary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsDisconnectPayload {
    pub connection_id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnected {
    pub connection_id: String,
}

#[tauri::command]
pub async fn ws_connect(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    registry: State<'_, Arc<WsRegistry>>,
    payload: WsConnectPayload,
) -> AppResult<WsConnected> {
    let call = context::prepare(
        &workspace,
        &config_store,
        None,
        payload.environment_path.as_deref(),
    )?;
    let mut missing = Vec::new();
    let url = substitute(&payload.url, &call.variables, &mut missing);
    if !missing.is_empty() {
        return Err(AppError::Validation(format!(
            "unresolved variables: {}",
            missing.join(", ")
        )));
    }
    let mut headers = Vec::new();
    for row in payload.headers {
        if !row.enabled || row.key.trim().is_empty() {
            continue;
        }
        headers.push((
            row.key,
            substitute(&row.value, &call.variables, &mut missing),
        ));
    }
    if !missing.is_empty() {
        return Err(AppError::Validation(format!(
            "unresolved variables: {}",
            missing.join(", ")
        )));
    }
    let message_app = app.clone();
    let status_app = app.clone();
    let id = registry.connect(
        WsConnect {
            url,
            headers,
            protocols: payload
                .protocols
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect(),
            auto_reconnect: payload.auto_reconnect,
        },
        websocket::WsSink::new(
            move |event| {
                let _ = message_app.emit("ws://message", &event);
            },
            move |event| {
                let _ = status_app.emit("ws://status", &event);
            },
        ),
    )?;
    Ok(WsConnected { connection_id: id })
}

#[tauri::command]
pub async fn ws_send(
    registry: State<'_, Arc<WsRegistry>>,
    payload: WsSendPayload,
) -> AppResult<()> {
    let message = if payload.is_binary {
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            payload.message.trim(),
        )
        .map_err(|error| AppError::Validation(format!("binary payload must be base64: {error}")))?;
        WsOutgoing::Binary(bytes)
    } else {
        WsOutgoing::Text(payload.message)
    };
    registry.send(&payload.connection_id, message)
}

#[tauri::command]
pub async fn ws_disconnect(
    registry: State<'_, Arc<WsRegistry>>,
    payload: WsDisconnectPayload,
) -> AppResult<()> {
    registry.disconnect(&payload.connection_id);
    Ok(())
}

#[tauri::command]
pub async fn ws_list_connections(
    registry: State<'_, Arc<WsRegistry>>,
) -> AppResult<Vec<websocket::WsConnectionInfo>> {
    Ok(registry.list())
}
