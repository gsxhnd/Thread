//! HTTP/HTTPS request execution (Rust side — no CORS, full TLS control).
//!
//! 覆盖阶段 1 验收项：方法、URL、Query、Headers、JSON/Raw/Form/文件 Body、
//! 超时、取消、重定向、证书校验、响应状态/Headers/Body/大小/耗时。
//!
//! 计时说明：`dnsMs` / `tcpMs` 来自发送前的轻量探测（解析 + TCP 连接，3 秒上限，
//! 失败记为 0）；`tlsMs` 在 MVP 中不单独测量（真实握手包含在 `ttfbMs` 内），
//! 精确的分阶段 TLS 计时随阶段 3 的代理引擎一起接入。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::config::AppSettings;
use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::workspace::formats::{AuthSpec, HttpBody, KeyValue, RequestSettings};

use super::variables::substitute;

const MAX_REDIRECTS: usize = 10;
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequestConfig {
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub query_params: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: HttpBody,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub settings: RequestSettings,
}

fn default_method() -> String {
    "GET".to_string()
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            method: default_method(),
            url: String::new(),
            query_params: Vec::new(),
            headers: Vec::new(),
            body: HttpBody::default(),
            auth: AuthSpec::default(),
            settings: RequestSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub dns_ms: f64,
    pub tcp_ms: f64,
    pub tls_ms: f64,
    pub ttfb_ms: f64,
    pub download_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub task_id: String,
    pub url: String,
    pub status_code: u16,
    pub status_text: String,
    pub headers: BTreeMap<String, Vec<String>>,
    /// `None` when the body exceeded the inline limit (see `bodyFilePath`).
    pub body: Option<String>,
    pub body_file_path: Option<String>,
    pub body_size: u64,
    pub content_type: Option<String>,
    pub timing: Timing,
    pub redirect_count: usize,
    pub unresolved_variables: Vec<String>,
}

/// The request as actually sent (variables substituted) — stored in history.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub body_preview: Option<String>,
    pub body_kind: String,
}

/// Settings already merged from request → collection → app defaults.
#[derive(Debug, Clone, Copy)]
pub struct EffectiveSettings {
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub verify_ssl: bool,
    pub max_inline_body_bytes: u64,
}

impl EffectiveSettings {
    pub fn resolve(
        request: &RequestSettings,
        collection: Option<&RequestSettings>,
        app: &AppSettings,
    ) -> Self {
        Self {
            timeout_ms: request
                .timeout_ms
                .or(collection.and_then(|settings| settings.timeout_ms))
                .unwrap_or(app.default_timeout_ms)
                .max(1),
            follow_redirects: request
                .follow_redirects
                .or(collection.and_then(|settings| settings.follow_redirects))
                .unwrap_or(app.follow_redirects),
            verify_ssl: request
                .verify_ssl
                .or(collection.and_then(|settings| settings.verify_ssl))
                .unwrap_or(app.verify_ssl),
            max_inline_body_bytes: app.max_inline_body_bytes,
        }
    }
}

#[derive(Clone)]
pub struct SendOptions {
    pub task_id: String,
    pub config: HttpRequestConfig,
    pub variables: BTreeMap<String, String>,
    pub collection_auth: Option<AuthSpec>,
    pub settings: EffectiveSettings,
    pub bodies_dir: PathBuf,
}

/// Registry of in-flight requests, used by `http_cancel`.
#[derive(Default)]
pub struct HttpTaskRegistry {
    tasks: parking_lot::Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl HttpTaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, task_id: &str) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        self.tasks.lock().insert(task_id.to_string(), sender);
        receiver
    }

    pub fn unregister(&self, task_id: &str) {
        self.tasks.lock().remove(task_id);
    }

    /// Returns true when a live request was found and signalled.
    pub fn cancel(&self, task_id: &str) -> bool {
        match self.tasks.lock().remove(task_id) {
            Some(sender) => sender.send(()).is_ok(),
            None => false,
        }
    }

    pub fn active(&self) -> Vec<String> {
        self.tasks.lock().keys().cloned().collect()
    }
}

/// Executes the request. `cancel` aborts the whole flow (including body download).
pub async fn execute(
    options: SendOptions,
    cancel: Option<oneshot::Receiver<()>>,
) -> AppResult<(HttpResponse, ResolvedRequest)> {
    let sender = options.config.clone();
    let variables = options.variables.clone();
    let settings = options.settings;
    let collection_auth = options.collection_auth.clone();
    let task_id = options.task_id.clone();
    let bodies_dir = options.bodies_dir.clone();

    let mut work = Box::pin(run(
        sender,
        variables,
        collection_auth,
        settings,
        task_id,
        bodies_dir,
    ));

    let Some(receiver) = cancel else {
        return work.await;
    };
    let mut receiver = Box::pin(receiver);

    tokio::select! {
        biased;
        message = &mut receiver => {
            if message.is_ok() {
                Err(AppError::cancelled())
            } else {
                // The sender was dropped: nobody can cancel this request anymore,
                // so wait for it to finish instead of reporting a bogus cancellation.
                work.await
            }
        }
        result = &mut work => result,
    }
}

async fn run(
    config: HttpRequestConfig,
    variables: BTreeMap<String, String>,
    collection_auth: Option<AuthSpec>,
    settings: EffectiveSettings,
    task_id: String,
    bodies_dir: PathBuf,
) -> AppResult<(HttpResponse, ResolvedRequest)> {
    let started = Instant::now();
    let mut missing: Vec<String> = Vec::new();

    crate::db::require_non_empty("url", &config.url)?;

    let raw_url = ensure_absolute_url(&substitute(&config.url, &variables, &mut missing));
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    for param in config.query_params.iter().filter(|param| param.enabled) {
        if param.key.trim().is_empty() {
            continue;
        }
        query_pairs.push((
            substitute(&param.key, &variables, &mut missing),
            substitute(&param.value, &variables, &mut missing),
        ));
    }

    let mut headers: Vec<KeyValue> = Vec::new();
    for header in config.headers.iter().filter(|header| header.enabled) {
        if header.key.trim().is_empty() {
            continue;
        }
        headers.push(KeyValue {
            key: substitute(&header.key, &variables, &mut missing),
            value: substitute(&header.value, &variables, &mut missing),
            enabled: true,
            description: header.description.clone(),
        });
    }

    // Auth: request-level configuration wins, otherwise the collection auth applies.
    let auth = AuthSpec::resolve(&config.auth, collection_auth.as_ref());
    let mut auth_query: Vec<(String, String)> = Vec::new();
    apply_auth(
        &auth,
        &variables,
        &mut missing,
        &mut headers,
        &mut auth_query,
    )?;
    query_pairs.extend(auth_query);

    let final_url = append_query_params(&raw_url, &query_pairs)?;

    let (body_bytes, body_content_type, body_preview) =
        build_body(&config.body, &variables, &mut missing)?;
    if let Some(content_type) = body_content_type
        && !headers
            .iter()
            .any(|header| header.key.eq_ignore_ascii_case("content-type"))
    {
        headers.push(KeyValue::new("Content-Type", &content_type));
    }

    let resolved = ResolvedRequest {
        method: config.method.to_uppercase(),
        url: final_url.clone(),
        headers: headers.clone(),
        body_preview,
        body_kind: config.body.kind.clone(),
    };

    // Preflight timing probe (best effort, never fails the request).
    let (dns_ms, tcp_ms) = probe_timings(&final_url).await;

    let redirects = Arc::new(AtomicUsize::new(0));
    let mut builder = Client::builder()
        .timeout(Duration::from_millis(settings.timeout_ms))
        .connect_timeout(Duration::from_millis(settings.timeout_ms.min(30_000)))
        .danger_accept_invalid_certs(!settings.verify_ssl)
        .user_agent(concat!("Thread/", env!("CARGO_PKG_VERSION")));

    if settings.follow_redirects {
        let counter = redirects.clone();
        builder = builder.redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.stop()
            } else {
                counter.fetch_add(1, Ordering::SeqCst);
                attempt.follow()
            }
        }));
    } else {
        builder = builder.redirect(Policy::none());
    }

    let client = builder
        .build()
        .map_err(|error| AppError::Network(error.to_string()))?;

    let method = Method::from_bytes(config.method.to_uppercase().as_bytes())
        .map_err(|_| AppError::Validation(format!("invalid HTTP method: {}", config.method)))?;

    let mut request = client.request(method, &final_url);
    request = request.header(reqwest::header::ACCEPT, HeaderValue::from_static("*/*"));
    for header in &headers {
        let name = HeaderName::from_bytes(header.key.as_bytes())
            .map_err(|_| AppError::Validation(format!("invalid header name: {}", header.key)))?;
        let value = HeaderValue::from_str(&header.value).map_err(|_| {
            AppError::Validation(format!("invalid header value for {}", header.key))
        })?;
        request = request.header(name, value);
    }
    if let Some(bytes) = body_bytes {
        request = request.body(bytes);
    }

    let send_started = Instant::now();
    let response = request.send().await.map_err(AppError::from)?;
    let ttfb = send_started.elapsed();

    let status = response.status();
    let response_url = response.url().to_string();
    let header_map: HeaderMap = response.headers().clone();
    let headers_out = collect_headers(&header_map);
    let content_type = header_map
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let (body, body_file_path, body_size) =
        read_body(response, settings.max_inline_body_bytes, &bodies_dir).await?;
    let total = started.elapsed();

    let response = HttpResponse {
        task_id,
        url: response_url,
        status_code: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        headers: headers_out,
        body,
        body_file_path,
        body_size,
        content_type,
        timing: Timing {
            dns_ms: round_ms(dns_ms),
            tcp_ms: round_ms(tcp_ms),
            tls_ms: 0.0,
            ttfb_ms: round_ms(ttfb.as_secs_f64() * 1000.0),
            download_ms: round_ms((send_started.elapsed() - ttfb).as_secs_f64() * 1000.0),
            total_ms: round_ms(total.as_secs_f64() * 1000.0),
        },
        redirect_count: redirects.load(Ordering::SeqCst),
        unresolved_variables: missing,
    };

    Ok((response, resolved))
}

fn round_ms(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn collect_headers(headers: &HeaderMap) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_lowercase();
        let text = value
            .to_str()
            .map(|value| value.to_string())
            .unwrap_or_else(|_| String::from_utf8_lossy(value.as_bytes()).into_owned());
        out.entry(key).or_default().push(text);
    }
    out
}

async fn read_body(
    response: reqwest::Response,
    max_inline: u64,
    bodies_dir: &Path,
) -> AppResult<(Option<String>, Option<String>, u64)> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    let mut file: Option<(PathBuf, fs::File)> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(AppError::from)?;
        total += chunk.len() as u64;
        if total > MAX_DOWNLOAD_BYTES {
            return Err(AppError::Network(format!(
                "response larger than {} bytes was truncated",
                MAX_DOWNLOAD_BYTES
            )));
        }
        if let Some((_, handle)) = file.as_mut() {
            handle.write_all(&chunk)?;
            continue;
        }
        buffer.extend_from_slice(&chunk);
        if buffer.len() as u64 > max_inline {
            fs::create_dir_all(bodies_dir)?;
            let path = bodies_dir.join(format!("{}.bin", new_id()));
            let mut handle = fs::File::create(&path)?;
            handle.write_all(&buffer)?;
            buffer.clear();
            file = Some((path, handle));
        }
    }

    if let Some((path, mut handle)) = file {
        handle.flush()?;
        return Ok((None, Some(path.to_string_lossy().into_owned()), total));
    }

    let text = String::from_utf8_lossy(&buffer).into_owned();
    Ok((Some(text), None, total))
}

/// `(body bytes, implied content type, preview text)`.
type BuiltBody = (Option<Vec<u8>>, Option<String>, Option<String>);

/// Builds the request body bytes plus the content type it implies.
fn build_body(
    body: &HttpBody,
    variables: &BTreeMap<String, String>,
    missing: &mut Vec<String>,
) -> AppResult<BuiltBody> {
    let kind = body.kind.as_str();
    match kind {
        "" | "none" => Ok((None, None, None)),
        "json" => {
            let content = substitute(&body.content, variables, missing);
            Ok((
                Some(content.clone().into_bytes()),
                Some("application/json".to_string()),
                Some(content),
            ))
        }
        "raw" => {
            let content = substitute(&body.content, variables, missing);
            let content_type = body
                .content_type
                .clone()
                .unwrap_or_else(|| "text/plain; charset=utf-8".to_string());
            Ok((
                Some(content.clone().into_bytes()),
                Some(content_type),
                Some(content),
            ))
        }
        "formUrlencoded" | "form" => {
            let mut pairs: Vec<(String, String)> = Vec::new();
            for field in body
                .fields
                .iter()
                .filter(|field| field.enabled && field.kind != "file")
            {
                if field.key.trim().is_empty() {
                    continue;
                }
                pairs.push((
                    substitute(&field.key, variables, missing),
                    substitute(&field.value, variables, missing),
                ));
            }
            let encoded = pairs
                .iter()
                .map(|(key, value)| {
                    format!("{}={}", encode_component(key), encode_component(value))
                })
                .collect::<Vec<_>>()
                .join("&");
            Ok((
                Some(encoded.clone().into_bytes()),
                Some("application/x-www-form-urlencoded".to_string()),
                Some(encoded),
            ))
        }
        "multipart" | "formData" => {
            let (bytes, content_type) = build_multipart(body, variables, missing)?;
            let preview = Some(format!(
                "<multipart body: {} fields, {} bytes>",
                body.fields.iter().filter(|field| field.enabled).count(),
                bytes.len()
            ));
            Ok((Some(bytes), Some(content_type), preview))
        }
        "binary" | "file" => {
            let raw_path = body.file_path.clone().unwrap_or_default();
            let path = substitute(&raw_path, variables, missing);
            if path.trim().is_empty() {
                return Err(AppError::Validation(
                    "binary body requires a file path".into(),
                ));
            }
            let bytes =
                fs::read(&path).map_err(|error| AppError::Io(format!("{path}: {error}")))?;
            let content_type = body
                .content_type
                .clone()
                .or_else(|| guess_content_type(&path))
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let preview = Some(format!("<file body: {path}, {} bytes>", bytes.len()));
            Ok((Some(bytes), Some(content_type), preview))
        }
        other => Err(AppError::Validation(format!(
            "unsupported body type: {other}"
        ))),
    }
}

fn build_multipart(
    body: &HttpBody,
    variables: &BTreeMap<String, String>,
    missing: &mut Vec<String>,
) -> AppResult<(Vec<u8>, String)> {
    let boundary = format!("----ThreadBoundary{}", new_id().replace('-', ""));
    let mut out: Vec<u8> = Vec::new();

    for field in body.fields.iter().filter(|field| field.enabled) {
        let key = substitute(&field.key, variables, missing);
        if key.trim().is_empty() {
            continue;
        }
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if field.kind == "file" {
            let raw_path = field.file_path.clone().unwrap_or_default();
            let path = substitute(&raw_path, variables, missing);
            if path.trim().is_empty() {
                return Err(AppError::Validation(format!(
                    "multipart field `{key}` requires a file path"
                )));
            }
            let bytes =
                fs::read(&path).map_err(|error| AppError::Io(format!("{path}: {error}")))?;
            let file_name = Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            let content_type = field
                .content_type
                .clone()
                .or_else(|| guess_content_type(&path))
                .unwrap_or_else(|| "application/octet-stream".to_string());
            out.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    escape_multipart(&key),
                    escape_multipart(&file_name)
                )
                .as_bytes(),
            );
            out.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
            out.extend_from_slice(&bytes);
            out.extend_from_slice(b"\r\n");
        } else {
            let value = substitute(&field.value, variables, missing);
            out.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                    escape_multipart(&key)
                )
                .as_bytes(),
            );
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
    }

    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((out, format!("multipart/form-data; boundary={boundary}")))
}

fn escape_multipart(value: &str) -> String {
    value.replace('"', "%22").replace(['\r', '\n'], " ")
}

fn guess_content_type(path: &str) -> Option<String> {
    let extension = Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())?;
    let guessed = match extension.as_str() {
        "json" => "application/json",
        "txt" | "md" => "text/plain",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "yaml" | "yml" => "application/yaml",
        "bin" => "application/octet-stream",
        _ => return None,
    };
    Some(guessed.to_string())
}

fn apply_auth(
    auth: &AuthSpec,
    variables: &BTreeMap<String, String>,
    missing: &mut Vec<String>,
    headers: &mut Vec<KeyValue>,
    query: &mut Vec<(String, String)>,
) -> AppResult<()> {
    let mut resolved = |value: &Option<String>| -> String {
        substitute(
            value.clone().unwrap_or_default().as_str(),
            variables,
            missing,
        )
    };

    match auth.kind.as_str() {
        "bearer" | "oauth2" => {
            let token = resolved(&auth.token);
            if !token.is_empty() {
                headers.retain(|header| !header.key.eq_ignore_ascii_case("authorization"));
                headers.push(KeyValue::new("Authorization", &format!("Bearer {token}")));
            }
        }
        "basic" => {
            let username = resolved(&auth.username);
            let password = resolved(&auth.password);
            headers.retain(|header| !header.key.eq_ignore_ascii_case("authorization"));
            headers.push(KeyValue::new(
                "Authorization",
                &format!(
                    "Basic {}",
                    base64_encode(format!("{username}:{password}").as_bytes())
                ),
            ));
        }
        "apiKey" | "apikey" => {
            let key = resolved(&auth.key);
            let value = resolved(&auth.value);
            if key.is_empty() {
                return Err(AppError::Validation(
                    "API key auth requires a header/query name".into(),
                ));
            }
            if auth.location.as_deref() == Some("query") {
                query.push((key, value));
            } else {
                headers.retain(|header| !header.key.eq_ignore_ascii_case(&key));
                headers.push(KeyValue::new(&key, &value));
            }
        }
        "none" | "inherit" | "" => {}
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auth type: {other}"
            )));
        }
    }
    Ok(())
}

/// Adds `http://` when the substituted URL has no scheme.
///
/// `localhost:3000/users` and `example.com/api` are otherwise rejected by
/// reqwest as a relative URL without a base. Values that already contain
/// `://`, or that still contain an unresolved `{{variable}}`, are left alone.
fn ensure_absolute_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains("://") || trimmed.contains("{{") {
        return trimmed.to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("//") {
        return format!("http://{rest}");
    }
    format!("http://{trimmed}")
}

/// Appends query params, preserving any query already present in the URL.
fn append_query_params(url: &str, pairs: &[(String, String)]) -> AppResult<String> {
    if pairs.is_empty() {
        return Ok(url.to_string());
    }
    let (base, fragment) = match url.split_once('#') {
        Some((base, fragment)) => (base.to_string(), Some(fragment.to_string())),
        None => (url.to_string(), None),
    };
    let mut out = base;
    let separator = if out.contains('?') { '&' } else { '?' };
    let encoded = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    out.push(separator);
    out.push_str(&encoded);
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&fragment);
    }
    Ok(out)
}

/// Percent-encodes a query/form component (RFC 3986 unreserved set preserved).
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn parse_host_port(url: &str) -> Option<(String, u16, bool)> {
    let (scheme, rest) = url.split_once("://")?;
    let secure = scheme.eq_ignore_ascii_case("https");
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return None;
    }
    if let Some(stripped) = authority.strip_prefix('[') {
        let (host, remainder) = stripped.split_once(']')?;
        let port = remainder
            .strip_prefix(':')
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(if secure { 443 } else { 80 });
        return Some((host.to_string(), port, secure));
    }
    match authority.split_once(':') {
        Some((host, port)) => Some((host.to_string(), port.parse::<u16>().ok()?, secure)),
        None => Some((authority.to_string(), if secure { 443 } else { 80 }, secure)),
    }
}

/// Best-effort DNS/TCP timing probe. Never fails the request itself.
async fn probe_timings(url: &str) -> (f64, f64) {
    let Some((host, port, _)) = parse_host_port(url) else {
        return (0.0, 0.0);
    };
    if host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok() {
        // No DNS involved; still measure TCP connect.
    }

    let probe = async {
        let dns_started = Instant::now();
        let resolved = tokio::net::lookup_host((host.as_str(), port)).await.ok();
        let dns_ms = dns_started.elapsed().as_secs_f64() * 1000.0;
        let is_ip = host.parse::<std::net::IpAddr>().is_ok();
        let dns_ms = if is_ip { 0.0 } else { dns_ms };

        let Some(address) = resolved.and_then(|mut addresses| addresses.next()) else {
            return (dns_ms, 0.0);
        };
        let tcp_started = Instant::now();
        let tcp_ms = match tokio::net::TcpStream::connect(address).await {
            Ok(stream) => {
                let elapsed = tcp_started.elapsed().as_secs_f64() * 1000.0;
                drop(stream);
                elapsed
            }
            Err(_) => 0.0,
        };
        (dns_ms, tcp_ms)
    };

    tokio::time::timeout(PROBE_TIMEOUT, probe)
        .await
        .unwrap_or((0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::formats::HttpBodyField;

    fn temp_bodies(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("thread-http-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn encodes_query_components() {
        assert_eq!(encode_component("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode_component("张三"), "%E5%BC%A0%E4%B8%89");
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn appends_query_to_url_with_existing_params_and_fragment() {
        let pairs = vec![("q".to_string(), "a b".to_string())];
        assert_eq!(
            append_query_params("http://x/api?t=1#frag", &pairs).unwrap(),
            "http://x/api?t=1&q=a%20b#frag"
        );
        assert_eq!(
            append_query_params("http://x/api", &pairs).unwrap(),
            "http://x/api?q=a%20b"
        );
    }

    #[test]
    fn parses_host_port_with_defaults_and_ipv6() {
        assert_eq!(
            parse_host_port("https://api.example.com/v1"),
            Some(("api.example.com".into(), 443, true))
        );
        assert_eq!(
            parse_host_port("http://localhost:8080/x"),
            Some(("localhost".into(), 8080, false))
        );
        assert_eq!(
            parse_host_port("https://user:pass@host:9443/x"),
            Some(("host".into(), 9443, true))
        );
        assert_eq!(
            parse_host_port("http://[::1]:9000/x"),
            Some(("::1".into(), 9000, false))
        );
    }

    #[test]
    fn multipart_body_contains_fields_and_file() {
        let dir = temp_bodies("multipart");
        let file_path = dir.join("upload.txt");
        fs::write(&file_path, "file-content").unwrap();

        let body = HttpBody {
            kind: "multipart".into(),
            content: String::new(),
            content_type: None,
            fields: vec![
                HttpBodyField {
                    key: "title".into(),
                    value: "hello".into(),
                    kind: "text".into(),
                    enabled: true,
                    file_path: None,
                    content_type: None,
                },
                HttpBodyField {
                    key: "doc".into(),
                    value: String::new(),
                    kind: "file".into(),
                    enabled: true,
                    file_path: Some(file_path.to_string_lossy().into_owned()),
                    content_type: None,
                },
            ],
            file_path: None,
        };

        let mut missing = Vec::new();
        let (bytes, content_type) = build_multipart(&body, &BTreeMap::new(), &mut missing).unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        assert!(text.contains("name=\"title\"\r\n\r\nhello\r\n"));
        assert!(text.contains("filename=\"upload.txt\""));
        assert!(text.contains("Content-Type: text/plain"));
        assert!(text.contains("file-content"));
        assert!(text.trim_end().ends_with("--"));
        assert!(missing.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_settings_merge_precedence() {
        let app = AppSettings::default();
        let collection = RequestSettings {
            timeout_ms: Some(5_000),
            follow_redirects: Some(false),
            verify_ssl: None,
        };
        let request = RequestSettings {
            timeout_ms: Some(1_000),
            follow_redirects: None,
            verify_ssl: None,
        };
        let merged = EffectiveSettings::resolve(&request, Some(&collection), &app);
        assert_eq!(merged.timeout_ms, 1_000);
        assert!(!merged.follow_redirects);
        assert!(merged.verify_ssl);
    }

    #[test]
    fn cancel_registry_signals_and_cleans_up() {
        let registry = HttpTaskRegistry::new();
        let mut receiver = registry.register("task-9");
        assert_eq!(registry.active(), vec!["task-9".to_string()]);
        assert!(registry.cancel("task-9"));
        assert!(receiver.try_recv().is_ok());
        assert!(registry.active().is_empty());
        assert!(!registry.cancel("task-9"));
    }

    #[test]
    fn ensure_absolute_url_adds_http_when_the_scheme_is_missing() {
        assert_eq!(
            ensure_absolute_url("localhost:3000/users"),
            "http://localhost:3000/users"
        );
        assert_eq!(
            ensure_absolute_url("  example.com/api "),
            "http://example.com/api"
        );
        assert_eq!(
            ensure_absolute_url("//cdn.example.com/a"),
            "http://cdn.example.com/a"
        );
        assert_eq!(
            ensure_absolute_url("https://example.com"),
            "https://example.com"
        );
        assert_eq!(
            ensure_absolute_url("{{baseUrl}}/users"),
            "{{baseUrl}}/users"
        );
    }
}
