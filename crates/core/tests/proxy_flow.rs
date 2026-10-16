//! End-to-end tests for the capture proxy: plain HTTP forwarding, CONNECT relaying,
//! pause/resume and the default "no HTTPS decryption" behaviour.
//!
//! Upstream servers are local and minimal; nothing here touches external networks or
//! installs a certificate.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thread_core::db::DbState;
use thread_core::error::AppError;
use thread_core::services::proxy::settings::CaptureSettings;
use thread_core::services::proxy::{ProxyEngine, ProxyEntrySummary, ProxyEventSink, ProxyStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
struct Recorder {
    entries: Mutex<Vec<ProxyEntrySummary>>,
    statuses: Mutex<Vec<ProxyStatus>>,
}

impl ProxyEventSink for Recorder {
    fn entry(&self, entry: &ProxyEntrySummary) {
        self.entries.lock().unwrap().push(entry.clone());
    }

    fn status(&self, status: &ProxyStatus) {
        self.statuses.lock().unwrap().push(status.clone());
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "thread-proxy-it-{tag}-{}-{}",
        std::process::id(),
        thread_core::db::new_id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_engine(tag: &str) -> (ProxyEngine, Arc<Recorder>, PathBuf) {
    let dir = temp_dir(tag);
    let db = Arc::new(DbState::open(dir.join("thread.db")).unwrap());
    let recorder = Arc::new(Recorder::default());
    let engine = ProxyEngine::new(dir.join("ca"), db, recorder.clone());
    (engine, recorder, dir)
}

/// One-shot HTTP/1.1 server: answers every connection with `hello`.
async fn spawn_http_upstream() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                read_head(&mut stream).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                    )
                    .await;
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

/// Echo server used as the far end of CONNECT tunnels.
async fn spawn_echo_upstream() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.into_split();
                let _ = tokio::io::copy(&mut reader, &mut writer).await;
            });
        }
    });
    port
}

async fn read_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        let read = stream.read(&mut byte).await.unwrap();
        if read == 0 {
            break;
        }
        buf.push(byte[0]);
    }
    buf
}

async fn send_raw(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut out))
        .await
        .expect("proxy response timed out")
        .unwrap();
    out
}

/// Entries are published when a response body finishes, which can be just after the
/// client sees EOF. Poll briefly instead of asserting immediately.
async fn wait_until(what: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn http_request_is_forwarded_and_summarized() {
    let upstream = spawn_http_upstream().await;
    let (engine, recorder, _dir) = make_engine("http");

    let status = engine
        .start(0, None, CaptureSettings::default())
        .await
        .expect("proxy starts");
    assert!(status.running);
    let proxy_port = status.port.expect("bound port");

    let request = format!(
        "GET http://127.0.0.1:{upstream}/hello?token=abc HTTP/1.1\r\nHost: 127.0.0.1:{upstream}\r\nAuthorization: Bearer secret\r\nConnection: close\r\n\r\n"
    );
    let response = send_raw(proxy_port, request.as_bytes()).await;
    let text = String::from_utf8_lossy(&response);
    assert!(text.starts_with("HTTP/1.1 200 OK"), "got: {text}");
    assert!(text.ends_with("hello"), "body must stream through: {text}");

    wait_until("the HTTP entry", || {
        !engine.entries(None).unwrap().is_empty()
    })
    .await;
    let entries = engine.entries(None).unwrap();
    let entry = &entries[0];
    assert_eq!(entry.method, "GET");
    assert_eq!(entry.mode, "http");
    assert!(!entry.decrypted);
    assert_eq!(entry.status_code, Some(200));
    assert_eq!(entry.path, "/hello");
    assert_eq!(entry.response_body_size, 5);
    assert!(
        entry.url.contains("token=***"),
        "query secret masked: {}",
        entry.url
    );
    assert!(!entry.url.contains("abc"));

    let detail = engine.entry(&entry.id).unwrap().expect("detail available");
    assert_eq!(detail.response_body.content, "hello");
    assert!(
        detail
            .request_headers
            .iter()
            .any(|header| header.key == "authorization" && header.value == "***"),
        "authorization header must be masked"
    );

    assert_eq!(
        recorder.entries.lock().unwrap().len(),
        1,
        "UI sink notified once"
    );

    let stopped = engine.stop().await;
    assert!(!stopped.running);
}

#[tokio::test]
async fn paused_capture_forwards_without_recording() {
    let upstream = spawn_http_upstream().await;
    let (engine, _recorder, _dir) = make_engine("paused");
    let proxy_port = engine
        .start(0, None, CaptureSettings::default())
        .await
        .unwrap()
        .port
        .unwrap();

    let paused = engine.set_paused(true).await.unwrap();
    assert!(paused.paused);

    let request = format!(
        "GET http://127.0.0.1:{upstream}/quiet HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    );
    let response = send_raw(proxy_port, request.as_bytes()).await;
    assert!(String::from_utf8_lossy(&response).ends_with("hello"));

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        engine.entries(None).unwrap().is_empty(),
        "nothing recorded while paused"
    );

    engine.set_paused(false).await.unwrap();
    let response = send_raw(proxy_port, request.as_bytes()).await;
    assert!(String::from_utf8_lossy(&response).ends_with("hello"));
    wait_until("entry after resume", || {
        !engine.entries(None).unwrap().is_empty()
    })
    .await;

    engine.stop().await;
}

#[tokio::test]
async fn connect_tunnel_relays_bytes_without_decrypting() {
    let echo = spawn_echo_upstream().await;
    let (engine, _recorder, _dir) = make_engine("tunnel");
    let proxy_port = engine
        .start(0, None, CaptureSettings::default())
        .await
        .unwrap()
        .port
        .unwrap();

    let mut stream = TcpStream::connect(("127.0.0.1", proxy_port)).await.unwrap();
    stream
        .write_all(
            format!("CONNECT 127.0.0.1:{echo} HTTP/1.1\r\nHost: 127.0.0.1:{echo}\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let head = String::from_utf8_lossy(&read_head(&mut stream).await).to_string();
    assert!(head.starts_with("HTTP/1.1 200"), "tunnel accepted: {head}");

    stream.write_all(b"ping").await.unwrap();
    let mut echoed = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut echoed))
        .await
        .expect("echo timed out")
        .unwrap();
    assert_eq!(&echoed, b"ping");
    drop(stream);

    wait_until("the tunnel entry", || {
        !engine.entries(None).unwrap().is_empty()
    })
    .await;
    let entries = engine.entries(None).unwrap();
    let entry = &entries[0];
    assert_eq!(entry.method, "CONNECT");
    assert_eq!(entry.mode, "tunnel");
    assert!(!entry.decrypted);
    assert_eq!(entry.request_body_size, 4);
    assert_eq!(entry.response_body_size, 4);
    assert!(
        entry.note.is_some(),
        "tunnel explains why it was not inspected"
    );

    engine.stop().await;
}

#[tokio::test]
async fn origin_form_request_is_rejected_with_guidance() {
    let (engine, _recorder, _dir) = make_engine("origin");
    let proxy_port = engine
        .start(0, None, CaptureSettings::default())
        .await
        .unwrap()
        .port
        .unwrap();

    let response = send_raw(
        proxy_port,
        b"GET /hello HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(text.starts_with("HTTP/1.1 400"), "got: {text}");

    engine.stop().await;
}

#[tokio::test]
async fn second_start_is_refused_while_running() {
    let (engine, _recorder, _dir) = make_engine("double");
    engine
        .start(0, None, CaptureSettings::default())
        .await
        .unwrap();

    let error = engine
        .start(0, None, CaptureSettings::default())
        .await
        .expect_err("second start must fail");
    assert!(matches!(error, AppError::Proxy(_)));

    engine.stop().await;
    let status = engine.status().await;
    assert!(!status.running);
}

#[tokio::test]
async fn stop_restores_idle_state_and_is_idempotent() {
    let (engine, _recorder, _dir) = make_engine("stop");
    let port = engine
        .start(0, None, CaptureSettings::default())
        .await
        .unwrap()
        .port
        .unwrap();

    engine.stop().await;
    engine.stop().await;

    assert!(
        TcpStream::connect(("127.0.0.1", port)).await.is_err(),
        "listener released"
    );
}
