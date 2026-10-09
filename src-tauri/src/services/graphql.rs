//! GraphQL over HTTP, plus `graphql-transport-ws` subscriptions.
//!
//! Query / Mutation / introspection reuse the shared hyper client.
//! Subscriptions open a WebSocket, speak the modern graphql-ws protocol, and
//! push each `next` / `error` / `complete` frame through a caller-supplied sink.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::error::{AppError, AppResult};
use crate::services::http::apply_auth;
use crate::services::variables::substitute;
use crate::workspace::formats::{AuthSpec, KeyValue};

pub const INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      kind
      name
      fields(includeDeprecated: true) {
        name
        args {
          name
          type { ...TypeRef }
        }
        type { ...TypeRef }
      }
    }
  }
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType { kind name }
    }
  }
}
"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: BTreeMap<String, Vec<String>>,
    pub body: String,
    pub duration_ms: f64,
    pub unresolved_variables: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GraphqlRequest {
    pub endpoint: String,
    pub query: String,
    pub variables: String,
    pub operation_name: Option<String>,
    pub headers: Vec<KeyValue>,
    pub auth: AuthSpec,
    pub timeout: Duration,
    pub verify_ssl: bool,
    pub variables_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionEvent {
    pub subscription_id: String,
    pub event: String,
    pub data: Option<Value>,
}

pub type SubscriptionSink = Arc<dyn Fn(SubscriptionEvent) + Send + Sync>;

struct LiveSubscription {
    outgoing: mpsc::UnboundedSender<String>,
    abort: tokio::task::AbortHandle,
}

/// Active GraphQL subscriptions. Dropping the registry aborts every socket.
#[derive(Default)]
pub struct SubscriptionRegistry {
    live: Mutex<BTreeMap<String, LiveSubscription>>,
}

impl SubscriptionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self, request: GraphqlRequest, sink: SubscriptionSink) -> AppResult<String> {
        let prepared = prepare_call(&request)?;
        if graphql_operation(&prepared.query) != "subscription" {
            return Err(AppError::Validation(
                "graphql_subscribe expects a subscription operation".into(),
            ));
        }
        let id = crate::db::new_id();
        let (tx, rx) = mpsc::unbounded_channel();
        let sink_task = sink.clone();
        let id_task = id.clone();
        let handle = tokio::spawn(async move {
            if let Err(error) = run_subscription(&id_task, &prepared, rx, &sink_task).await {
                sink_task(SubscriptionEvent {
                    subscription_id: id_task.clone(),
                    event: "error".into(),
                    data: Some(json!({ "message": error.to_string() })),
                });
            }
            sink_task(SubscriptionEvent {
                subscription_id: id_task,
                event: "complete".into(),
                data: None,
            });
        });
        self.live.lock().insert(
            id.clone(),
            LiveSubscription {
                outgoing: tx,
                abort: handle.abort_handle(),
            },
        );
        Ok(id)
    }

    pub fn unsubscribe(&self, id: &str) -> bool {
        let Some(live) = self.live.lock().remove(id) else {
            return false;
        };
        let _ = live.outgoing.send(complete_frame(id));
        live.abort.abort();
        true
    }

    pub fn list(&self) -> Vec<String> {
        self.live.lock().keys().cloned().collect()
    }
}

impl Drop for SubscriptionRegistry {
    fn drop(&mut self) {
        for (_, live) in self.live.lock().iter() {
            live.abort.abort();
        }
    }
}

pub async fn send(request: GraphqlRequest) -> AppResult<GraphqlResponse> {
    let prepared = prepare_call(&request)?;
    if graphql_operation(&prepared.query) == "subscription" {
        return Err(AppError::Validation(
            "subscription operations use graphql_subscribe".into(),
        ));
    }
    let started = Instant::now();
    let payload = serde_json::to_vec(&prepared.body)?;
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::ACCEPT,
        http::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    for (name, value) in &prepared.headers {
        let header_name = http::HeaderName::try_from(name.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header {name}: {error}")))?;
        let header_value = http::HeaderValue::try_from(value.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header value: {error}")))?;
        headers.insert(header_name, header_value);
    }
    let response = crate::services::transport::exchange(crate::services::transport::Exchange {
        method: http::Method::POST,
        url: prepared.endpoint,
        headers,
        body: Some(bytes::Bytes::from(payload)),
        timeout: request.timeout,
        verify_ssl: request.verify_ssl,
        max_redirects: 10,
    })
    .await?;
    let status = response.status;
    let mut headers = BTreeMap::new();
    for (name, value) in &response.headers {
        headers
            .entry(name.as_str().to_string())
            .or_insert_with(Vec::new)
            .push(value.to_str().unwrap_or("").to_string());
    }
    let mut body_bytes = Vec::new();
    let mut stream = response.body;
    while let Some(chunk) = stream.next().await {
        body_bytes.extend_from_slice(&chunk.map_err(|error| AppError::from_transport(&error))?);
    }
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    Ok(GraphqlResponse {
        status_code: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        headers,
        body,
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
        unresolved_variables: prepared.missing,
    })
}

struct PreparedCall {
    endpoint: String,
    query: String,
    body: Value,
    headers: Vec<(String, String)>,
    missing: Vec<String>,
}

fn prepare_call(request: &GraphqlRequest) -> AppResult<PreparedCall> {
    let mut missing = Vec::new();
    let endpoint = substitute(&request.endpoint, &request.variables_map, &mut missing);
    if endpoint.trim().is_empty() {
        return Err(AppError::Validation("GraphQL endpoint is required".into()));
    }
    let query = substitute(&request.query, &request.variables_map, &mut missing);
    if query.trim().is_empty() {
        return Err(AppError::Validation("GraphQL query is required".into()));
    }
    let variables_text = substitute(&request.variables, &request.variables_map, &mut missing);
    let variables = parse_variables(&variables_text)?;
    let operation_name = request
        .operation_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| substitute(value, &request.variables_map, &mut missing));

    let mut header_rows = request.headers.clone();
    let mut query_pairs = Vec::new();
    apply_auth(
        &request.auth,
        &request.variables_map,
        &mut missing,
        &mut header_rows,
        &mut query_pairs,
    )?;
    let mut headers = Vec::new();
    let mut saw_content_type = false;
    for row in header_rows {
        if !row.enabled || row.key.trim().is_empty() {
            continue;
        }
        let value = substitute(&row.value, &request.variables_map, &mut missing);
        if row.key.eq_ignore_ascii_case("content-type") {
            saw_content_type = true;
        }
        headers.push((row.key, value));
    }
    if !saw_content_type {
        headers.push(("content-type".into(), "application/json".into()));
    }
    if !query_pairs.is_empty() {
        return Err(AppError::Validation(
            "GraphQL auth cannot place an API key in the query string; use a header".into(),
        ));
    }

    let mut body = json!({
        "query": query,
        "variables": variables,
    });
    if let Some(name) = operation_name {
        body["operationName"] = Value::String(name);
    }
    Ok(PreparedCall {
        endpoint,
        query,
        body,
        headers,
        missing,
    })
}

fn parse_variables(text: &str) -> AppResult<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(trimmed)?;
    if !value.is_object() {
        return Err(AppError::Validation(
            "GraphQL variables must be a JSON object".into(),
        ));
    }
    Ok(value)
}

/// Leading operation keyword, or `query` for an anonymous selection set.
pub fn graphql_operation(query: &str) -> &'static str {
    let trimmed = strip_graphql_noise(query);
    if trimmed.starts_with("subscription") {
        "subscription"
    } else if trimmed.starts_with("mutation") {
        "mutation"
    } else {
        "query"
    }
}

fn strip_graphql_noise(query: &str) -> String {
    let mut out = String::new();
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '#' {
            for next in chars.by_ref() {
                if next == '\n' {
                    break;
                }
            }
            out.push(' ');
            continue;
        }
        out.push(ch);
    }
    out.split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn complete_frame(id: &str) -> String {
    json!({ "type": "complete", "id": id }).to_string()
}

async fn run_subscription(
    id: &str,
    prepared: &PreparedCall,
    mut stop: mpsc::UnboundedReceiver<String>,
    sink: &SubscriptionSink,
) -> AppResult<()> {
    let ws_url = http_to_ws(&prepared.endpoint)?;
    let mut request = ws_url
        .into_client_request()
        .map_err(|error| AppError::Validation(format!("invalid subscription url: {error}")))?;
    for (name, value) in &prepared.headers {
        let header_name = http::HeaderName::try_from(name.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header {name}: {error}")))?;
        let header_value = http::HeaderValue::try_from(value.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header value: {error}")))?;
        request.headers_mut().insert(header_name, header_value);
    }
    request.headers_mut().insert(
        "sec-websocket-protocol",
        http::HeaderValue::from_static("graphql-transport-ws"),
    );

    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;

    let init = json!({
        "type": "connection_init",
        "payload": header_payload(&prepared.headers),
    });
    socket
        .send(Message::Text(init.to_string().into()))
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;

    match tokio::time::timeout(Duration::from_secs(10), socket.next()).await {
        Ok(Some(Ok(message))) => {
            let text = message_text(message)?;
            let frame: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            match frame.get("type").and_then(Value::as_str) {
                Some("connection_ack") => {}
                Some("connection_error") => {
                    return Err(AppError::Network(format!(
                        "connection_error: {}",
                        frame.get("payload").unwrap_or(&Value::Null)
                    )));
                }
                other => {
                    return Err(AppError::Network(format!(
                        "expected connection_ack, got {}",
                        other.unwrap_or("invalid frame")
                    )));
                }
            }
        }
        Ok(Some(Err(error))) => return Err(AppError::Network(error.to_string())),
        _ => return Err(AppError::Timeout("subscription ack timed out".into())),
    }

    let subscribe = json!({
        "type": "subscribe",
        "id": id,
        "payload": prepared.body,
    });
    socket
        .send(Message::Text(subscribe.to_string().into()))
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    sink(SubscriptionEvent {
        subscription_id: id.to_string(),
        event: "ack".into(),
        data: None,
    });

    loop {
        tokio::select! {
            incoming = socket.next() => {
                let Some(incoming) = incoming else { break };
                let message = incoming.map_err(|error| AppError::Network(error.to_string()))?;
                if message.is_close() {
                    break;
                }
                if message.is_ping() || message.is_pong() {
                    continue;
                }
                let text = message_text(message)?;
                let frame: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                if frame.get("type").and_then(Value::as_str) == Some("ping") {
                    let _ = socket.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                    continue;
                }
                let event_name = frame.get("type").and_then(Value::as_str).unwrap_or("next");
                sink(SubscriptionEvent {
                    subscription_id: id.to_string(),
                    event: event_name.to_string(),
                    data: frame.get("payload").cloned(),
                });
                if event_name == "complete" || event_name == "error" {
                    break;
                }
            }
            outgoing = stop.recv() => {
                let Some(frame) = outgoing else { break };
                let _ = socket.send(Message::Text(frame.into())).await;
                break;
            }
        }
    }
    Ok(())
}

fn header_payload(headers: &[(String, String)]) -> Value {
    let mut payload = serde_json::Map::new();
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-type") || name.eq_ignore_ascii_case("accept") {
            continue;
        }
        payload.insert(name.clone(), Value::String(value.clone()));
    }
    Value::Object(payload)
}

fn http_to_ws(endpoint: &str) -> AppResult<String> {
    let trimmed = endpoint.trim();
    if let Some(rest) = trimmed.strip_prefix("https://") {
        return Ok(format!("wss://{rest}"));
    }
    if let Some(rest) = trimmed.strip_prefix("http://") {
        return Ok(format!("ws://{rest}"));
    }
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return Ok(trimmed.to_string());
    }
    Err(AppError::Validation(
        "subscription endpoint must be an http(s) URL".into(),
    ))
}

fn message_text(message: Message) -> AppResult<String> {
    match message {
        Message::Text(text) => Ok(text.to_string()),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
            .map_err(|_| AppError::Network("subscription frame was not utf-8".into())),
        Message::Close(_) => Ok(String::new()),
        _ => Ok(String::new()),
    }
}

#[derive(Debug, Deserialize)]
struct FrameType<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
}

pub fn frame_kind(text: &str) -> Option<String> {
    serde_json::from_str::<FrameType<'_>>(text)
        .ok()
        .map(|frame| frame.kind.to_string())
}

#[cfg(test)]
fn select_graphql_protocol(
    _request: &tokio_tungstenite::tungstenite::handshake::server::Request,
    mut response: tokio_tungstenite::tungstenite::handshake::server::Response,
) -> Result<
    tokio_tungstenite::tungstenite::handshake::server::Response,
    tokio_tungstenite::tungstenite::handshake::server::ErrorResponse,
> {
    response.headers_mut().insert(
        "sec-websocket-protocol",
        tokio_tungstenite::tungstenite::http::HeaderValue::from_static("graphql-transport-ws"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_selection_is_a_query() {
        assert_eq!(graphql_operation("{ hello }"), "query");
        assert_eq!(graphql_operation("mutation Add { add }"), "mutation");
        assert_eq!(
            graphql_operation("# note\nsubscription OnEvent { event }"),
            "subscription"
        );
    }

    #[test]
    fn variables_must_be_an_object() {
        assert!(parse_variables("").unwrap().is_object());
        assert!(parse_variables("[1]").is_err());
    }

    #[test]
    fn http_endpoint_becomes_a_websocket_url() {
        assert_eq!(
            http_to_ws("http://127.0.0.1:9/graphql").unwrap(),
            "ws://127.0.0.1:9/graphql"
        );
        assert_eq!(
            http_to_ws("https://example.com/graphql").unwrap(),
            "wss://example.com/graphql"
        );
    }

    #[tokio::test]
    async fn query_posts_json_to_a_local_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0_u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let body = br#"{"data":{"hello":"world"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            use tokio::io::AsyncWriteExt;
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let response = send(GraphqlRequest {
            endpoint: format!("http://{address}/graphql"),
            query: "{ hello }".into(),
            variables: String::new(),
            operation_name: None,
            headers: Vec::new(),
            auth: AuthSpec::default(),
            timeout: Duration::from_secs(5),
            verify_ssl: true,
            variables_map: BTreeMap::new(),
        })
        .await
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert!(response.body.contains("world"));
    }

    #[tokio::test]
    async fn subscription_receives_next_then_complete() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(stream, select_graphql_protocol)
                .await
                .unwrap();
            let init = socket.next().await.unwrap().unwrap();
            assert!(init.to_string().contains("connection_init"));
            socket
                .send(Message::Text(r#"{"type":"connection_ack"}"#.into()))
                .await
                .unwrap();
            let _subscribe = socket.next().await.unwrap().unwrap();
            socket
                .send(Message::Text(
                    r#"{"type":"next","id":"x","payload":{"data":{"event":1}}}"#.into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(r#"{"type":"complete","id":"x"}"#.into()))
                .await
                .unwrap();
        });

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let collected = events.clone();
        let registry = SubscriptionRegistry::new();
        registry
            .subscribe(
                GraphqlRequest {
                    endpoint: format!("http://{address}/graphql"),
                    query: "subscription OnEvent { event }".into(),
                    variables: String::new(),
                    operation_name: None,
                    headers: Vec::new(),
                    auth: AuthSpec::default(),
                    timeout: Duration::from_secs(5),
                    verify_ssl: true,
                    variables_map: BTreeMap::new(),
                },
                Arc::new(move |event| collected.lock().unwrap().push(event.event)),
            )
            .unwrap();

        for _ in 0..50 {
            let names = events.lock().unwrap().clone();
            if names.iter().any(|name| name == "next")
                && names.iter().any(|name| name == "complete")
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("subscription events: {:?}", events.lock().unwrap());
    }
}
