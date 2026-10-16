//! Traffic capture helpers: bounded body buffers, redaction and hop-by-hop headers.
//!
//! `Tap` wraps a hyper body and copies frames into a bounded buffer as they flow
//! through, so the proxy streams data to the client without holding whole bodies.
//! When the body ends (or is dropped on abort), the `on_end` callback records the entry.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::HeaderMap;
use http::header::CONNECTION;
use http_body::{Body, Frame, SizeHint};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// One header line as displayed to the user. Duplicate names are kept in order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderPair {
    pub key: String,
    pub value: String,
}

pub const MASKED_VALUE: &str = "***";

/// Bytes observed for one body. `total` counts everything, `bytes` keeps at most the limit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturedBody {
    pub bytes: Vec<u8>,
    pub total: u64,
}

impl CapturedBody {
    pub fn record(&mut self, chunk: &[u8], limit: usize) {
        self.total += chunk.len() as u64;
        let room = limit.saturating_sub(self.bytes.len());
        let keep = room.min(chunk.len());
        self.bytes.extend_from_slice(&chunk[..keep]);
    }

    pub fn is_truncated(&self) -> bool {
        self.total > self.bytes.len() as u64
    }
}

/// Body representation sent to the UI. Valid UTF-8 is shown as text, anything else as base64.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BodyView {
    pub encoding: String,
    pub content: String,
    pub truncated: bool,
    pub total_bytes: u64,
}

pub fn body_view(body: &CapturedBody) -> BodyView {
    let (encoding, content) = match std::str::from_utf8(&body.bytes) {
        Ok(text) => ("text", text.to_string()),
        Err(_) => (
            "base64",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &body.bytes),
        ),
    };
    BodyView {
        encoding: encoding.to_string(),
        content,
        truncated: body.is_truncated(),
        total_bytes: body.total,
    }
}

const SENSITIVE_HEADERS: [&str; 6] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-auth-token",
    "x-access-token",
];

const SENSITIVE_PARAMS: [&str; 8] = [
    "token",
    "access_token",
    "refresh_token",
    "password",
    "passwd",
    "secret",
    "api_key",
    "apikey",
];

pub fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_HEADERS.contains(&lower.as_str())
        || lower.contains("api-key")
        || lower.contains("apikey")
}

pub fn is_sensitive_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_PARAMS.contains(&lower.as_str())
}

pub fn redact_headers(headers: &HeaderMap, mask: bool) -> Vec<HeaderPair> {
    headers
        .iter()
        .map(|(name, value)| HeaderPair {
            key: name.as_str().to_string(),
            value: if mask && is_sensitive_header(name.as_str()) {
                MASKED_VALUE.to_string()
            } else {
                String::from_utf8_lossy(value.as_bytes()).into_owned()
            },
        })
        .collect()
}

/// Masks sensitive query parameter values. Returns the input unchanged when nothing matches.
pub fn redact_url(raw: &str, mask: bool) -> String {
    if !mask {
        return raw.to_string();
    }
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    if url.query().is_none() || !url.query_pairs().any(|(key, _)| is_sensitive_param(&key)) {
        return raw.to_string();
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if is_sensitive_param(&key) {
                MASKED_VALUE.to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    url.query_pairs_mut().clear().extend_pairs(pairs);
    url.to_string()
}

const HOP_BY_HOP: [&str; 8] = [
    "keep-alive",
    "proxy-connection",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Removes hop-by-hop headers (RFC 7230 §6.1), including any named by `Connection`.
pub fn strip_hop_by_hop(headers: &mut HeaderMap) {
    let listed: Vec<String> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    headers.remove(CONNECTION);
    for name in HOP_BY_HOP {
        headers.remove(name);
    }
    for name in listed {
        headers.remove(name.as_str());
    }
}

/// Body wrapper that records frames as they pass through and runs `on_end` exactly once
/// when the body is dropped (completed, failed or abandoned by the peer).
pub struct Tap<B> {
    inner: B,
    limit: usize,
    buffer: Option<Arc<Mutex<CapturedBody>>>,
    on_end: Option<Box<dyn FnOnce() + Send>>,
}

impl<B> Tap<B> {
    /// `buffer` is `None` when capture is paused: frames still flow, nothing is copied.
    pub fn new(inner: B, limit: usize, buffer: Option<Arc<Mutex<CapturedBody>>>) -> Self {
        Self {
            inner,
            limit,
            buffer,
            on_end: None,
        }
    }

    pub fn on_end(mut self, callback: impl FnOnce() + Send + 'static) -> Self {
        self.on_end = Some(Box::new(callback));
        self
    }
}

impl<B> Body for Tap<B>
where
    B: Body<Data = Bytes> + Unpin,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let (Some(buffer), Some(data)) = (&this.buffer, frame.data_ref()) {
                    buffer.lock().record(data, this.limit);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B> Drop for Tap<B> {
    fn drop(&mut self) {
        if let Some(callback) = self.on_end.take() {
            callback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;
    use http::header::{AUTHORIZATION, CONTENT_TYPE};

    #[test]
    fn captured_body_keeps_prefix_and_counts_everything() {
        let mut body = CapturedBody::default();
        body.record(b"hello ", 8);
        body.record(b"world", 8);
        assert_eq!(body.bytes, b"hello wo");
        assert_eq!(body.total, 11);
        assert!(body.is_truncated());
    }

    #[test]
    fn zero_limit_stores_nothing_but_reports_size() {
        let mut body = CapturedBody::default();
        body.record(b"secret", 0);
        assert!(body.bytes.is_empty());
        assert_eq!(body.total, 6);
        assert!(body.is_truncated());
    }

    #[test]
    fn body_view_uses_base64_for_binary() {
        let text = body_view(&CapturedBody {
            bytes: b"{\"ok\":true}".to_vec(),
            total: 11,
        });
        assert_eq!(text.encoding, "text");
        assert!(!text.truncated);

        let binary = body_view(&CapturedBody {
            bytes: vec![0xff, 0x00, 0x10],
            total: 3,
        });
        assert_eq!(binary.encoding, "base64");
        assert_eq!(binary.content, "/wAQ");
    }

    #[test]
    fn redacts_sensitive_headers_only_when_masking() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer abc"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let masked = redact_headers(&headers, true);
        let auth = masked.iter().find(|h| h.key == "authorization").unwrap();
        assert_eq!(auth.value, MASKED_VALUE);
        let content_type = masked.iter().find(|h| h.key == "content-type").unwrap();
        assert_eq!(content_type.value, "application/json");

        let raw = redact_headers(&headers, false);
        let auth = raw.iter().find(|h| h.key == "authorization").unwrap();
        assert_eq!(auth.value, "Bearer abc");
    }

    #[test]
    fn redacts_sensitive_query_params() {
        let masked = redact_url("https://api.test/v1?token=abc&page=2", true);
        assert!(masked.contains("token=%2A%2A%2A") || masked.contains("token=***"));
        assert!(masked.contains("page=2"));
        assert!(!masked.contains("abc"));

        let untouched = redact_url("https://api.test/v1?page=2", true);
        assert_eq!(untouched, "https://api.test/v1?page=2");
    }

    #[test]
    fn strips_hop_by_hop_and_connection_listed_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "connection",
            HeaderValue::from_static("keep-alive, x-trace"),
        );
        headers.insert("x-trace", HeaderValue::from_static("1"));
        headers.insert("proxy-connection", HeaderValue::from_static("keep-alive"));
        headers.insert("accept", HeaderValue::from_static("*/*"));
        strip_hop_by_hop(&mut headers);
        assert!(headers.get("connection").is_none());
        assert!(headers.get("x-trace").is_none());
        assert!(headers.get("proxy-connection").is_none());
        assert!(headers.get("accept").is_some());
    }
}
