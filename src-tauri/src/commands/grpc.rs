//! gRPC commands: proto load, reflection, unary, and streaming.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use crate::config::ThreadConfigStore;
use crate::db::SharedDb;
use crate::error::AppResult;
use crate::services::grpc::{GrpcInvoke, ProtoCatalog, StreamEvent};
use crate::services::history::{self, NewHistoryEntry};
use crate::services::variables::substitute;
use crate::workspace::SharedWorkspace;
use crate::workspace::formats::KeyValue;

use super::context;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcLoadProtoPayload {
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcReflectPayload {
    pub host: String,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default)]
    pub metadata: Vec<KeyValue>,
    #[serde(default)]
    pub environment_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcInvokePayload {
    pub host: String,
    #[serde(default)]
    pub tls_enabled: bool,
    pub pool_id: String,
    pub service: String,
    pub method: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub metadata: Vec<KeyValue>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub request_path: Option<String>,
    #[serde(default)]
    pub environment_path: Option<String>,
    #[serde(default = "default_true")]
    pub save_history: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStreamSendPayload {
    pub stream_id: String,
    pub payload: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStreamIdPayload {
    pub stream_id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStreamStarted {
    pub stream_id: String,
}

fn invoke_from(payload: &GrpcInvokePayload, call: &context::CallContext) -> AppResult<GrpcInvoke> {
    let mut missing = Vec::new();
    let host = substitute(&payload.host, &call.variables, &mut missing);
    let service = substitute(&payload.service, &call.variables, &mut missing);
    let method = substitute(&payload.method, &call.variables, &mut missing);
    let body = substitute(&payload.payload, &call.variables, &mut missing);
    let mut metadata = Vec::new();
    for row in &payload.metadata {
        if !row.enabled || row.key.trim().is_empty() {
            continue;
        }
        metadata.push((
            row.key.clone(),
            substitute(&row.value, &call.variables, &mut missing),
        ));
    }
    if !missing.is_empty() {
        return Err(crate::error::AppError::Validation(format!(
            "unresolved variables: {}",
            missing.join(", ")
        )));
    }
    Ok(GrpcInvoke {
        host,
        tls: payload.tls_enabled,
        metadata,
        pool_id: payload.pool_id.clone(),
        service,
        method,
        payload: body,
        timeout: Duration::from_millis(payload.timeout_ms.unwrap_or(call.timeout_ms).max(1)),
    })
}

#[tauri::command]
pub async fn grpc_load_proto(
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcLoadProtoPayload,
) -> AppResult<crate::services::grpc::ProtoDefinition> {
    catalog.load_file(PathBuf::from(payload.file_path).as_path())
}

#[tauri::command]
pub async fn grpc_reflect(
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcReflectPayload,
) -> AppResult<crate::services::grpc::ProtoDefinition> {
    let call = context::prepare(
        &workspace,
        &config_store,
        None,
        payload.environment_path.as_deref(),
    )?;
    let mut missing = Vec::new();
    let host = substitute(&payload.host, &call.variables, &mut missing);
    if !missing.is_empty() {
        return Err(crate::error::AppError::Validation(format!(
            "unresolved variables: {}",
            missing.join(", ")
        )));
    }
    let mut metadata = Vec::new();
    for row in &payload.metadata {
        if !row.enabled || row.key.trim().is_empty() {
            continue;
        }
        metadata.push((
            row.key.clone(),
            substitute(&row.value, &call.variables, &mut missing),
        ));
    }
    if !missing.is_empty() {
        return Err(crate::error::AppError::Validation(format!(
            "unresolved variables: {}",
            missing.join(", ")
        )));
    }
    catalog.reflect(&host, payload.tls_enabled, &metadata).await
}

#[tauri::command]
pub async fn grpc_invoke(
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    db: State<'_, SharedDb>,
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcInvokePayload,
) -> AppResult<crate::services::grpc::GrpcResult> {
    let call = context::prepare(
        &workspace,
        &config_store,
        payload.request_path.as_deref(),
        payload.environment_path.as_deref(),
    )?;
    let request = invoke_from(&payload, &call)?;
    let result = catalog.unary(&request).await?;
    if payload.save_history {
        let _ = record_history(&call, &payload, &result, db.inner());
    }
    Ok(result)
}

#[tauri::command]
pub async fn grpc_open_stream(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcInvokePayload,
) -> AppResult<GrpcStreamStarted> {
    let call = context::prepare(
        &workspace,
        &config_store,
        payload.request_path.as_deref(),
        payload.environment_path.as_deref(),
    )?;
    let request = invoke_from(&payload, &call)?;
    let catalog = Arc::clone(catalog.inner());
    let app_emit = app.clone();
    let stream_id = catalog.open_stream(
        request,
        Arc::new(move |event: StreamEvent| {
            let _ = app_emit.emit("grpc://stream", &event);
        }),
    )?;
    Ok(GrpcStreamStarted { stream_id })
}

#[tauri::command]
pub async fn grpc_stream_send(
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcStreamSendPayload,
) -> AppResult<()> {
    catalog.send_stream(&payload.stream_id, &payload.payload)
}

#[tauri::command]
pub async fn grpc_stream_close(
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcStreamIdPayload,
) -> AppResult<()> {
    catalog.close_stream(&payload.stream_id)
}

#[tauri::command]
pub async fn grpc_cancel_stream(
    catalog: State<'_, Arc<ProtoCatalog>>,
    payload: GrpcStreamIdPayload,
) -> AppResult<()> {
    catalog.cancel_stream(&payload.stream_id);
    Ok(())
}

fn record_history(
    call: &context::CallContext,
    payload: &GrpcInvokePayload,
    result: &crate::services::grpc::GrpcResult,
    db: &crate::db::DbState,
) -> AppResult<()> {
    let name = call.request_name.clone().unwrap_or_else(|| "gRPC".into());
    let entry = NewHistoryEntry {
        workspace_path: call.root.to_string_lossy().into_owned(),
        request_file_path: payload.request_path.clone(),
        protocol: "grpc".into(),
        name: Some(name.clone()),
        method: Some(payload.method.clone()),
        url: Some(payload.host.clone()),
        request_snapshot: json!({
            "protocol": "grpc",
            "name": name,
            "document": call.request_document.clone().unwrap_or(Value::Null),
            "resolved": {
                "method": payload.method,
                "url": payload.host,
                "bodyPreview": payload.payload,
            }
        })
        .to_string(),
        response_snapshot: Some(
            json!({
                "statusCode": 200,
                "statusText": "OK",
                "body": result.response,
                "durationMs": result.duration_ms,
            })
            .to_string(),
        ),
        response_body_path: None,
        status_code: Some(0),
        duration_ms: Some(result.duration_ms.round() as i64),
    };
    history::insert(db, &entry)?;
    if call.history_limit > 0 {
        let _ = history::trim(db, &call.root.to_string_lossy(), call.history_limit);
    }
    Ok(())
}
