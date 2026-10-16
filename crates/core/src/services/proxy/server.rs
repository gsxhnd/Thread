//! Connection handling for the capture proxy.
//!
//! Each accepted socket is served by hyper's HTTP/1 server. Absolute-form requests are
//! forwarded upstream; `CONNECT` either relays bytes or, when interception is allowed,
//! terminates TLS with a leaf certificate from the local CA and forwards the inner
//! requests. Bodies stream through [`Tap`], so entries are finalized when the response
//! body finishes (or is abandoned) rather than after buffering it.

use std::convert::Infallible;
use std::error::Error as StdError;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, Request, Response, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

use crate::db::{new_id, now_ms};
use crate::services::proxy::ProxyEntrySummary;
use crate::services::proxy::Shared;
use crate::services::proxy::capture::{
    CapturedBody, HeaderPair, Tap, redact_headers, redact_url, strip_hop_by_hop,
};
use crate::services::proxy::publish;
use crate::services::proxy::store::StoredEntry;
use crate::services::transport::{BoxError, ProxyBody, proxy_client};

const UPSTREAM_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
const TUNNEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Where a request goes and how its entry is labelled.
#[derive(Debug, Clone)]
struct Route {
    scheme: &'static str,
    host: String,
    port: u16,
    mode: &'static str,
    decrypted: bool,
}

/// Request-side facts captured before the upstream call.
struct Draft {
    id: String,
    captured_at: i64,
    started: Instant,
    method: String,
    url: String,
    path: String,
    route: Route,
    request_headers: Vec<HeaderPair>,
    request_body: Arc<Mutex<CapturedBody>>,
}

/// Response-side facts. `body` is the live buffer filled while the body streams.
struct ResponseSide {
    status: Option<StatusCode>,
    headers: Vec<HeaderPair>,
    content_type: Option<String>,
    ttfb_ms: Option<u64>,
    body: Option<Arc<Mutex<CapturedBody>>>,
}

pub async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let shutdown = shared.shutdown.clone();
                    tokio::select! {
                        _ = serve_client(stream, shared) => {}
                        _ = until_shutdown(shutdown) => {}
                    }
                });
            }
            Err(error) => {
                tracing::warn!(?error, "proxy accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn serve_client(stream: TcpStream, shared: Arc<Shared>) {
    let service = service_fn(move |req| handle(req, Arc::clone(&shared)));
    if let Err(error) = http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades()
        .await
    {
        tracing::debug!(?error, "proxy client connection ended");
    }
}

async fn until_shutdown(mut rx: watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

async fn handle(
    req: Request<Incoming>,
    shared: Arc<Shared>,
) -> Result<Response<ProxyBody>, Infallible> {
    if req.method() == Method::CONNECT {
        return Ok(handle_connect(req, shared));
    }
    Ok(handle_plain(req, &shared).await)
}

async fn handle_plain(req: Request<Incoming>, shared: &Arc<Shared>) -> Response<ProxyBody> {
    let uri = req.uri().clone();
    match (uri.scheme_str(), uri.authority()) {
        (Some("http"), Some(authority)) => {
            let route = Route {
                scheme: "http",
                host: normalize_host(authority.host()),
                port: authority.port_u16().unwrap_or(80),
                mode: "http",
                decrypted: false,
            };
            forward(req, shared, route, uri).await
        }
        (Some("https"), _) => local_response(
            StatusCode::BAD_REQUEST,
            "HTTPS targets must be reached through CONNECT.",
        ),
        _ => local_response(
            StatusCode::BAD_REQUEST,
            "This is the Thread capture proxy. Configure your client to use it as an HTTP proxy.",
        ),
    }
}

fn handle_connect(mut req: Request<Incoming>, shared: Arc<Shared>) -> Response<ProxyBody> {
    let Some(authority) = req.uri().authority().cloned() else {
        return local_response(StatusCode::BAD_REQUEST, "CONNECT requires host:port.");
    };
    let host = normalize_host(authority.host());
    let port = authority.port_u16().unwrap_or(443);
    let upgrade = hyper::upgrade::on(&mut req);

    tokio::spawn(async move {
        let upgraded = match upgrade.await {
            Ok(upgraded) => upgraded,
            Err(error) => {
                tracing::debug!(?error, "CONNECT upgrade failed");
                return;
            }
        };
        let client = TokioIo::new(upgraded);
        let shutdown = shared.shutdown.clone();
        tokio::select! {
            _ = run_connect(client, host, port, shared) => {}
            _ = until_shutdown(shutdown) => {}
        }
    });

    let mut response = Response::new(full_body(Bytes::new()));
    *response.status_mut() = StatusCode::OK;
    response
}

enum ConnectPlan {
    Intercept,
    Tunnel(Option<String>),
}

fn plan_connect(shared: &Shared, host: &str) -> ConnectPlan {
    if shared.bypass.lock().contains(host) {
        return ConnectPlan::Tunnel(Some(
            "The client rejected this host's certificate earlier in the session, so it was relayed without inspection."
                .into(),
        ));
    }
    if !shared.settings.decrypt_https {
        return ConnectPlan::Tunnel(Some(
            "HTTPS decryption is turned off in capture settings; relayed without inspection."
                .into(),
        ));
    }
    if !shared.settings.ca_trust_acknowledged {
        return ConnectPlan::Tunnel(Some(
            "The local CA is not confirmed as trusted; HTTPS was relayed without inspection."
                .into(),
        ));
    }
    ConnectPlan::Intercept
}

async fn run_connect(client: TokioIo<Upgraded>, host: String, port: u16, shared: Arc<Shared>) {
    match plan_connect(&shared, &host) {
        ConnectPlan::Intercept => intercept(client, host, port, shared).await,
        ConnectPlan::Tunnel(note) => tunnel(client, host, port, shared, note).await,
    }
}

/// Relays bytes between client and upstream without looking inside.
async fn tunnel(
    mut client: TokioIo<Upgraded>,
    host: String,
    port: u16,
    shared: Arc<Shared>,
    note: Option<String>,
) {
    let started = Instant::now();
    let captured_at = now_ms();
    let mut upstream = match connect_upstream(&host, port).await {
        Ok(stream) => stream,
        Err(error) => {
            publish_connect(
                &shared,
                &host,
                port,
                captured_at,
                started,
                ConnectOutcome {
                    mode: "tunnel",
                    note,
                    error: Some(error),
                    sent: 0,
                    received: 0,
                },
            );
            return;
        }
    };
    // Byte counts are lost if the relay ends with an I/O error; the entry still records the close.
    let (sent, received) = tokio::io::copy_bidirectional(&mut client, &mut upstream)
        .await
        .unwrap_or((0, 0));
    publish_connect(
        &shared,
        &host,
        port,
        captured_at,
        started,
        ConnectOutcome {
            mode: "tunnel",
            note,
            error: None,
            sent,
            received,
        },
    );
}

/// Terminates TLS with a leaf certificate for `host` and forwards the decrypted requests.
async fn intercept(client: TokioIo<Upgraded>, host: String, port: u16, shared: Arc<Shared>) {
    let started = Instant::now();
    let captured_at = now_ms();

    let config = match shared.ca.server_config(&host) {
        Ok(config) => config,
        Err(error) => {
            publish_connect(
                &shared,
                &host,
                port,
                captured_at,
                started,
                ConnectOutcome {
                    mode: "mitm",
                    note: None,
                    error: Some(format!("cannot issue a certificate for this host: {error}")),
                    sent: 0,
                    received: 0,
                },
            );
            return;
        }
    };

    let tls = match TlsAcceptor::from(config).accept(client).await {
        Ok(tls) => tls,
        Err(error) => {
            let message = if is_client_rejection(&error) {
                shared.bypass.lock().insert(host.clone());
                "The client rejected the proxy certificate. This host is relayed without inspection for the rest of the session."
                    .to_string()
            } else {
                format!("TLS handshake failed: {error}")
            };
            publish_connect(
                &shared,
                &host,
                port,
                captured_at,
                started,
                ConnectOutcome {
                    mode: "mitm",
                    note: None,
                    error: Some(message),
                    sent: 0,
                    received: 0,
                },
            );
            return;
        }
    };

    let service = service_fn(move |req| {
        let shared = Arc::clone(&shared);
        let host = host.clone();
        async move { Ok::<_, Infallible>(handle_intercepted(req, shared, host, port).await) }
    });
    if let Err(error) = http1::Builder::new()
        .serve_connection(TokioIo::new(tls), service)
        .await
    {
        tracing::debug!(?error, "intercepted connection ended");
    }
}

async fn handle_intercepted(
    req: Request<Incoming>,
    shared: Arc<Shared>,
    host: String,
    port: u16,
) -> Response<ProxyBody> {
    if req.method() == Method::CONNECT {
        return local_response(StatusCode::BAD_REQUEST, "Nested CONNECT is not supported.");
    }
    let path = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());
    let target = format!("https://{}{}", authority_string(&host, port, 443), path);
    let uri: Uri = match target.parse() {
        Ok(uri) => uri,
        Err(_) => return local_response(StatusCode::BAD_REQUEST, "Invalid request target."),
    };
    let route = Route {
        scheme: "https",
        host,
        port,
        mode: "mitm",
        decrypted: true,
    };
    forward(req, &shared, route, uri).await
}

/// Forwards one request to `uri` and streams the response back while recording it.
async fn forward(
    req: Request<Incoming>,
    shared: &Arc<Shared>,
    route: Route,
    uri: Uri,
) -> Response<ProxyBody> {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let mut headers = parts.headers;
    strip_hop_by_hop(&mut headers);

    let mask = shared.settings.mask_sensitive;
    let limit = shared.settings.max_body_bytes;
    let draft = shared.recording().then(|| Draft {
        id: new_id(),
        captured_at: now_ms(),
        started: Instant::now(),
        method: method.as_str().to_string(),
        url: redact_url(&uri.to_string(), mask),
        path: uri.path().to_string(),
        route: route.clone(),
        request_headers: redact_headers(&headers, mask),
        request_body: Arc::new(Mutex::new(CapturedBody::default())),
    });

    let request_buffer = draft.as_ref().map(|draft| Arc::clone(&draft.request_body));
    let upstream_body = Tap::new(body, limit, request_buffer)
        .map_err(box_error)
        .boxed_unsync();
    let mut upstream = Request::new(upstream_body);
    *upstream.method_mut() = method;
    *upstream.uri_mut() = uri;
    *upstream.headers_mut() = headers;

    let client = match proxy_client() {
        Ok(client) => client,
        Err(error) => {
            return fail(shared, draft, StatusCode::BAD_GATEWAY, error.to_string());
        }
    };
    let response =
        match tokio::time::timeout(UPSTREAM_RESPONSE_TIMEOUT, client.request(upstream)).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                return fail(
                    shared,
                    draft,
                    StatusCode::BAD_GATEWAY,
                    format!("upstream request failed: {}", describe(&error)),
                );
            }
            Err(_) => {
                return fail(
                    shared,
                    draft,
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream did not respond in time".into(),
                );
            }
        };

    let (mut response_parts, incoming) = response.into_parts();
    strip_hop_by_hop(&mut response_parts.headers);
    let response_buffer = draft
        .as_ref()
        .map(|_| Arc::new(Mutex::new(CapturedBody::default())));
    let side = ResponseSide {
        status: Some(response_parts.status),
        headers: redact_headers(&response_parts.headers, mask),
        content_type: response_parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        ttfb_ms: draft
            .as_ref()
            .map(|draft| draft.started.elapsed().as_millis() as u64),
        body: response_buffer.clone(),
    };

    let tap = Tap::new(incoming, limit, response_buffer);
    let tap = match draft {
        Some(draft) => {
            let shared = Arc::clone(shared);
            tap.on_end(move || {
                let entry = finish(&shared, &draft, side, None);
                publish(&shared, entry);
            })
        }
        None => tap,
    };

    let mut response = Response::new(tap.map_err(box_error).boxed_unsync());
    *response.status_mut() = response_parts.status;
    *response.headers_mut() = response_parts.headers;
    response
}

/// Upstream failure: answer the client with a local error and record the entry.
fn fail(
    shared: &Arc<Shared>,
    draft: Option<Draft>,
    status: StatusCode,
    message: String,
) -> Response<ProxyBody> {
    if let Some(draft) = draft {
        let side = ResponseSide {
            status: None,
            headers: Vec::new(),
            content_type: None,
            ttfb_ms: None,
            body: None,
        };
        publish(shared, finish(shared, &draft, side, Some(message.clone())));
    }
    local_response(status, &message)
}

fn finish(
    shared: &Shared,
    draft: &Draft,
    side: ResponseSide,
    error: Option<String>,
) -> StoredEntry {
    let request_body = draft.request_body.lock().clone();
    let response_body = side
        .body
        .as_ref()
        .map(|buffer| buffer.lock().clone())
        .unwrap_or_default();
    let summary = ProxyEntrySummary {
        id: draft.id.clone(),
        session_id: shared.record.id.clone(),
        captured_at: draft.captured_at,
        method: draft.method.clone(),
        url: draft.url.clone(),
        scheme: draft.route.scheme.to_string(),
        host: draft.route.host.clone(),
        port: draft.route.port,
        path: draft.path.clone(),
        status_code: side.status.map(|status| status.as_u16()),
        status_text: side
            .status
            .and_then(|status| status.canonical_reason().map(str::to_string)),
        content_type: side.content_type,
        request_body_size: request_body.total,
        response_body_size: response_body.total,
        duration_ms: draft.started.elapsed().as_millis() as u64,
        ttfb_ms: side.ttfb_ms,
        mode: draft.route.mode.to_string(),
        decrypted: draft.route.decrypted,
        error,
        note: None,
    };
    StoredEntry {
        summary,
        request_headers: draft.request_headers.clone(),
        response_headers: side.headers,
        request_body,
        response_body,
    }
}

struct ConnectOutcome {
    mode: &'static str,
    note: Option<String>,
    error: Option<String>,
    sent: u64,
    received: u64,
}

/// Records a CONNECT outcome (tunnel relayed, tunnel failed, or interception failed).
fn publish_connect(
    shared: &Arc<Shared>,
    host: &str,
    port: u16,
    captured_at: i64,
    started: Instant,
    outcome: ConnectOutcome,
) {
    if !shared.recording() {
        return;
    }
    let scheme = if port == 443 { "https" } else { "tcp" };
    let authority = authority_string(host, port, 443);
    let summary = ProxyEntrySummary {
        id: new_id(),
        session_id: shared.record.id.clone(),
        captured_at,
        method: "CONNECT".into(),
        url: format!("{scheme}://{authority}"),
        scheme: scheme.into(),
        host: host.to_string(),
        port,
        path: "/".into(),
        status_code: None,
        status_text: None,
        content_type: None,
        request_body_size: outcome.sent,
        response_body_size: outcome.received,
        duration_ms: started.elapsed().as_millis() as u64,
        ttfb_ms: None,
        mode: outcome.mode.into(),
        decrypted: false,
        error: outcome.error,
        note: outcome.note,
    };
    publish(
        shared,
        StoredEntry {
            summary,
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            request_body: CapturedBody::default(),
            response_body: CapturedBody::default(),
        },
    );
}

async fn connect_upstream(host: &str, port: u16) -> Result<TcpStream, String> {
    match tokio::time::timeout(TUNNEL_CONNECT_TIMEOUT, TcpStream::connect((host, port))).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(error)) => Err(format!("cannot connect to {host}:{port}: {error}")),
        Err(_) => Err(format!("connecting to {host}:{port} timed out")),
    }
}

/// True when the client itself refused our certificate (it sent a fatal TLS alert).
fn is_client_rejection(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|inner| matches!(inner, rustls::Error::AlertReceived(_)))
}

fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

fn authority_string(host: &str, port: u16, default_port: u16) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if port == default_port {
        host
    } else {
        format!("{host}:{port}")
    }
}

fn describe(error: &(dyn StdError + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        message.push_str(": ");
        message.push_str(&next.to_string());
        source = next.source();
    }
    message
}

fn box_error(error: hyper::Error) -> BoxError {
    Box::new(error)
}

fn full_body(bytes: Bytes) -> ProxyBody {
    Full::new(bytes)
        .map_err(|never: Infallible| -> BoxError { match never {} })
        .boxed_unsync()
}

fn local_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    let mut response = Response::new(full_body(Bytes::copy_from_slice(message.as_bytes())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
