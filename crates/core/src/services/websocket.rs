//! WebSocket client sessions.
//!
//! Each connection is a tokio task. Incoming frames and status changes are
//! reported through a sink so the command layer can emit Tauri events without
//! this module depending on the UI runtime. Dropping the registry aborts every
//! task, which is how sockets are released when the app exits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::error::{AppError, AppResult};

const MAX_RECONNECTS: u32 = 3;

#[derive(Debug, Clone)]
pub struct WsConnect {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub protocols: Vec<String>,
    pub auto_reconnect: bool,
}

#[derive(Debug, Clone)]
pub enum WsOutgoing {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsMessageEvent {
    pub connection_id: String,
    pub direction: String,
    pub is_binary: bool,
    pub payload: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsStatusEvent {
    pub connection_id: String,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectionInfo {
    pub connection_id: String,
    pub url: String,
}

#[derive(Clone)]
pub struct WsSink {
    message: Arc<dyn Fn(WsMessageEvent) + Send + Sync>,
    status: Arc<dyn Fn(WsStatusEvent) + Send + Sync>,
}

impl WsSink {
    pub fn new(
        message: impl Fn(WsMessageEvent) + Send + Sync + 'static,
        status: impl Fn(WsStatusEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: Arc::new(message),
            status: Arc::new(status),
        }
    }
}

struct LiveSocket {
    url: String,
    outgoing: mpsc::UnboundedSender<WsOutgoing>,
    abort: tokio::task::AbortHandle,
}

/// Active sockets. The map is the source of truth for `ws_list_connections`.
#[derive(Default)]
pub struct WsRegistry {
    live: Mutex<std::collections::BTreeMap<String, LiveSocket>>,
    /// Runtime used when `connect` is called from the UI thread.
    io: Mutex<Option<tokio::runtime::Handle>>,
}

impl WsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn bind_runtime(&self, handle: tokio::runtime::Handle) {
        crate::services::bind_io(&self.io, handle);
    }

    pub fn connect(self: &Arc<Self>, config: WsConnect, sink: WsSink) -> AppResult<String> {
        validate_ws_url(&config.url)?;
        let id = crate::db::new_id();
        let (tx, rx) = mpsc::unbounded_channel();
        let id_task = id.clone();
        let url = config.url.clone();
        let registry = Arc::clone(self);
        let finished = id.clone();
        let handle = crate::services::spawn_io(&self.io, async move {
            run_connection(id_task, config, rx, sink).await;
            // 对端关闭或重试耗尽后任务自己结束，必须从登记表摘掉，否则 id 一直占着。
            registry.live.lock().remove(&finished);
        })?;
        self.live.lock().insert(
            id.clone(),
            LiveSocket {
                url,
                outgoing: tx,
                abort: handle.abort_handle(),
            },
        );
        Ok(id)
    }

    pub fn send(&self, id: &str, message: WsOutgoing) -> AppResult<()> {
        let live = self.live.lock();
        let Some(socket) = live.get(id) else {
            return Err(AppError::NotFound(format!("websocket not connected: {id}")));
        };
        socket
            .outgoing
            .send(message)
            .map_err(|_| AppError::Network("websocket is closed".into()))
    }

    pub fn disconnect(&self, id: &str) -> bool {
        let Some(socket) = self.live.lock().remove(id) else {
            return false;
        };
        let _ = socket.outgoing.send(WsOutgoing::Close);
        socket.abort.abort();
        true
    }

    pub fn list(&self) -> Vec<WsConnectionInfo> {
        self.live
            .lock()
            .iter()
            .map(|(id, socket)| WsConnectionInfo {
                connection_id: id.clone(),
                url: socket.url.clone(),
            })
            .collect()
    }
}

impl Drop for WsRegistry {
    fn drop(&mut self) {
        for socket in self.live.lock().values() {
            socket.abort.abort();
        }
    }
}

async fn run_connection(
    id: String,
    config: WsConnect,
    mut outgoing: mpsc::UnboundedReceiver<WsOutgoing>,
    sink: WsSink,
) {
    let user_closed = Arc::new(AtomicBool::new(false));
    let mut attempt = 0_u32;
    loop {
        emit_status(
            &sink,
            &id,
            if attempt == 0 {
                "connecting"
            } else {
                "reconnecting"
            },
            None,
        );
        match connect_once(&config).await {
            Ok(socket) => {
                attempt = 0;
                emit_status(&sink, &id, "open", None);
                let closed = pump(&id, socket, &mut outgoing, &sink, &user_closed).await;
                if user_closed.load(Ordering::SeqCst) || closed.is_ok() {
                    emit_status(&sink, &id, "closed", None);
                    break;
                }
                let reason = closed.err().map(|error| error.to_string());
                if !config.auto_reconnect || attempt >= MAX_RECONNECTS {
                    emit_status(&sink, &id, "error", reason);
                    break;
                }
                attempt += 1;
                emit_status(&sink, &id, "reconnecting", reason);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(error) => {
                if !config.auto_reconnect || attempt >= MAX_RECONNECTS {
                    emit_status(&sink, &id, "error", Some(error.to_string()));
                    break;
                }
                attempt += 1;
                emit_status(&sink, &id, "reconnecting", Some(error.to_string()));
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn connect_once(
    config: &WsConnect,
) -> AppResult<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let mut request = config
        .url
        .clone()
        .into_client_request()
        .map_err(|error| AppError::Validation(format!("invalid websocket url: {error}")))?;
    for (name, value) in &config.headers {
        let header_name = http::HeaderName::try_from(name.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header {name}: {error}")))?;
        let header_value = http::HeaderValue::try_from(value.as_str())
            .map_err(|error| AppError::Validation(format!("invalid header value: {error}")))?;
        request.headers_mut().insert(header_name, header_value);
    }
    if !config.protocols.is_empty() {
        let joined = config.protocols.join(", ");
        request.headers_mut().insert(
            http::header::SEC_WEBSOCKET_PROTOCOL,
            http::HeaderValue::from_str(&joined)
                .map_err(|error| AppError::Validation(error.to_string()))?,
        );
    }
    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    Ok(socket)
}

async fn pump(
    id: &str,
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    outgoing: &mut mpsc::UnboundedReceiver<WsOutgoing>,
    sink: &WsSink,
    user_closed: &AtomicBool,
) -> AppResult<()> {
    let (mut write, mut read) = socket.split();
    loop {
        tokio::select! {
            incoming = read.next() => {
                let Some(incoming) = incoming else {
                    return Err(AppError::Network("connection closed".into()));
                };
                let message = incoming.map_err(|error| AppError::Network(error.to_string()))?;
                match message {
                    Message::Text(text) => emit_message(sink, id, "in", false, text.to_string()),
                    Message::Binary(bytes) => emit_message(
                        sink,
                        id,
                        "in",
                        true,
                        base64::engine::general_purpose::STANDARD.encode(bytes),
                    ),
                    Message::Close(frame) => {
                        let reason = frame.map(|frame| frame.reason.to_string());
                        if reason.is_some() {
                            emit_status(sink, id, "closing", reason);
                        }
                        return Ok(());
                    }
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                }
            }
            next = outgoing.recv() => {
                let Some(next) = next else {
                    user_closed.store(true, Ordering::SeqCst);
                    let _ = write.send(Message::Close(None)).await;
                    return Ok(());
                };
                match next {
                    WsOutgoing::Text(text) => {
                        emit_message(sink, id, "out", false, text.clone());
                        write
                            .send(Message::Text(text.into()))
                            .await
                            .map_err(|error| AppError::Network(error.to_string()))?;
                    }
                    WsOutgoing::Binary(bytes) => {
                        emit_message(
                            sink,
                            id,
                            "out",
                            true,
                            base64::engine::general_purpose::STANDARD.encode(&bytes),
                        );
                        write
                            .send(Message::Binary(bytes.into()))
                            .await
                            .map_err(|error| AppError::Network(error.to_string()))?;
                    }
                    WsOutgoing::Close => {
                        user_closed.store(true, Ordering::SeqCst);
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn emit_message(sink: &WsSink, id: &str, direction: &str, is_binary: bool, payload: String) {
    (sink.message)(WsMessageEvent {
        connection_id: id.to_string(),
        direction: direction.to_string(),
        is_binary,
        payload,
        timestamp: crate::db::now_ms(),
    });
}

fn emit_status(sink: &WsSink, id: &str, status: &str, reason: Option<String>) {
    (sink.status)(WsStatusEvent {
        connection_id: id.to_string(),
        status: status.to_string(),
        reason,
    });
}

fn validate_ws_url(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return Ok(());
    }
    Err(AppError::Validation(
        "websocket url must start with ws:// or wss://".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[tokio::test]
    async fn echo_round_trip_then_close() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(message) = socket.next().await {
                let message = message.unwrap();
                if message.is_close() {
                    break;
                }
                socket.send(message).await.unwrap();
            }
        });

        let messages = Arc::new(StdMutex::new(Vec::new()));
        let statuses = Arc::new(StdMutex::new(Vec::new()));
        let messages_sink = messages.clone();
        let statuses_sink = statuses.clone();
        let registry = Arc::new(WsRegistry::new());
        let id = registry
            .connect(
                WsConnect {
                    url: format!("ws://{address}"),
                    headers: Vec::new(),
                    protocols: Vec::new(),
                    auto_reconnect: false,
                },
                WsSink::new(
                    move |event| messages_sink.lock().unwrap().push(event),
                    move |event| statuses_sink.lock().unwrap().push(event.status),
                ),
            )
            .unwrap();

        let opened = wait_status(&statuses, "open").await;
        assert!(opened, "socket did not open: {statuses:?}");
        registry.send(&id, WsOutgoing::Text("ping".into())).unwrap();
        let echoed = wait_message(&messages, "in", "ping").await;
        assert!(echoed, "echo missing: {messages:?}");
        assert!(registry.disconnect(&id));
    }

    async fn wait_status(statuses: &StdMutex<Vec<String>>, expected: &str) -> bool {
        for _ in 0..50 {
            if statuses
                .lock()
                .unwrap()
                .iter()
                .any(|status| status == expected)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    async fn wait_message(
        messages: &StdMutex<Vec<WsMessageEvent>>,
        direction: &str,
        payload: &str,
    ) -> bool {
        for _ in 0..50 {
            if messages
                .lock()
                .unwrap()
                .iter()
                .any(|event| event.direction == direction && event.payload == payload)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }
}
