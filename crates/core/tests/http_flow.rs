//! End-to-end HTTP execution tests against a local test server.
//!
//! 阶段 1 验收要求用「本地测试服务」验证 GET、JSON POST、文件上传、超时与取消，
//! 这些用例直接驱动 `services::http::execute`（即生产代码路径），服务端是本文件
//! 内的最小 HTTP/1.1 服务器，不依赖外部网络。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use thread_core::error::AppError;
use thread_core::services::http::{
    EffectiveSettings, HttpRequestConfig, HttpTaskRegistry, SendOptions, execute,
};
use thread_core::workspace::formats::{HttpBody, HttpBodyField, KeyValue};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

struct TestServer {
    base_url: String,
    _handle: tokio::task::JoinHandle<()>,
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "thread-http-it-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Minimal HTTP/1.1 server: one request per connection, `Connection: close`.
async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(handle_connection(stream));
        }
    });
    TestServer {
        base_url: format!("http://{address}"),
        _handle: handle,
    }
}

async fn handle_connection(mut stream: TcpStream) {
    // Probes (DNS/TCP timing) connect and disconnect without sending anything.
    let Ok(request) =
        tokio::time::timeout(Duration::from_millis(500), read_request(&mut stream)).await
    else {
        return;
    };
    let Some((request_line, headers, body)) = request else {
        return;
    };

    let target = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    // Route on the path only; the query string is echoed back through /echo bodies.
    let path = target.split('?').next().unwrap_or("/").to_string();
    let content_type = headers
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "text/plain".to_string());

    let (status, extra_headers, payload) = match path.as_str() {
        "/hello" => (
            "200 OK",
            vec![("Content-Type".to_string(), "text/plain".to_string())],
            b"hello world".to_vec(),
        ),
        "/echo" => (
            "200 OK",
            vec![
                ("Content-Type".to_string(), content_type),
                (
                    "X-Echo-Method".to_string(),
                    request_line
                        .split_whitespace()
                        .next()
                        .unwrap_or("GET")
                        .to_string(),
                ),
            ],
            body,
        ),
        "/large" => (
            "200 OK",
            vec![(
                "Content-Type".to_string(),
                "application/octet-stream".to_string(),
            )],
            vec![b'x'; 200 * 1024],
        ),
        "/redirect" => (
            "302 Found",
            vec![("Location".to_string(), "/hello".to_string())],
            Vec::new(),
        ),
        "/slow" => {
            let delay = headers
                .get("x-delay-ms")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_000);
            tokio::time::sleep(Duration::from_millis(delay)).await;
            (
                "200 OK",
                vec![("Content-Type".to_string(), "text/plain".to_string())],
                b"slow response".to_vec(),
            )
        }
        _ => (
            "404 Not Found",
            vec![("Content-Type".to_string(), "text/plain".to_string())],
            b"not found".to_vec(),
        ),
    };

    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        payload.len()
    );
    for (name, value) in extra_headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.write_all(&payload).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

type ParsedRequest = (String, BTreeMap<String, String>, Vec<u8>);

async fn read_request(stream: &mut TcpStream) -> Option<ParsedRequest> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];

    // Read until the end of the header block.
    let header_end = loop {
        if let Some(position) = find_subslice(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next()?.to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    Some((request_line, headers, body))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn options(url: String, bodies_dir: PathBuf) -> SendOptions {
    SendOptions {
        task_id: "task-1".to_string(),
        config: HttpRequestConfig {
            url,
            ..HttpRequestConfig::default()
        },
        variables: BTreeMap::new(),
        collection_auth: None,
        settings: EffectiveSettings {
            timeout_ms: 10_000,
            follow_redirects: true,
            verify_ssl: true,
            max_inline_body_bytes: 64 * 1024,
        },
        bodies_dir,
    }
}

#[tokio::test]
async fn get_returns_status_headers_body_size_and_timing() {
    let server = spawn_server().await;
    let bodies = temp_dir("get");
    let mut send = options(format!("{}/hello", server.base_url), bodies.clone());

    let (response, resolved) = execute(send.clone(), None).await.unwrap();

    assert_eq!(response.status_code, 200);
    assert_eq!(response.status_text, "OK");
    assert_eq!(response.body.as_deref(), Some("hello world"));
    assert_eq!(response.body_size, 11);
    assert_eq!(response.content_type.as_deref(), Some("text/plain"));
    assert_eq!(
        response
            .headers
            .get("content-type")
            .map(|values| values[0].as_str()),
        Some("text/plain")
    );
    assert!(response.timing.total_ms > 0.0);
    assert!(response.timing.ttfb_ms > 0.0);
    assert!(response.timing.dns_ms >= 0.0);
    assert!(
        response.timing.tcp_ms > 0.0,
        "local TCP connect should be measurable"
    );
    assert_eq!(response.redirect_count, 0);
    assert!(response.unresolved_variables.is_empty());
    assert_eq!(resolved.url, format!("{}/hello", server.base_url));

    // Query params are appended, not replacing an existing query string.
    send.config.query_params = vec![KeyValue::new("page", "1"), KeyValue::new("q", "a b")];
    let (response, resolved) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 200);
    assert!(resolved.url.ends_with("/hello?page=1&q=a%20b"));
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn post_json_echoes_body_and_content_type() {
    let server = spawn_server().await;
    let bodies = temp_dir("json");
    let mut send = options(format!("{}/echo", server.base_url), bodies.clone());
    send.config.method = "POST".to_string();
    send.config.body = HttpBody {
        kind: "json".to_string(),
        content: r#"{"name":"张三","email":"john@example.com"}"#.to_string(),
        ..HttpBody::default()
    };

    let (response, resolved) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(
        response.body.as_deref(),
        Some(r#"{"name":"张三","email":"john@example.com"}"#)
    );
    assert_eq!(
        resolved
            .headers
            .iter()
            .find(|header| header.key.eq_ignore_ascii_case("content-type"))
            .map(|header| header.value.as_str()),
        Some("application/json")
    );
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn post_multipart_uploads_a_file() {
    let server = spawn_server().await;
    let bodies = temp_dir("upload");
    let file_path = bodies.join("payload.txt");
    std::fs::write(&file_path, "file-payload-content").unwrap();

    let mut send = options(format!("{}/echo", server.base_url), bodies.clone());
    send.config.method = "POST".to_string();
    send.config.body = HttpBody {
        kind: "multipart".to_string(),
        fields: vec![
            HttpBodyField {
                key: "title".to_string(),
                value: "hello".to_string(),
                kind: "text".to_string(),
                enabled: true,
                file_path: None,
                content_type: None,
            },
            HttpBodyField {
                key: "upload".to_string(),
                value: String::new(),
                kind: "file".to_string(),
                enabled: true,
                file_path: Some(file_path.to_string_lossy().into_owned()),
                content_type: None,
            },
        ],
        ..HttpBody::default()
    };

    let (response, _) = execute(send, None).await.unwrap();
    let echoed = response.body.unwrap();
    assert!(echoed.contains("name=\"title\""));
    assert!(echoed.contains("hello"));
    assert!(echoed.contains("filename=\"payload.txt\""));
    assert!(echoed.contains("file-payload-content"));
    assert!(
        response
            .content_type
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn post_form_urlencoded_encodes_fields() {
    let server = spawn_server().await;
    let bodies = temp_dir("form");
    let mut send = options(format!("{}/echo", server.base_url), bodies.clone());
    send.config.method = "POST".to_string();
    send.config.body = HttpBody {
        kind: "formUrlencoded".to_string(),
        fields: vec![HttpBodyField {
            key: "user name".to_string(),
            value: "张三 & co".to_string(),
            kind: "text".to_string(),
            enabled: true,
            file_path: None,
            content_type: None,
        }],
        ..HttpBody::default()
    };

    let (response, _) = execute(send, None).await.unwrap();
    assert_eq!(
        response.body.as_deref(),
        Some("user%20name=%E5%BC%A0%E4%B8%89%20%26%20co")
    );
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn large_body_is_streamed_to_a_file() {
    let server = spawn_server().await;
    let bodies = temp_dir("large");
    let send = options(format!("{}/large", server.base_url), bodies.clone());

    let (response, _) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 200);
    assert!(response.body.is_none(), "large bodies must not be inlined");
    assert_eq!(response.body_size, 200 * 1024);
    let path = response.body_file_path.expect("body file path");
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        200 * 1024,
        "the full body must be persisted"
    );
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn redirects_are_followed_or_reported() {
    let server = spawn_server().await;
    let bodies = temp_dir("redirect");

    let send = options(format!("{}/redirect", server.base_url), bodies.clone());
    let (response, _) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.redirect_count, 1);
    assert!(response.url.ends_with("/hello"));

    let mut send = options(format!("{}/redirect", server.base_url), bodies.clone());
    send.settings.follow_redirects = false;
    let (response, _) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 302);
    assert_eq!(response.redirect_count, 0);
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn timeout_is_reported_as_a_timeout_error() {
    let server = spawn_server().await;
    let bodies = temp_dir("timeout");
    let mut send = options(format!("{}/slow", server.base_url), bodies.clone());
    send.config.headers = vec![KeyValue::new("X-Delay-Ms", "2000")];
    send.settings.timeout_ms = 250;

    let error = execute(send, None).await.unwrap_err();
    assert_eq!(error.code(), "TIMEOUT");
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn cancel_interrupts_an_in_flight_request() {
    let server = spawn_server().await;
    let bodies = temp_dir("cancel");
    let registry = Arc::new(HttpTaskRegistry::new());
    let receiver = registry.register("task-cancel");

    // Fire the cancellation shortly after the request starts.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(registry.cancel("task-cancel"));
    });

    let mut send = options(format!("{}/slow", server.base_url), bodies.clone());
    send.config.headers = vec![KeyValue::new("X-Delay-Ms", "3000")];
    send.task_id = "task-cancel".to_string();

    let started = std::time::Instant::now();
    let error = execute(send, Some(receiver)).await.unwrap_err();
    assert_eq!(error.code(), "CANCELLED");
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "cancel must not wait for the full response"
    );
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn variables_are_substituted_and_missing_ones_reported() {
    let server = spawn_server().await;
    let bodies = temp_dir("variables");
    let mut send = options(
        "{{baseUrl}}/hello?token={{token}}".to_string(),
        bodies.clone(),
    );
    send.variables
        .insert("baseUrl".to_string(), server.base_url.clone());
    send.config.query_params = vec![KeyValue::new("id", "{{userId}}")];

    let (response, resolved) = execute(send, None).await.unwrap();
    assert_eq!(response.status_code, 200);
    // URL is resolved before query params, so `token` is reported first.
    assert_eq!(
        response.unresolved_variables,
        vec!["token".to_string(), "userId".to_string()]
    );
    // Missing variables stay visible (query values are percent-encoded in the URL).
    assert!(resolved.url.contains("{{token}}"));
    assert!(resolved.url.contains("id=%7B%7BuserId%7D%7D"));
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn network_errors_surface_as_network_error_code() {
    // Nothing is listening on this port.
    let bodies = temp_dir("offline");
    let send = options("http://127.0.0.1:1/hello".to_string(), bodies.clone());
    let error = execute(send, None).await.unwrap_err();
    assert_eq!(error.code(), "NETWORK_ERROR");
    assert!(matches!(error, AppError::Network(_)));
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn unsupported_body_type_is_rejected_before_sending() {
    let server = spawn_server().await;
    let bodies = temp_dir("badbody");
    let mut send = options(format!("{}/echo", server.base_url), bodies.clone());
    send.config.method = "POST".to_string();
    send.config.body = HttpBody {
        kind: "banana".to_string(),
        ..HttpBody::default()
    };
    let error = execute(send, None).await.unwrap_err();
    assert_eq!(error.code(), "VALIDATION");
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn empty_url_is_rejected() {
    let bodies = temp_dir("emptyurl");
    let send = options(String::new(), bodies.clone());
    let error = execute(send, None).await.unwrap_err();
    assert_eq!(error.code(), "VALIDATION");
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn binary_file_body_is_uploaded_with_a_guessed_content_type() {
    let server = spawn_server().await;
    let bodies = temp_dir("binary");
    let file_path = bodies.join("data.json");
    std::fs::write(&file_path, r#"{"ok":true}"#).unwrap();

    let mut send = options(format!("{}/echo", server.base_url), bodies.clone());
    send.config.method = "PUT".to_string();
    send.config.body = HttpBody {
        kind: "binary".to_string(),
        file_path: Some(file_path.to_string_lossy().into_owned()),
        ..HttpBody::default()
    };

    let (response, resolved) = execute(send, None).await.unwrap();
    assert_eq!(response.body.as_deref(), Some(r#"{"ok":true}"#));
    assert_eq!(
        response.headers.get("x-echo-method").map(|v| v[0].as_str()),
        Some("PUT")
    );
    assert_eq!(resolved.body_kind, "binary");
    let _ = std::fs::remove_dir_all(&bodies);
}

#[tokio::test]
async fn a_dropped_cancellation_sender_does_not_cancel_the_request() {
    let server = spawn_server().await;
    let bodies = temp_dir("oneshot");
    let registry = HttpTaskRegistry::new();
    let receiver = registry.register("unused");
    let send = options(format!("{}/hello", server.base_url), bodies.clone());
    let (response, _) = execute(send, Some(receiver)).await.unwrap();
    assert_eq!(response.status_code, 200);

    // Dropping the sender without signalling must not be treated as a cancellation.
    let (sender, receiver) = oneshot::channel::<()>();
    drop(sender);
    let send = options(format!("{}/hello", server.base_url), bodies.clone());
    let (response, _) = execute(send, Some(receiver)).await.unwrap();
    assert_eq!(response.status_code, 200);
    let _ = std::fs::remove_dir_all(&bodies);
}
