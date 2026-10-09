//! GraphQL commands: query/mutation, introspection, and subscriptions.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use crate::config::ThreadConfigStore;
use crate::db::SharedDb;
use crate::error::{AppError, AppResult};
use crate::services::graphql::{self, GraphqlRequest, SubscriptionEvent, SubscriptionRegistry};
use crate::services::history::{self, NewHistoryEntry};
use crate::workspace::SharedWorkspace;
use crate::workspace::formats::AuthSpec;

use super::context;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlSendPayload {
    pub endpoint: String,
    pub query: String,
    #[serde(default)]
    pub variables: String,
    #[serde(default)]
    pub operation_name: Option<String>,
    #[serde(default)]
    pub headers: Vec<crate::workspace::formats::KeyValue>,
    #[serde(default)]
    pub auth: AuthSpec,
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
pub struct GraphqlIntrospectPayload {
    pub endpoint: String,
    #[serde(default)]
    pub headers: Vec<crate::workspace::formats::KeyValue>,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub environment_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlSubscribePayload {
    pub endpoint: String,
    pub query: String,
    #[serde(default)]
    pub variables: String,
    #[serde(default)]
    pub operation_name: Option<String>,
    #[serde(default)]
    pub headers: Vec<crate::workspace::formats::KeyValue>,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub request_path: Option<String>,
    #[serde(default)]
    pub environment_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionIdPayload {
    pub subscription_id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionStarted {
    pub subscription_id: String,
}

fn request_from(
    payload_endpoint: String,
    payload_query: String,
    payload_variables: String,
    operation_name: Option<String>,
    headers: Vec<crate::workspace::formats::KeyValue>,
    auth: AuthSpec,
    call: &context::CallContext,
) -> GraphqlRequest {
    GraphqlRequest {
        endpoint: payload_endpoint,
        query: payload_query,
        variables: payload_variables,
        operation_name,
        headers,
        auth: AuthSpec::resolve(&auth, call.collection_auth.as_ref()),
        timeout: Duration::from_millis(call.timeout_ms),
        verify_ssl: call.verify_ssl,
        variables_map: call.variables.clone(),
    }
}

#[tauri::command]
pub async fn graphql_send(
    app_workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    db: State<'_, SharedDb>,
    payload: GraphqlSendPayload,
) -> AppResult<graphql::GraphqlResponse> {
    let call = context::prepare(
        &app_workspace,
        &config_store,
        payload.request_path.as_deref(),
        payload.environment_path.as_deref(),
    )?;
    let response = graphql::send(request_from(
        payload.endpoint.clone(),
        payload.query.clone(),
        payload.variables.clone(),
        payload.operation_name.clone(),
        payload.headers.clone(),
        payload.auth.clone(),
        &call,
    ))
    .await?;
    if payload.save_history {
        let _ = record_history(&call, &payload, &response, db.inner());
    }
    Ok(response)
}

#[tauri::command]
pub async fn graphql_introspect(
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    payload: GraphqlIntrospectPayload,
) -> AppResult<Value> {
    let call = context::prepare(
        &workspace,
        &config_store,
        None,
        payload.environment_path.as_deref(),
    )?;
    let response = graphql::send(request_from(
        payload.endpoint,
        graphql::INTROSPECTION_QUERY.to_string(),
        String::new(),
        None,
        payload.headers,
        payload.auth,
        &call,
    ))
    .await?;
    if response.status_code >= 400 {
        return Err(AppError::Network(format!(
            "introspection failed: HTTP {}",
            response.status_code
        )));
    }
    let body: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    if let Some(errors) = body.get("errors") {
        return Err(AppError::Validation(errors.to_string()));
    }
    body.get("data")
        .and_then(|data| data.get("__schema"))
        .cloned()
        .ok_or_else(|| AppError::Validation("introspection response had no __schema".into()))
}

#[tauri::command]
pub async fn graphql_subscribe(
    app: AppHandle,
    workspace: State<'_, SharedWorkspace>,
    config_store: State<'_, Arc<ThreadConfigStore>>,
    registry: State<'_, Arc<SubscriptionRegistry>>,
    payload: GraphqlSubscribePayload,
) -> AppResult<SubscriptionStarted> {
    let call = context::prepare(
        &workspace,
        &config_store,
        payload.request_path.as_deref(),
        payload.environment_path.as_deref(),
    )?;
    let request = request_from(
        payload.endpoint,
        payload.query,
        payload.variables,
        payload.operation_name,
        payload.headers,
        payload.auth,
        &call,
    );
    let app_emit = app.clone();
    let id = registry.subscribe(
        request,
        Arc::new(move |event: SubscriptionEvent| {
            let _ = app_emit.emit("graphql://subscription", &event);
        }),
    )?;
    Ok(SubscriptionStarted {
        subscription_id: id,
    })
}

#[tauri::command]
pub async fn graphql_unsubscribe(
    registry: State<'_, Arc<SubscriptionRegistry>>,
    payload: SubscriptionIdPayload,
) -> AppResult<()> {
    registry.unsubscribe(&payload.subscription_id);
    Ok(())
}

fn record_history(
    call: &context::CallContext,
    payload: &GraphqlSendPayload,
    response: &graphql::GraphqlResponse,
    db: &crate::db::DbState,
) -> AppResult<()> {
    let name = call
        .request_name
        .clone()
        .unwrap_or_else(|| "GraphQL".into());
    let entry = NewHistoryEntry {
        workspace_path: call.root.to_string_lossy().into_owned(),
        request_file_path: payload.request_path.clone(),
        protocol: "graphql".into(),
        name: Some(name.clone()),
        method: Some(graphql::graphql_operation(&payload.query).into()),
        url: Some(payload.endpoint.clone()),
        request_snapshot: json!({
            "protocol": "graphql",
            "name": name,
            "document": call.request_document.clone().unwrap_or(Value::Null),
            "resolved": {
                "method": graphql::graphql_operation(&payload.query),
                "url": payload.endpoint,
                "bodyPreview": payload.query,
            }
        })
        .to_string(),
        response_snapshot: Some(
            json!({
                "statusCode": response.status_code,
                "statusText": response.status_text,
                "headers": response.headers,
                "body": response.body,
                "bodySize": response.body.len(),
                "durationMs": response.duration_ms,
            })
            .to_string(),
        ),
        response_body_path: None,
        status_code: Some(i64::from(response.status_code)),
        duration_ms: Some(response.duration_ms.round() as i64),
    };
    history::insert(db, &entry)?;
    if call.history_limit > 0 {
        let _ = history::trim(db, &call.root.to_string_lossy(), call.history_limit);
    }
    Ok(())
}
