//! MQTT client sessions on top of the in-tree `mqtt_codec` crate.
//!
//! Each connection is a tokio task. The codec only frames packets; this module
//! owns the socket (TCP, TLS, or WebSocket), keep-alive, and the QoS 1/2
//! handshake. Unacknowledged publishes are not retried after a disconnect.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use mqtt_codec::{Decoder, Encoder};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::error::{AppError, AppResult};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct MqttConnect {
    pub broker_url: String,
    /// Upgrade `mqtt://` / `tcp://` / `ws://` to TLS. `mqtts://` and `wss://` are always TLS.
    pub tls: bool,
    pub verify_ssl: bool,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub keep_alive: u16,
    pub clean_session: bool,
    /// `4` is MQTT 3.1.1, `5` is MQTT 5.0.
    pub protocol_level: u8,
    pub subscriptions: Vec<(String, u8)>,
}

#[derive(Debug, Clone)]
pub enum MqttCommand {
    Subscribe {
        topic: String,
        qos: u8,
    },
    Unsubscribe {
        topic: String,
    },
    Publish {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
    },
    Disconnect,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttMessageEvent {
    pub connection_id: String,
    pub direction: String,
    pub topic: String,
    pub payload: String,
    pub qos: u8,
    pub retain: bool,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttStatusEvent {
    pub connection_id: String,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttConnectionInfo {
    pub connection_id: String,
    pub broker_url: String,
}

#[derive(Clone)]
pub struct MqttSink {
    message: Arc<dyn Fn(MqttMessageEvent) + Send + Sync>,
    status: Arc<dyn Fn(MqttStatusEvent) + Send + Sync>,
}

impl MqttSink {
    pub fn new(
        message: impl Fn(MqttMessageEvent) + Send + Sync + 'static,
        status: impl Fn(MqttStatusEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: Arc::new(message),
            status: Arc::new(status),
        }
    }
}

struct LiveSession {
    broker_url: String,
    outgoing: mpsc::UnboundedSender<MqttCommand>,
    abort: tokio::task::AbortHandle,
}

/// Active MQTT sessions. Dropping the registry aborts every task.
#[derive(Default)]
pub struct MqttRegistry {
    live: Mutex<std::collections::BTreeMap<String, LiveSession>>,
    io: Mutex<Option<tokio::runtime::Handle>>,
}

impl MqttRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn bind_runtime(&self, handle: tokio::runtime::Handle) {
        crate::services::bind_io(&self.io, handle);
    }

    pub fn connect(self: &Arc<Self>, mut config: MqttConnect, sink: MqttSink) -> AppResult<String> {
        normalize_connect(&mut config)?;
        let id = crate::db::new_id();
        let (tx, rx) = mpsc::unbounded_channel();
        let broker_url = config.broker_url.clone();
        let id_task = id.clone();
        let registry = Arc::clone(self);
        let finished = id.clone();
        let handle = crate::services::spawn_io(&self.io, async move {
            run_connection(id_task, config, rx, sink).await;
            registry.live.lock().remove(&finished);
        })?;
        self.live.lock().insert(
            id.clone(),
            LiveSession {
                broker_url,
                outgoing: tx,
                abort: handle.abort_handle(),
            },
        );
        Ok(id)
    }

    pub fn subscribe(&self, id: &str, topic: &str, qos: u8) -> AppResult<()> {
        check_filter(topic)?;
        check_qos(qos)?;
        self.command(
            id,
            MqttCommand::Subscribe {
                topic: topic.to_string(),
                qos,
            },
        )
    }

    pub fn unsubscribe(&self, id: &str, topic: &str) -> AppResult<()> {
        check_filter(topic)?;
        self.command(
            id,
            MqttCommand::Unsubscribe {
                topic: topic.to_string(),
            },
        )
    }

    pub fn publish(
        &self,
        id: &str,
        topic: &str,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
    ) -> AppResult<()> {
        check_publish_topic(topic)?;
        check_qos(qos)?;
        self.command(
            id,
            MqttCommand::Publish {
                topic: topic.to_string(),
                payload,
                qos,
                retain,
            },
        )
    }

    pub fn disconnect(&self, id: &str) -> bool {
        let Some(session) = self.live.lock().remove(id) else {
            return false;
        };
        let _ = session.outgoing.send(MqttCommand::Disconnect);
        true
    }

    pub fn list(&self) -> Vec<MqttConnectionInfo> {
        self.live
            .lock()
            .iter()
            .map(|(id, session)| MqttConnectionInfo {
                connection_id: id.clone(),
                broker_url: session.broker_url.clone(),
            })
            .collect()
    }

    fn command(&self, id: &str, command: MqttCommand) -> AppResult<()> {
        let live = self.live.lock();
        let Some(session) = live.get(id) else {
            return Err(AppError::NotFound(format!("mqtt not connected: {id}")));
        };
        session
            .outgoing
            .send(command)
            .map_err(|_| AppError::Network("mqtt connection is closed".into()))
    }
}

impl Drop for MqttRegistry {
    fn drop(&mut self) {
        for session in self.live.lock().values() {
            session.abort.abort();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Version {
    V4,
    V5,
}

struct Endpoint {
    kind: TransportKind,
    host: String,
    port: u16,
    path: String,
    username: String,
    password: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransportKind {
    Tcp,
    Tls,
    WebSocket { tls: bool },
}

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

enum Reader {
    Tcp(tokio::io::ReadHalf<TcpStream>),
    Tls(tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>),
    Ws(futures_util::stream::SplitStream<WsStream>),
}

enum Writer {
    Tcp(tokio::io::WriteHalf<TcpStream>),
    Tls(tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>),
    Ws(futures_util::stream::SplitSink<WsStream, tokio_tungstenite::tungstenite::Message>),
}

enum Wake {
    Command(Option<MqttCommand>),
    Packet(Incoming),
    Ping,
    Failed(AppError),
}

struct PacketIds {
    next: u16,
}

impl PacketIds {
    fn new() -> Self {
        Self { next: 1 }
    }

    fn alloc(&mut self) -> u16 {
        let id = self.next;
        self.next = if self.next == u16::MAX {
            1
        } else {
            self.next + 1
        };
        id
    }
}

struct ConnAckInfo {
    ok: bool,
    reason: String,
    server_keep_alive: Option<u16>,
    max_packet_size: Option<u32>,
}

struct Published {
    topic: String,
    payload: Vec<u8>,
    qos: u8,
    retain: bool,
    packet_id: Option<u16>,
}

struct AckInfo {
    packet_id: u16,
    ok: bool,
    detail: String,
}

enum Incoming {
    ConnAck(ConnAckInfo),
    Publish(Published),
    PubAck(u16),
    PubRec(u16),
    PubRel(u16),
    PubComp(u16),
    SubAck(AckInfo),
    UnsubAck(u16),
    PingResp,
    Disconnect(String),
    Auth,
}

enum Outgoing {
    V4(mqtt_codec::v4::Packet),
    V5(Box<mqtt_codec::v5::Packet>),
}

fn v5(packet: mqtt_codec::v5::Packet) -> Outgoing {
    Outgoing::V5(Box::new(packet))
}

fn normalize_connect(config: &mut MqttConnect) -> AppResult<()> {
    let endpoint = parse_broker(&config.broker_url, config.tls)?;
    if config.username.trim().is_empty() {
        config.username = endpoint.username;
    }
    if config.password.is_empty() {
        config.password = endpoint.password;
    }
    config.client_id = config.client_id.trim().to_string();
    config.username = config.username.trim().to_string();
    if !config.password.is_empty() && config.username.is_empty() {
        return Err(AppError::Validation(
            "mqtt password requires a username".into(),
        ));
    }
    if config.client_id.is_empty() && !config.clean_session {
        return Err(AppError::Validation(
            "a persistent session needs a client id".into(),
        ));
    }
    match config.protocol_level {
        4 | 5 => {}
        other => {
            return Err(AppError::Validation(format!(
                "mqtt protocol level must be 4 or 5, got {other}"
            )));
        }
    }
    for (topic, qos) in &config.subscriptions {
        if topic.trim().is_empty() {
            continue;
        }
        check_filter(topic)?;
        check_qos(*qos)?;
    }
    Ok(())
}

fn parse_broker(raw: &str, force_tls: bool) -> AppResult<Endpoint> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("broker url is required".into()));
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else if force_tls {
        format!("mqtts://{trimmed}")
    } else {
        format!("mqtt://{trimmed}")
    };
    let parsed = url::Url::parse(&with_scheme)
        .map_err(|error| AppError::Validation(format!("invalid broker url: {error}")))?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let (mut kind, default_port) = match scheme.as_str() {
        "mqtt" | "tcp" => (TransportKind::Tcp, 1883),
        "mqtts" | "ssl" | "tls" => (TransportKind::Tls, 8883),
        "ws" => (TransportKind::WebSocket { tls: false }, 80),
        "wss" => (TransportKind::WebSocket { tls: true }, 443),
        other => {
            return Err(AppError::Validation(format!(
                "unsupported broker scheme: {other}"
            )));
        }
    };
    if force_tls {
        kind = match kind {
            TransportKind::Tcp | TransportKind::Tls => TransportKind::Tls,
            TransportKind::WebSocket { .. } => TransportKind::WebSocket { tls: true },
        };
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| AppError::Validation("broker url is missing a host".into()))?
        .to_string();
    let port = parsed.port().unwrap_or(default_port);
    let path = match kind {
        TransportKind::WebSocket { .. } => {
            let path = parsed.path();
            if path.is_empty() || path == "/" {
                "/mqtt".to_string()
            } else {
                path.to_string()
            }
        }
        TransportKind::Tcp | TransportKind::Tls => String::new(),
    };
    Ok(Endpoint {
        kind,
        host,
        port,
        path,
        username: parsed.username().to_string(),
        password: parsed.password().unwrap_or("").to_string(),
    })
}

async fn run_connection(
    id: String,
    config: MqttConnect,
    mut outgoing: mpsc::UnboundedReceiver<MqttCommand>,
    sink: MqttSink,
) {
    emit_status(&sink, &id, "connecting", None);
    let version = if config.protocol_level == 5 {
        Version::V5
    } else {
        Version::V4
    };
    let endpoint = match parse_broker(&config.broker_url, config.tls) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            emit_status(&sink, &id, "error", Some(error.to_string()));
            return;
        }
    };
    let (mut reader, mut writer) = match connect_link(&endpoint, config.verify_ssl).await {
        Ok(link) => link,
        Err(error) => {
            emit_status(&sink, &id, "error", Some(error.to_string()));
            return;
        }
    };

    if let Err(error) = send_connect(&mut writer, version, &config).await {
        emit_status(&sink, &id, "error", Some(error.to_string()));
        return;
    }

    let mut buf = BytesMut::new();
    let mut max_packet = 0_u32;
    let connack = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        next_incoming(&mut reader, version, max_packet, &mut buf),
    )
    .await
    {
        Ok(Ok(Incoming::ConnAck(info))) => info,
        Ok(Ok(_)) => {
            emit_status(
                &sink,
                &id,
                "error",
                Some("broker did not send CONNACK".into()),
            );
            return;
        }
        Ok(Err(error)) => {
            emit_status(&sink, &id, "error", Some(error.to_string()));
            return;
        }
        Err(_) => {
            emit_status(
                &sink,
                &id,
                "error",
                Some("timed out waiting for CONNACK".into()),
            );
            return;
        }
    };
    if !connack.ok {
        emit_status(&sink, &id, "error", Some(connack.reason));
        return;
    }
    if let Some(limit) = connack.max_packet_size.filter(|limit| *limit > 0) {
        max_packet = limit;
    }
    let keep_alive = connack.server_keep_alive.unwrap_or(config.keep_alive);
    emit_status(&sink, &id, "open", None);

    let mut ids = PacketIds::new();
    let mut pending_sub: HashMap<u16, String> = HashMap::new();
    let mut qos2_out: HashSet<u16> = HashSet::new();
    for (topic, qos) in &config.subscriptions {
        let topic = topic.trim();
        if topic.is_empty() {
            continue;
        }
        let mut session = session(
            &mut writer,
            version,
            max_packet,
            &mut ids,
            &mut pending_sub,
            &mut qos2_out,
        );
        if let Err(error) = send_subscribe(&mut session, topic, *qos).await {
            emit_status(&sink, &id, "rejected", Some(format!("{topic}: {error}")));
        }
    }

    let mut ping = tokio::time::interval(ping_period(keep_alive));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping.tick().await;

    loop {
        let wake = tokio::select! {
            command = outgoing.recv() => Wake::Command(command),
            incoming = next_incoming(&mut reader, version, max_packet, &mut buf) => {
                match incoming {
                    Ok(packet) => Wake::Packet(packet),
                    Err(error) => Wake::Failed(error),
                }
            }
            _ = ping.tick(), if keep_alive > 0 => Wake::Ping,
        };

        let mut session = session(
            &mut writer,
            version,
            max_packet,
            &mut ids,
            &mut pending_sub,
            &mut qos2_out,
        );
        let step = match wake {
            Wake::Command(None) | Wake::Command(Some(MqttCommand::Disconnect)) => {
                let _ = send_disconnect(&mut session).await;
                emit_status(&sink, &id, "closed", None);
                break;
            }
            Wake::Command(Some(command)) => apply_command(&id, &sink, &mut session, command).await,
            Wake::Packet(packet) => apply_packet(&id, &sink, &mut session, packet).await,
            Wake::Ping => send_ping(&mut session)
                .await
                .map(|()| Loop::Continue)
                .unwrap_or_else(Loop::Stop),
            Wake::Failed(error) => Loop::Stop(error),
        };
        match step {
            Loop::Continue => {}
            Loop::Closed(reason) => {
                emit_status(&sink, &id, "closed", reason);
                break;
            }
            Loop::Stop(error) => {
                emit_status(&sink, &id, "error", Some(error.to_string()));
                break;
            }
        }
    }
}

enum Loop {
    Continue,
    Closed(Option<String>),
    Stop(AppError),
}

struct Session<'a> {
    writer: &'a mut Writer,
    version: Version,
    max_packet: u32,
    ids: &'a mut PacketIds,
    pending_sub: &'a mut HashMap<u16, String>,
    qos2_out: &'a mut HashSet<u16>,
}

fn session<'a>(
    writer: &'a mut Writer,
    version: Version,
    max_packet: u32,
    ids: &'a mut PacketIds,
    pending_sub: &'a mut HashMap<u16, String>,
    qos2_out: &'a mut HashSet<u16>,
) -> Session<'a> {
    Session {
        writer,
        version,
        max_packet,
        ids,
        pending_sub,
        qos2_out,
    }
}

async fn apply_command(
    id: &str,
    sink: &MqttSink,
    session: &mut Session<'_>,
    command: MqttCommand,
) -> Loop {
    let result = match command {
        MqttCommand::Subscribe { topic, qos } => send_subscribe(session, &topic, qos).await,
        MqttCommand::Unsubscribe { topic } => send_unsubscribe(session, &topic).await,
        MqttCommand::Publish {
            topic,
            payload,
            qos,
            retain,
        } => send_publish(id, sink, session, &topic, payload, qos, retain).await,
        MqttCommand::Disconnect => unreachable!("disconnect is handled before apply_command"),
    };
    match result {
        Ok(()) => Loop::Continue,
        Err(error) => {
            emit_status(sink, id, "rejected", Some(error.to_string()));
            Loop::Continue
        }
    }
}

async fn apply_packet(
    id: &str,
    sink: &MqttSink,
    session: &mut Session<'_>,
    packet: Incoming,
) -> Loop {
    match packet {
        Incoming::Publish(publish) => {
            if publish.qos == 1
                && let Some(packet_id) = publish.packet_id
                && let Err(error) = write_packet(session, puback(session.version, packet_id)).await
            {
                return Loop::Stop(error);
            }
            if publish.qos == 2
                && let Some(packet_id) = publish.packet_id
                && let Err(error) = write_packet(session, pubrec(session.version, packet_id)).await
            {
                return Loop::Stop(error);
            }
            emit_message(
                sink,
                id,
                "in",
                &publish.topic,
                &publish.payload,
                publish.qos,
                publish.retain,
            );
            Loop::Continue
        }
        Incoming::PubRec(packet_id) => {
            session.qos2_out.remove(&packet_id);
            match write_packet(session, pubrel(session.version, packet_id)).await {
                Ok(()) => Loop::Continue,
                Err(error) => Loop::Stop(error),
            }
        }
        Incoming::PubRel(packet_id) => {
            match write_packet(session, pubcomp(session.version, packet_id)).await {
                Ok(()) => Loop::Continue,
                Err(error) => Loop::Stop(error),
            }
        }
        Incoming::SubAck(ack) => {
            let topic = session
                .pending_sub
                .remove(&ack.packet_id)
                .unwrap_or_default();
            let detail = if topic.is_empty() {
                ack.detail
            } else {
                format!("{topic} {}", ack.detail)
            };
            emit_status(
                sink,
                id,
                if ack.ok { "subscribed" } else { "rejected" },
                Some(detail),
            );
            Loop::Continue
        }
        Incoming::UnsubAck(packet_id) => {
            emit_status(sink, id, "unsubscribed", Some(packet_id.to_string()));
            Loop::Continue
        }
        Incoming::Disconnect(reason) => {
            let reason = if reason.is_empty() {
                None
            } else {
                Some(reason)
            };
            Loop::Closed(reason)
        }
        Incoming::Auth => Loop::Stop(AppError::Validation(
            "mqtt extended authentication is not supported".into(),
        )),
        Incoming::ConnAck(_) | Incoming::PingResp => Loop::Continue,
        Incoming::PubAck(packet_id) => {
            tracing::debug!(packet_id, "mqtt puback");
            Loop::Continue
        }
        Incoming::PubComp(packet_id) => {
            tracing::debug!(packet_id, "mqtt pubcomp");
            Loop::Continue
        }
    }
}

async fn connect_link(endpoint: &Endpoint, verify_ssl: bool) -> AppResult<(Reader, Writer)> {
    match endpoint.kind {
        TransportKind::Tcp => {
            let stream = tcp_connect(&endpoint.host, endpoint.port).await?;
            let (read, write) = tokio::io::split(stream);
            Ok((Reader::Tcp(read), Writer::Tcp(write)))
        }
        TransportKind::Tls => {
            let stream = tls_connect(&endpoint.host, endpoint.port, verify_ssl).await?;
            let (read, write) = tokio::io::split(stream);
            Ok((Reader::Tls(read), Writer::Tls(write)))
        }
        TransportKind::WebSocket { tls } => {
            let url = websocket_url(endpoint, tls);
            let mut request = url.clone().into_client_request().map_err(|error| {
                AppError::Validation(format!("invalid websocket broker url: {error}"))
            })?;
            request.headers_mut().insert(
                http::header::SEC_WEBSOCKET_PROTOCOL,
                http::HeaderValue::from_static("mqtt"),
            );
            let (socket, _) =
                tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
                    .await
                    .map_err(|_| AppError::Timeout(format!("timed out connecting to {url}")))?
                    .map_err(|error| AppError::Network(error.to_string()))?;
            let (write, read) = socket.split();
            Ok((Reader::Ws(read), Writer::Ws(write)))
        }
    }
}

fn websocket_url(endpoint: &Endpoint, tls: bool) -> String {
    let scheme = if tls { "wss" } else { "ws" };
    let host = if endpoint.host.contains(':') {
        format!("[{}]", endpoint.host)
    } else {
        endpoint.host.clone()
    };
    format!("{scheme}://{host}:{}{}", endpoint.port, endpoint.path)
}

async fn tcp_connect(host: &str, port: u16) -> AppResult<TcpStream> {
    tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| AppError::Timeout(format!("timed out connecting to {host}:{port}")))?
        .map_err(|error| AppError::Network(error.to_string()))
}

async fn tls_connect(
    host: &str,
    port: u16,
    verify_ssl: bool,
) -> AppResult<tokio_rustls::client::TlsStream<TcpStream>> {
    let tcp = tcp_connect(host, port).await?;
    let config = if verify_ssl {
        native_roots_config()?
    } else {
        super::transport::client_config_insecure()
    };
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| AppError::Validation(format!("invalid tls server name: {host}")))?;
    tokio_rustls::TlsConnector::from(config)
        .connect(server_name, tcp)
        .await
        .map_err(|error| AppError::Tls(error.to_string()))
}

fn native_roots_config() -> AppResult<std::sync::Arc<rustls::ClientConfig>> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let loaded = rustls_native_certs::load_native_certs();
    let mut roots = rustls::RootCertStore::empty();
    let mut added = 0;
    for cert in loaded.certs {
        if roots.add(cert).is_ok() {
            added += 1;
        }
    }
    if added == 0 {
        let detail = if loaded.errors.is_empty() {
            "no native root certificates".to_string()
        } else {
            loaded
                .errors
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        };
        return Err(AppError::Tls(detail));
    }
    Ok(std::sync::Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

async fn read_some(reader: &mut Reader, buf: &mut BytesMut) -> AppResult<usize> {
    match reader {
        Reader::Tcp(stream) => read_byte(stream, buf).await,
        Reader::Tls(stream) => read_byte(stream, buf).await,
        Reader::Ws(socket) => loop {
            let Some(message) = socket.next().await else {
                return Ok(0);
            };
            let message = message.map_err(|error| AppError::Network(error.to_string()))?;
            match message {
                tokio_tungstenite::tungstenite::Message::Binary(bytes) => {
                    let len = bytes.len();
                    buf.extend_from_slice(&bytes);
                    return Ok(len);
                }
                tokio_tungstenite::tungstenite::Message::Text(text) => {
                    let raw = text.as_bytes();
                    buf.extend_from_slice(raw);
                    return Ok(raw.len());
                }
                tokio_tungstenite::tungstenite::Message::Close(_) => return Ok(0),
                tokio_tungstenite::tungstenite::Message::Ping(_)
                | tokio_tungstenite::tungstenite::Message::Pong(_)
                | tokio_tungstenite::tungstenite::Message::Frame(_) => {}
            }
        },
    }
}

async fn read_byte<S>(stream: &mut S, buf: &mut BytesMut) -> AppResult<usize>
where
    S: AsyncRead + Unpin,
{
    buf.reserve(4096);
    stream
        .read_buf(buf)
        .await
        .map_err(|error| AppError::Network(error.to_string()))
}

async fn write_bytes(writer: &mut Writer, bytes: &[u8]) -> AppResult<()> {
    match writer {
        Writer::Tcp(stream) => write_byte(stream, bytes).await,
        Writer::Tls(stream) => write_byte(stream, bytes).await,
        Writer::Ws(socket) => socket
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                bytes.to_vec().into(),
            ))
            .await
            .map_err(|error| AppError::Network(error.to_string())),
    }
}

async fn write_byte<S>(stream: &mut S, bytes: &[u8]) -> AppResult<()>
where
    S: AsyncWrite + Unpin,
{
    stream
        .write_all(bytes)
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    stream
        .flush()
        .await
        .map_err(|error| AppError::Network(error.to_string()))
}

async fn next_incoming(
    reader: &mut Reader,
    version: Version,
    max_packet: u32,
    buf: &mut BytesMut,
) -> AppResult<Incoming> {
    loop {
        if let Some(packet) = decode_one(version, max_packet, buf)? {
            return Ok(packet);
        }
        let n = read_some(reader, buf).await?;
        if n == 0 {
            return Err(AppError::Network("mqtt connection closed".into()));
        }
    }
}

fn decode_one(
    version: Version,
    max_packet: u32,
    buf: &mut BytesMut,
) -> AppResult<Option<Incoming>> {
    match version {
        Version::V4 => {
            let mut codec = mqtt_codec::v4::MqttCodec::new();
            match codec.decode(buf) {
                Ok(Some(packet)) => Ok(Some(incoming_v4(packet))),
                Ok(None) => Ok(None),
                Err(error) => Err(codec_error(error)),
            }
        }
        Version::V5 => {
            let mut codec = v5_codec(max_packet);
            match codec.decode(buf) {
                Ok(Some(packet)) => Ok(Some(incoming_v5(packet))),
                Ok(None) => Ok(None),
                Err(error) => Err(codec_error(error)),
            }
        }
    }
}

fn v5_codec(max_packet: u32) -> mqtt_codec::v5::MqttCodec {
    let mut codec = mqtt_codec::v5::MqttCodec::new();
    if max_packet > 0 {
        codec.set_max_packet_size(max_packet);
    }
    codec
}

fn incoming_v4(packet: mqtt_codec::v4::Packet) -> Incoming {
    use mqtt_codec::v4::Packet;
    match packet {
        Packet::ConnAck(ack) => Incoming::ConnAck(ConnAckInfo {
            ok: ack.return_code.is_success(),
            reason: ack.return_code.to_string(),
            server_keep_alive: None,
            max_packet_size: None,
        }),
        Packet::Publish(publish) => Incoming::Publish(Published {
            topic: publish.topic_name,
            payload: publish.payload.to_vec(),
            qos: u8::from(publish.qos),
            retain: publish.retain,
            packet_id: publish.packet_id,
        }),
        Packet::PubAck(ack) => Incoming::PubAck(ack.packet_id),
        Packet::PubRec(ack) => Incoming::PubRec(ack.packet_id),
        Packet::PubRel(ack) => Incoming::PubRel(ack.packet_id),
        Packet::PubComp(ack) => Incoming::PubComp(ack.packet_id),
        Packet::SubAck(ack) => {
            let ok = ack.return_codes.iter().all(|code| code.is_success());
            let detail = ack
                .return_codes
                .iter()
                .map(|code| code.as_u8().to_string())
                .collect::<Vec<_>>()
                .join(",");
            Incoming::SubAck(AckInfo {
                packet_id: ack.packet_id,
                ok,
                detail,
            })
        }
        Packet::UnsubAck(ack) => Incoming::UnsubAck(ack.packet_id),
        Packet::PingResp(_) => Incoming::PingResp,
        Packet::Disconnect(_) => Incoming::Disconnect(String::new()),
        Packet::Connect(_) | Packet::Subscribe(_) | Packet::Unsubscribe(_) | Packet::PingReq(_) => {
            Incoming::Disconnect("unexpected packet from broker".into())
        }
    }
}

fn incoming_v5(packet: mqtt_codec::v5::Packet) -> Incoming {
    use mqtt_codec::v5::Packet;
    match packet {
        Packet::ConnAck(ack) => Incoming::ConnAck(ConnAckInfo {
            ok: ack.reason_code == mqtt_codec::v5::ReasonCode::Success,
            reason: format!("{:?}", ack.reason_code),
            server_keep_alive: ack.properties.server_keep_alive,
            max_packet_size: ack.properties.maximum_packet_size,
        }),
        Packet::Publish(publish) => Incoming::Publish(Published {
            topic: publish.topic_name,
            payload: publish.payload.to_vec(),
            qos: u8::from(publish.qos),
            retain: publish.retain,
            packet_id: publish.packet_id,
        }),
        Packet::PubAck(ack) => Incoming::PubAck(ack.packet_id),
        Packet::PubRec(ack) => Incoming::PubRec(ack.packet_id),
        Packet::PubRel(ack) => Incoming::PubRel(ack.packet_id),
        Packet::PubComp(ack) => Incoming::PubComp(ack.packet_id),
        Packet::SubAck(ack) => {
            let ok = ack.reason_codes.iter().all(|code| code.is_success());
            let detail = ack
                .reason_codes
                .iter()
                .map(|code| format!("{code:?}"))
                .collect::<Vec<_>>()
                .join(",");
            Incoming::SubAck(AckInfo {
                packet_id: ack.packet_id,
                ok,
                detail,
            })
        }
        Packet::UnsubAck(ack) => Incoming::UnsubAck(ack.packet_id),
        Packet::PingResp(_) => Incoming::PingResp,
        Packet::Disconnect(packet) => Incoming::Disconnect(format!("{:?}", packet.reason_code)),
        Packet::Auth(_) => Incoming::Auth,
        Packet::Connect(_) | Packet::Subscribe(_) | Packet::Unsubscribe(_) | Packet::PingReq(_) => {
            Incoming::Disconnect("unexpected packet from broker".into())
        }
    }
}

async fn send_connect(
    writer: &mut Writer,
    version: Version,
    config: &MqttConnect,
) -> AppResult<()> {
    let packet = match version {
        Version::V4 => {
            let mut builder = mqtt_codec::v4::ConnectPacket::builder(config.client_id.clone())
                .keep_alive(config.keep_alive)
                .clean_session(config.clean_session);
            if !config.username.is_empty() {
                builder = builder.username(config.username.clone());
            }
            if !config.password.is_empty() {
                builder = builder.password(Bytes::from(config.password.clone()));
            }
            Outgoing::V4(mqtt_codec::v4::Packet::Connect(builder.build()))
        }
        Version::V5 => {
            let mut builder = mqtt_codec::v5::ConnectPacket::builder(config.client_id.clone())
                .keep_alive(config.keep_alive)
                .clean_start(config.clean_session);
            if !config.username.is_empty() {
                builder = builder.username(config.username.clone());
            }
            if !config.password.is_empty() {
                builder = builder.password(Bytes::from(config.password.clone()));
            }
            v5(mqtt_codec::v5::Packet::Connect(builder.build()))
        }
    };
    send_simple(writer, version, 0, packet).await
}

async fn send_subscribe(session: &mut Session<'_>, topic: &str, qos: u8) -> AppResult<()> {
    let packet_id = session.ids.alloc();
    let packet = match session.version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::Subscribe(
            mqtt_codec::v4::SubscribePacket::new(packet_id, topic, v4_qos(qos)?),
        )),
        Version::V5 => {
            let subscribe = mqtt_codec::v5::SubscribePacketBuilder::new(packet_id)
                .topic(topic, v5_qos(qos)?)
                .build()
                .ok_or_else(|| AppError::Validation("subscribe needs a topic filter".into()))?;
            v5(mqtt_codec::v5::Packet::Subscribe(subscribe))
        }
    };
    write_packet(session, packet).await?;
    session.pending_sub.insert(packet_id, topic.to_string());
    Ok(())
}

async fn send_unsubscribe(session: &mut Session<'_>, topic: &str) -> AppResult<()> {
    let packet_id = session.ids.alloc();
    let packet = match session.version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::Unsubscribe(
            mqtt_codec::v4::UnsubscribePacket::new(packet_id, topic),
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::Unsubscribe(
            mqtt_codec::v5::UnsubscribePacket {
                packet_id,
                properties: mqtt_codec::v5::Properties::new(),
                topics: vec![topic.to_string()],
            },
        )),
    };
    write_packet(session, packet).await
}

async fn send_publish(
    id: &str,
    sink: &MqttSink,
    session: &mut Session<'_>,
    topic: &str,
    payload: Vec<u8>,
    qos: u8,
    retain: bool,
) -> AppResult<()> {
    let packet_id = if qos == 0 {
        None
    } else {
        Some(session.ids.alloc())
    };
    let bytes = Bytes::from(payload.clone());
    let packet = match session.version {
        Version::V4 => {
            let mut builder = mqtt_codec::v4::PublishPacketBuilder::new(topic, bytes)
                .qos(v4_qos(qos)?)
                .retain(retain);
            if let Some(packet_id) = packet_id {
                builder = builder.packet_id(packet_id);
            }
            Outgoing::V4(mqtt_codec::v4::Packet::Publish(builder.build()))
        }
        Version::V5 => {
            let mut builder = mqtt_codec::v5::PublishPacketBuilder::new(topic, bytes)
                .qos(v5_qos(qos)?)
                .retain(retain);
            if let Some(packet_id) = packet_id {
                builder = builder.packet_id(packet_id);
            }
            v5(mqtt_codec::v5::Packet::Publish(builder.build()))
        }
    };
    write_packet(session, packet).await?;
    if qos == 2
        && let Some(packet_id) = packet_id
    {
        session.qos2_out.insert(packet_id);
    }
    emit_message(sink, id, "out", topic, &payload, qos, retain);
    Ok(())
}

fn puback(version: Version, packet_id: u16) -> Outgoing {
    match version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::PubAck(
            mqtt_codec::v4::PubAckPacket { packet_id },
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::PubAck(
            mqtt_codec::v5::PubAckPacket {
                packet_id,
                reason_code: mqtt_codec::v5::ReasonCode::Success,
                properties: mqtt_codec::v5::Properties::new(),
            },
        )),
    }
}

fn pubrec(version: Version, packet_id: u16) -> Outgoing {
    match version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::PubRec(
            mqtt_codec::v4::PubRecPacket { packet_id },
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::PubRec(
            mqtt_codec::v5::PubRecPacket {
                packet_id,
                reason_code: mqtt_codec::v5::ReasonCode::Success,
                properties: mqtt_codec::v5::Properties::new(),
            },
        )),
    }
}

fn pubrel(version: Version, packet_id: u16) -> Outgoing {
    match version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::PubRel(
            mqtt_codec::v4::PubRelPacket { packet_id },
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::PubRel(
            mqtt_codec::v5::PubRelPacket {
                packet_id,
                reason_code: mqtt_codec::v5::ReasonCode::Success,
                properties: mqtt_codec::v5::Properties::new(),
            },
        )),
    }
}

fn pubcomp(version: Version, packet_id: u16) -> Outgoing {
    match version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::PubComp(
            mqtt_codec::v4::PubCompPacket { packet_id },
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::PubComp(
            mqtt_codec::v5::PubCompPacket {
                packet_id,
                reason_code: mqtt_codec::v5::ReasonCode::Success,
                properties: mqtt_codec::v5::Properties::new(),
            },
        )),
    }
}

async fn send_ping(session: &mut Session<'_>) -> AppResult<()> {
    let packet = match session.version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::PingReq(
            mqtt_codec::v4::PingReqPacket,
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::PingReq(
            mqtt_codec::v5::PingReqPacket,
        )),
    };
    write_packet(session, packet).await
}

async fn send_disconnect(session: &mut Session<'_>) -> AppResult<()> {
    let packet = match session.version {
        Version::V4 => Outgoing::V4(mqtt_codec::v4::Packet::Disconnect(
            mqtt_codec::v4::DisconnectPacket,
        )),
        Version::V5 => v5(mqtt_codec::v5::Packet::Disconnect(
            mqtt_codec::v5::DisconnectPacket::default(),
        )),
    };
    write_packet(session, packet).await
}

async fn write_packet(session: &mut Session<'_>, packet: Outgoing) -> AppResult<()> {
    send_simple(session.writer, session.version, session.max_packet, packet).await
}

async fn send_simple(
    writer: &mut Writer,
    version: Version,
    max_packet: u32,
    packet: Outgoing,
) -> AppResult<()> {
    let bytes = encode(version, max_packet, packet)?;
    write_bytes(writer, &bytes).await
}

fn encode(version: Version, max_packet: u32, packet: Outgoing) -> AppResult<BytesMut> {
    let mut dst = BytesMut::new();
    match (version, packet) {
        (Version::V4, Outgoing::V4(packet)) => {
            let mut codec = mqtt_codec::v4::MqttCodec::new();
            codec.encode(packet, &mut dst).map_err(codec_error)?;
        }
        (Version::V5, Outgoing::V5(packet)) => {
            let mut codec = v5_codec(max_packet);
            codec.encode(*packet, &mut dst).map_err(codec_error)?;
        }
        _ => {
            return Err(AppError::Internal(
                "mqtt packet version does not match the session".into(),
            ));
        }
    }
    Ok(dst)
}

fn v4_qos(qos: u8) -> AppResult<mqtt_codec::v4::QoS> {
    match qos {
        0 => Ok(mqtt_codec::v4::QoS::AtMostOnce),
        1 => Ok(mqtt_codec::v4::QoS::AtLeastOnce),
        2 => Ok(mqtt_codec::v4::QoS::ExactlyOnce),
        _ => Err(AppError::Validation("qos must be 0, 1, or 2".into())),
    }
}

fn v5_qos(qos: u8) -> AppResult<mqtt_codec::v5::QoS> {
    match qos {
        0 => Ok(mqtt_codec::v5::QoS::AtMostOnce),
        1 => Ok(mqtt_codec::v5::QoS::AtLeastOnce),
        2 => Ok(mqtt_codec::v5::QoS::ExactlyOnce),
        _ => Err(AppError::Validation("qos must be 0, 1, or 2".into())),
    }
}

fn codec_error(error: mqtt_codec::MqttError) -> AppError {
    AppError::Validation(error.to_string())
}

fn check_qos(qos: u8) -> AppResult<()> {
    if qos > 2 {
        Err(AppError::Validation("qos must be 0, 1, or 2".into()))
    } else {
        Ok(())
    }
}

fn check_filter(topic: &str) -> AppResult<()> {
    if topic.trim().is_empty() || topic.contains('\0') {
        Err(AppError::Validation("topic filter is required".into()))
    } else {
        Ok(())
    }
}

fn check_publish_topic(topic: &str) -> AppResult<()> {
    if topic.is_empty() || topic.contains('\0') || topic.contains('+') || topic.contains('#') {
        Err(AppError::Validation(
            "publish topic must be a non-empty name without wildcards".into(),
        ))
    } else {
        Ok(())
    }
}

fn ping_period(keep_alive: u16) -> Duration {
    if keep_alive == 0 {
        Duration::from_secs(3600)
    } else {
        Duration::from_secs((u64::from(keep_alive) * 3 / 4).max(1))
    }
}

fn payload_text(bytes: &[u8]) -> String {
    use base64::Engine;
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            format!(
                "base64:{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        }
    }
}

fn emit_message(
    sink: &MqttSink,
    id: &str,
    direction: &str,
    topic: &str,
    payload: &[u8],
    qos: u8,
    retain: bool,
) {
    (sink.message)(MqttMessageEvent {
        connection_id: id.to_string(),
        direction: direction.to_string(),
        topic: topic.to_string(),
        payload: payload_text(payload),
        qos,
        retain,
        timestamp: crate::db::now_ms(),
    });
}

fn emit_status(sink: &MqttSink, id: &str, status: &str, reason: Option<String>) {
    (sink.status)(MqttStatusEvent {
        connection_id: id.to_string(),
        status: status.to_string(),
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[test]
    fn parses_broker_urls() {
        let tcp = parse_broker("mqtt://127.0.0.1:1883", false).unwrap();
        assert!(matches!(tcp.kind, TransportKind::Tcp));
        assert_eq!(tcp.port, 1883);

        let tls = parse_broker("mqtts://localhost", false).unwrap();
        assert!(matches!(tls.kind, TransportKind::Tls));
        assert_eq!(tls.port, 8883);

        let forced = parse_broker("mqtt://localhost:1883", true).unwrap();
        assert!(matches!(forced.kind, TransportKind::Tls));
        assert_eq!(forced.port, 1883);

        let plain = parse_broker("localhost:1884", false).unwrap();
        assert!(matches!(plain.kind, TransportKind::Tcp));
        assert_eq!(plain.port, 1884);

        let ws = parse_broker("ws://localhost:8083", false).unwrap();
        assert!(matches!(ws.kind, TransportKind::WebSocket { tls: false }));
        assert_eq!(ws.path, "/mqtt");

        let wss = parse_broker("wss://localhost/custom", false).unwrap();
        assert!(matches!(wss.kind, TransportKind::WebSocket { tls: true }));
        assert_eq!(wss.port, 443);
        assert_eq!(wss.path, "/custom");

        assert!(parse_broker("ftp://localhost", false).is_err());
        assert!(parse_broker("", false).is_err());
    }

    #[test]
    fn rejects_password_without_username() {
        let registry = Arc::new(MqttRegistry::new());
        let error = registry
            .connect(sample_connect("mqtt://127.0.0.1:1", 4), sink_pair().0)
            .expect_err("password without username");
        assert!(error.to_string().contains("username"));
    }

    #[tokio::test]
    async fn v4_tcp_publish_is_delivered_back() {
        let (address, task) = spawn_v4_tcp().await;
        round_trip(&format!("mqtt://{address}"), 4, false).await;
        task.abort();
    }

    #[tokio::test]
    async fn v5_tcp_publish_is_delivered_back() {
        let (address, task) = spawn_v5_tcp().await;
        round_trip(&format!("mqtt://{address}"), 5, false).await;
        task.abort();
    }

    #[tokio::test]
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite handshake errors are full HTTP responses"
    )]
    async fn v4_websocket_publish_is_delivered_back() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_hdr_async(
                stream,
                |request: &http::Request<()>, mut response: http::Response<()>| {
                    if let Some(protocol) =
                        request.headers().get(http::header::SEC_WEBSOCKET_PROTOCOL)
                    {
                        response
                            .headers_mut()
                            .insert(http::header::SEC_WEBSOCKET_PROTOCOL, protocol.clone());
                    }
                    Ok::<_, http::Response<Option<String>>>(response)
                },
            )
            .await
            .unwrap();
            drive_v4_ws(socket).await;
        });
        round_trip(&format!("ws://{address}/mqtt"), 4, false).await;
        task.abort();
    }

    #[tokio::test]
    async fn v4_tls_publish_is_delivered_back() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let acceptor = tls_acceptor();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = acceptor.accept(stream).await.unwrap();
            drive_v4_byte(stream).await;
        });
        round_trip(&format!("mqtts://{address}"), 4, true).await;
        task.abort();
    }

    async fn round_trip(url: &str, level: u8, tls: bool) {
        let (sink, messages, statuses) = sink_pair();
        let registry = Arc::new(MqttRegistry::new());
        let mut config = sample_connect(url, level);
        config.password.clear();
        config.tls = tls;
        config.verify_ssl = false;
        config.subscriptions = vec![("sensors/temp".into(), 1)];
        let id = registry.connect(config, sink).unwrap();
        wait_status(&statuses, "subscribed").await;
        registry
            .publish(&id, "sensors/temp", b"21".to_vec(), 1, false)
            .unwrap();
        let incoming = wait_payload(&messages, "21").await;
        assert_eq!(incoming.topic, "sensors/temp");
        assert_eq!(incoming.qos, 0);
        registry
            .publish(&id, "sensors/temp", b"22".to_vec(), 2, false)
            .unwrap();
        let exactly_once = wait_payload(&messages, "22").await;
        assert_eq!(exactly_once.topic, "sensors/temp");
        assert_eq!(exactly_once.qos, 0);
        registry.disconnect(&id);
        wait_status(&statuses, "closed").await;
    }

    fn sample_connect(url: &str, level: u8) -> MqttConnect {
        MqttConnect {
            broker_url: url.to_string(),
            tls: false,
            verify_ssl: false,
            client_id: "thread-test".into(),
            username: String::new(),
            password: "secret".into(),
            keep_alive: 30,
            clean_session: true,
            protocol_level: level,
            subscriptions: Vec::new(),
        }
    }

    fn sink_pair() -> CapturedEvents {
        let messages = Arc::new(StdMutex::new(Vec::new()));
        let statuses = Arc::new(StdMutex::new(Vec::new()));
        let messages_sink = Arc::clone(&messages);
        let statuses_sink = Arc::clone(&statuses);
        let sink = MqttSink::new(
            move |event| messages_sink.lock().unwrap().push(event),
            move |event| statuses_sink.lock().unwrap().push(event),
        );
        (sink, messages, statuses)
    }

    type CapturedEvents = (
        MqttSink,
        Arc<StdMutex<Vec<MqttMessageEvent>>>,
        Arc<StdMutex<Vec<MqttStatusEvent>>>,
    );

    async fn wait_status(statuses: &StdMutex<Vec<MqttStatusEvent>>, kind: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if statuses
                .lock()
                .unwrap()
                .iter()
                .any(|event| event.status == kind)
            {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                panic!(
                    "timed out waiting for {kind}: {:?}",
                    statuses.lock().unwrap()
                );
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn wait_payload(
        messages: &StdMutex<Vec<MqttMessageEvent>>,
        payload: &str,
    ) -> MqttMessageEvent {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(event) = messages
                .lock()
                .unwrap()
                .iter()
                .find(|event| event.direction == "in" && event.payload == payload)
                .cloned()
            {
                return event;
            }
            if tokio::time::Instant::now() > deadline {
                panic!(
                    "timed out waiting for publish: {:?}",
                    messages.lock().unwrap()
                );
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn spawn_v4_tcp() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drive_v4_byte(stream).await;
        });
        (address, task)
    }

    async fn spawn_v5_tcp() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drive_v5_byte(stream).await;
        });
        (address, task)
    }

    async fn drive_v4_byte<S>(mut stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut codec = mqtt_codec::v4::MqttCodec::new();
        let mut buf = BytesMut::new();
        let mut subs = Vec::new();
        let mut held = HashMap::new();
        loop {
            let packet = read_codec(&mut stream, &mut codec, &mut buf).await;
            let reply = respond_v4(packet, &mut subs, &mut held);
            for packet in reply.packets {
                write_codec(&mut stream, &mut codec, packet).await;
            }
            if reply.close {
                break;
            }
        }
    }

    async fn drive_v4_ws(socket: tokio_tungstenite::WebSocketStream<TcpStream>) {
        let (mut write, mut read) = socket.split();
        let mut codec = mqtt_codec::v4::MqttCodec::new();
        let mut buf = BytesMut::new();
        let mut subs = Vec::new();
        let mut held = HashMap::new();
        loop {
            let message = read.next().await.unwrap().unwrap();
            let tokio_tungstenite::tungstenite::Message::Binary(bytes) = message else {
                continue;
            };
            buf.extend_from_slice(&bytes);
            while let Some(packet) = codec.decode(&mut buf).unwrap() {
                let reply = respond_v4(packet, &mut subs, &mut held);
                for packet in reply.packets {
                    let mut dst = BytesMut::new();
                    codec.encode(packet, &mut dst).unwrap();
                    write
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            dst.to_vec().into(),
                        ))
                        .await
                        .unwrap();
                }
                if reply.close {
                    return;
                }
            }
        }
    }

    async fn drive_v5_byte<S>(mut stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut codec = mqtt_codec::v5::MqttCodec::new();
        let mut buf = BytesMut::new();
        let mut subs = Vec::new();
        let mut held = HashMap::new();
        loop {
            let packet = read_codec(&mut stream, &mut codec, &mut buf).await;
            let reply = respond_v5(packet, &mut subs, &mut held);
            for packet in reply.packets {
                write_codec(&mut stream, &mut codec, packet).await;
            }
            if reply.close {
                break;
            }
        }
    }

    async fn read_codec<S, C>(stream: &mut S, codec: &mut C, buf: &mut BytesMut) -> C::Item
    where
        S: AsyncRead + Unpin,
        C: Decoder,
        C::Error: std::fmt::Debug,
    {
        loop {
            if let Some(packet) = codec.decode(buf).unwrap() {
                return packet;
            }
            let n = stream.read_buf(buf).await.unwrap();
            assert!(n > 0, "broker connection closed");
        }
    }

    async fn write_codec<S, C, I>(stream: &mut S, codec: &mut C, packet: I)
    where
        S: AsyncWrite + Unpin,
        C: Encoder<I>,
        <C as Encoder<I>>::Error: std::fmt::Debug,
    {
        let mut dst = BytesMut::new();
        codec.encode(packet, &mut dst).unwrap();
        stream.write_all(&dst).await.unwrap();
        stream.flush().await.unwrap();
    }

    struct Reply<P> {
        packets: Vec<P>,
        close: bool,
    }

    fn respond_v4(
        packet: mqtt_codec::v4::Packet,
        subs: &mut Vec<String>,
        held: &mut HashMap<u16, mqtt_codec::v4::PublishPacket>,
    ) -> Reply<mqtt_codec::v4::Packet> {
        use mqtt_codec::v4::{
            ConnAckPacket, Packet, PubAckPacket, PubCompPacket, PubRecPacket, PublishPacket,
            SubAckPacket, SubAckReturnCode, UnsubAckPacket,
        };
        match packet {
            Packet::Connect(_) => Reply {
                packets: vec![Packet::ConnAck(ConnAckPacket::accepted(false))],
                close: false,
            },
            Packet::Subscribe(subscribe) => {
                let packet_id = subscribe.packet_id;
                let codes = subscribe
                    .topics
                    .iter()
                    .map(|(_, qos)| match qos {
                        mqtt_codec::v4::QoS::AtMostOnce => SubAckReturnCode::SuccessQoS0,
                        mqtt_codec::v4::QoS::AtLeastOnce => SubAckReturnCode::SuccessQoS1,
                        mqtt_codec::v4::QoS::ExactlyOnce => SubAckReturnCode::SuccessQoS2,
                    })
                    .collect();
                subs.extend(subscribe.topics.into_iter().map(|(topic, _)| topic));
                Reply {
                    packets: vec![Packet::SubAck(SubAckPacket::new(packet_id, codes))],
                    close: false,
                }
            }
            Packet::Unsubscribe(unsubscribe) => {
                let packet_id = unsubscribe.packet_id;
                subs.retain(|topic| !unsubscribe.topics.contains(topic));
                Reply {
                    packets: vec![Packet::UnsubAck(UnsubAckPacket { packet_id })],
                    close: false,
                }
            }
            Packet::Publish(publish) => {
                let mut packets = Vec::new();
                if let Some(packet_id) = publish.packet_id {
                    match publish.qos {
                        mqtt_codec::v4::QoS::AtLeastOnce => {
                            packets.push(Packet::PubAck(PubAckPacket { packet_id }));
                        }
                        mqtt_codec::v4::QoS::ExactlyOnce => {
                            packets.push(Packet::PubRec(PubRecPacket { packet_id }));
                            held.insert(packet_id, publish);
                            return Reply {
                                packets,
                                close: false,
                            };
                        }
                        mqtt_codec::v4::QoS::AtMostOnce => {}
                    }
                }
                if subs
                    .iter()
                    .any(|filter| topic_matches(filter, &publish.topic_name))
                {
                    packets.push(Packet::Publish(PublishPacket::new(
                        publish.topic_name,
                        publish.payload,
                    )));
                }
                Reply {
                    packets,
                    close: false,
                }
            }
            Packet::PubRel(rel) => {
                let mut packets = Vec::new();
                if let Some(publish) = held.remove(&rel.packet_id) {
                    packets.push(Packet::Publish(PublishPacket::new(
                        publish.topic_name,
                        publish.payload,
                    )));
                }
                packets.push(Packet::PubComp(PubCompPacket {
                    packet_id: rel.packet_id,
                }));
                Reply {
                    packets,
                    close: false,
                }
            }
            Packet::PingReq(_) => Reply {
                packets: vec![Packet::PingResp(mqtt_codec::v4::PingRespPacket)],
                close: false,
            },
            Packet::Disconnect(_) => Reply {
                packets: Vec::new(),
                close: true,
            },
            _ => Reply {
                packets: Vec::new(),
                close: false,
            },
        }
    }

    fn respond_v5(
        packet: mqtt_codec::v5::Packet,
        subs: &mut Vec<String>,
        held: &mut HashMap<u16, mqtt_codec::v5::PublishPacket>,
    ) -> Reply<mqtt_codec::v5::Packet> {
        use mqtt_codec::v5::{
            ConnAckPacket, Packet, Properties, PubAckPacket, PubCompPacket, PubRecPacket,
            PublishPacket, QoS, ReasonCode, SubAckPacket, UnsubAckPacket,
        };
        let props = Properties::new;
        match packet {
            Packet::Connect(_) => Reply {
                packets: vec![Packet::ConnAck(ConnAckPacket::accepted(false))],
                close: false,
            },
            Packet::Subscribe(subscribe) => {
                let packet_id = subscribe.packet_id;
                let reason_codes = subscribe
                    .topics
                    .iter()
                    .map(|topic| match topic.qos {
                        QoS::AtMostOnce => ReasonCode::Success,
                        QoS::AtLeastOnce => ReasonCode::GrantedQoS1,
                        QoS::ExactlyOnce => ReasonCode::GrantedQoS2,
                    })
                    .collect();
                subs.extend(subscribe.topics.into_iter().map(|topic| topic.topic_filter));
                Reply {
                    packets: vec![Packet::SubAck(SubAckPacket {
                        packet_id,
                        properties: props(),
                        reason_codes,
                    })],
                    close: false,
                }
            }
            Packet::Unsubscribe(unsubscribe) => {
                let packet_id = unsubscribe.packet_id;
                let reason_codes = vec![ReasonCode::Success; unsubscribe.topics.len()];
                subs.retain(|topic| !unsubscribe.topics.contains(topic));
                Reply {
                    packets: vec![Packet::UnsubAck(UnsubAckPacket {
                        packet_id,
                        properties: props(),
                        reason_codes,
                    })],
                    close: false,
                }
            }
            Packet::Publish(publish) => {
                let mut packets = Vec::new();
                if let Some(packet_id) = publish.packet_id {
                    match publish.qos {
                        QoS::AtLeastOnce => packets.push(Packet::PubAck(PubAckPacket {
                            packet_id,
                            reason_code: ReasonCode::Success,
                            properties: props(),
                        })),
                        QoS::ExactlyOnce => {
                            packets.push(Packet::PubRec(PubRecPacket {
                                packet_id,
                                reason_code: ReasonCode::Success,
                                properties: props(),
                            }));
                            held.insert(packet_id, publish);
                            return Reply {
                                packets,
                                close: false,
                            };
                        }
                        QoS::AtMostOnce => {}
                    }
                }
                if subs
                    .iter()
                    .any(|filter| topic_matches(filter, &publish.topic_name))
                {
                    packets.push(Packet::Publish(PublishPacket::new(
                        publish.topic_name,
                        publish.payload,
                    )));
                }
                Reply {
                    packets,
                    close: false,
                }
            }
            Packet::PubRel(rel) => {
                let mut packets = Vec::new();
                if let Some(publish) = held.remove(&rel.packet_id) {
                    packets.push(Packet::Publish(PublishPacket::new(
                        publish.topic_name,
                        publish.payload,
                    )));
                }
                packets.push(Packet::PubComp(PubCompPacket {
                    packet_id: rel.packet_id,
                    reason_code: ReasonCode::Success,
                    properties: props(),
                }));
                Reply {
                    packets,
                    close: false,
                }
            }
            Packet::PingReq(_) => Reply {
                packets: vec![Packet::PingResp(mqtt_codec::v5::PingRespPacket)],
                close: false,
            },
            Packet::Disconnect(_) => Reply {
                packets: Vec::new(),
                close: true,
            },
            _ => Reply {
                packets: Vec::new(),
                close: false,
            },
        }
    }

    fn topic_matches(filter: &str, topic: &str) -> bool {
        let filter_levels: Vec<&str> = filter.split('/').collect();
        let topic_levels: Vec<&str> = topic.split('/').collect();
        let mut topic_index = 0;
        for (filter_index, level) in filter_levels.iter().enumerate() {
            if *level == "#" {
                return filter_index + 1 == filter_levels.len();
            }
            if topic_index >= topic_levels.len() {
                return false;
            }
            if *level != "+" && *level != topic_levels[topic_index] {
                return false;
            }
            topic_index += 1;
        }
        topic_index == topic_levels.len()
    }

    fn tls_acceptor() -> tokio_rustls::TlsAcceptor {
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
        let certificate = params.self_signed(&key).unwrap();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()),
        );
        let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.der().clone()], key_der)
            .unwrap();
        tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config))
    }
}
