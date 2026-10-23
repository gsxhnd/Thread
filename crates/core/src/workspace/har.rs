//! HTTP Archive (HAR) 1.2 import/export for HTTP requests and local history.

use std::path::Path;

use base64::Engine as _;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use url::Url;

use crate::error::{AppError, AppResult};
use crate::services::history::HistoryDetail;

use super::formats::{AuthSpec, HttpBody, HttpBodyField, HttpRequestDoc, KeyValue};
use super::postman::{self, PostmanCollection, PostmanItem};

pub const MAX_HAR_BYTES: usize = 32 * 1024 * 1024;

/// Convert an HTTP Archive into a workspace collection of HTTP request documents.
pub fn parse_har(source: &str) -> AppResult<PostmanCollection> {
    if source.len() > MAX_HAR_BYTES {
        return Err(AppError::Validation(
            "HAR file exceeds the 32 MiB import limit".into(),
        ));
    }
    let har: Value = serde_json::from_str(source)
        .map_err(|error| AppError::Validation(format!("invalid HAR JSON: {error}")))?;
    let log = har
        .get("log")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Validation("HAR document is missing log".into()))?;
    let entries = log
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Validation("HAR log.entries must be an array".into()))?;
    let name = log
        .get("creator")
        .and_then(Value::as_object)
        .and_then(|creator| creator.get("name"))
        .and_then(Value::as_str)
        .map(|name| format!("{name} HAR import"))
        .unwrap_or_else(|| "HAR import".into());
    let items = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| parse_entry(entry, index))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(PostmanCollection {
        name,
        description: Some("Imported from HAR 1.2".into()),
        auth: AuthSpec::default(),
        headers: Vec::new(),
        items,
    })
}

/// Import a HAR file into a workspace using the safe, staged Postman workspace writer.
pub fn import_har(
    root: &Path,
    parent_rel: Option<&str>,
    source: &str,
) -> AppResult<postman::ImportReport> {
    let collection = parse_har(source)?;
    let source = postman::serialize_collection(&collection)?;
    postman::import_collection(root, parent_rel, &source)
}

/// Export selected local HTTP history entries as a HAR 1.2 archive.
pub fn export_history(entries: &[HistoryDetail]) -> AppResult<String> {
    let mut har_entries = Vec::with_capacity(entries.len());
    for detail in entries {
        if detail.summary.protocol != "http" {
            return Err(AppError::Validation(format!(
                "HAR export supports HTTP history entries only: {}",
                detail.summary.id
            )));
        }
        har_entries.push(history_entry_to_har(detail)?);
    }
    let document = json!({
        "log": {
            "version": "1.2",
            "creator": { "name": "Thread", "version": env!("CARGO_PKG_VERSION") },
            "entries": har_entries,
        }
    });
    let mut output = serde_json::to_string_pretty(&document)?;
    output.push('\n');
    Ok(output)
}

fn parse_entry(entry: &Value, index: usize) -> AppResult<PostmanItem> {
    let request = entry
        .get("request")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::Validation(format!("HAR entry {} is missing request", index + 1))
        })?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::Validation(format!(
                "HAR entry {} request.method is required",
                index + 1
            ))
        })?
        .to_ascii_uppercase();
    let raw_url = request.get("url").and_then(Value::as_str).ok_or_else(|| {
        AppError::Validation(format!("HAR entry {} request.url is required", index + 1))
    })?;
    let url = Url::parse(raw_url).map_err(|error| {
        AppError::Validation(format!("invalid HAR request URL {raw_url:?}: {error}"))
    })?;
    let query_params = if let Some(query) = request.get("queryString").and_then(Value::as_array) {
        parse_pairs(query)?
    } else {
        url.query_pairs()
            .map(|(key, value)| KeyValue::new(&key, &value))
            .collect()
    };
    let base_url = strip_query_fragment(raw_url);
    let mut headers = request
        .get("headers")
        .and_then(Value::as_array)
        .map(|values| parse_pairs(values))
        .transpose()?
        .unwrap_or_default();
    let cookie_header = request
        .get("cookies")
        .and_then(Value::as_array)
        .map(|cookies| {
            cookies
                .iter()
                .filter_map(|cookie| {
                    Some(format!(
                        "{}={}",
                        cookie.get("name")?.as_str()?,
                        cookie
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    ))
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    if !cookie_header.is_empty()
        && !headers
            .iter()
            .any(|header| header.key.eq_ignore_ascii_case("cookie"))
    {
        headers.push(KeyValue::new("Cookie", &cookie_header));
    }
    let body = parse_post_data(request.get("postData"), &headers, index)?;
    let request_name = request
        .get("comment")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", method, base_url));
    Ok(PostmanItem::Request(Box::new(HttpRequestDoc {
        version: super::layout::FILE_VERSION,
        id: None,
        name: request_name,
        method,
        url: base_url,
        query_params,
        headers,
        body,
        auth: AuthSpec::default(),
        settings: Default::default(),
        pre_script: String::new(),
        post_script: String::new(),
    })))
}

fn parse_pairs(values: &[Value]) -> AppResult<Vec<KeyValue>> {
    values
        .iter()
        .map(|pair| {
            let object = pair.as_object().ok_or_else(|| {
                AppError::Validation("HAR header/query entry must be an object".into())
            })?;
            let key = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let value = object
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(KeyValue::new(key, value))
        })
        .collect()
}

fn parse_post_data(
    post_data: Option<&Value>,
    headers: &[KeyValue],
    index: usize,
) -> AppResult<HttpBody> {
    let Some(post_data) = post_data else {
        return Ok(HttpBody::default());
    };
    let mime = post_data
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(params) = post_data.get("params").and_then(Value::as_array) {
        let multipart = mime.to_ascii_lowercase().starts_with("multipart/form-data");
        let fields = params.iter().map(|param| {
            if param.get("fileName").and_then(Value::as_str).is_some_and(|name| !name.is_empty()) {
                return Err(AppError::Validation(format!(
                    "HAR entry {} includes a file upload; reselect files manually after importing a text-only HAR",
                    index + 1
                )));
            }
            Ok(HttpBodyField {
                key: param.get("name").and_then(Value::as_str).unwrap_or_default().into(),
                value: param.get("value").and_then(Value::as_str).unwrap_or_default().into(),
                kind: "text".into(),
                enabled: true,
                file_path: None,
                content_type: param.get("contentType").and_then(Value::as_str).map(str::to_string),
            })
        }).collect::<AppResult<Vec<_>>>()?;
        return Ok(HttpBody {
            kind: if multipart {
                "multipart"
            } else {
                "formUrlencoded"
            }
            .into(),
            fields,
            ..HttpBody::default()
        });
    }
    let text = post_data
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if text.is_empty() {
        return Ok(HttpBody::default());
    }
    let is_json = mime.to_ascii_lowercase().contains("json")
        || headers.iter().any(|header| {
            header.key.eq_ignore_ascii_case("content-type")
                && header.value.to_ascii_lowercase().contains("json")
        });
    Ok(HttpBody {
        kind: if is_json { "json" } else { "raw" }.into(),
        content: text.to_string(),
        ..HttpBody::default()
    })
}

fn strip_query_fragment(url: &str) -> String {
    let fragment = url.find('#').unwrap_or(url.len());
    let query = url[..fragment].find('?').unwrap_or(fragment);
    url[..query].to_string()
}

fn history_entry_to_har(detail: &HistoryDetail) -> AppResult<Value> {
    let request_snapshot = &detail.request_snapshot;
    let response_snapshot = detail.response_snapshot.as_ref().unwrap_or(&Value::Null);
    let resolved = request_snapshot.get("resolved");
    let sent = request_snapshot.get("sent");
    let request_headers =
        resolved
            .and_then(|resolved| resolved.get("headers"))
            .and_then(Value::as_array)
            .map(|headers| {
                headers.iter().map(|header| json!({
            "name": header.get("key").and_then(Value::as_str).unwrap_or_default(),
            "value": header.get("value").and_then(Value::as_str).unwrap_or_default(),
        })).collect::<Vec<_>>()
            })
            .unwrap_or_default();
    let request_body = resolved
        .and_then(|resolved| resolved.get("bodyPreview"))
        .and_then(Value::as_str);
    let content_type = response_snapshot
        .get("contentType")
        .and_then(Value::as_str)
        .or_else(|| {
            response_snapshot
                .get("headers")
                .and_then(|headers| headers.get("content-type"))
                .and_then(Value::as_array)
                .and_then(|values| values.first())
                .and_then(Value::as_str)
        })
        .unwrap_or("text/plain");
    let mut response_content = json!({
        "size": response_snapshot.get("bodySize").and_then(Value::as_i64).unwrap_or(0),
        "mimeType": content_type,
    });
    let (body, encoding) = history_body(detail)?;
    if let Some(body) = body {
        response_content["text"] = Value::String(body);
    }
    if let Some(encoding) = encoding {
        response_content["encoding"] = Value::String(encoding.into());
    }
    let response_headers = response_snapshot.get("headers").and_then(Value::as_object)
        .map(|headers| headers.iter().flat_map(|(name, values)| {
            values.as_array().into_iter().flatten().map(move |value| json!({ "name": name, "value": value.as_str().unwrap_or_default() }))
        }).collect::<Vec<_>>()).unwrap_or_default();
    let timing = response_snapshot.get("timing").unwrap_or(&Value::Null);
    let dns = timing.get("dnsMs").and_then(Value::as_f64).unwrap_or(-1.0);
    let tcp = timing.get("tcpMs").and_then(Value::as_f64).unwrap_or(-1.0);
    let tls = timing.get("tlsMs").and_then(Value::as_f64).unwrap_or(-1.0);
    let ttfb = timing.get("ttfbMs").and_then(Value::as_f64).unwrap_or(-1.0);
    let download = timing
        .get("downloadMs")
        .and_then(Value::as_f64)
        .unwrap_or(-1.0);
    let total = timing
        .get("totalMs")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| detail.summary.duration_ms.unwrap_or_default() as f64);
    let started = DateTime::<Utc>::from_timestamp_millis(detail.summary.created_at)
        .map(|date| date.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    let post_data = request_body.map(|text| json!({
        "mimeType": sent.and_then(|sent| sent.get("body")).and_then(|body| body.get("contentType")).and_then(Value::as_str).unwrap_or("text/plain"),
        "text": text,
    }));
    let mut har_request = json!({
        "method": detail.summary.method.as_deref().unwrap_or("GET"),
        "url": detail.summary.url.as_deref().unwrap_or_default(),
        "httpVersion": "HTTP/1.1",
        "headers": request_headers,
        "queryString": [],
        "cookies": [],
        "headersSize": -1,
        "bodySize": request_body.map_or(0, |body| body.len() as i64),
    });
    if let Some(post_data) = post_data {
        har_request["postData"] = post_data;
    }
    Ok(json!({
        "startedDateTime": started,
        "time": total,
        "request": har_request,
        "response": {
            "status": response_snapshot.get("statusCode").and_then(Value::as_u64).unwrap_or(0),
            "statusText": response_snapshot.get("statusText").and_then(Value::as_str).unwrap_or_default(),
            "httpVersion": "HTTP/1.1",
            "headers": response_headers,
            "cookies": [],
            "content": response_content,
            "redirectURL": "",
            "headersSize": -1,
            "bodySize": response_snapshot.get("bodySize").and_then(Value::as_i64).unwrap_or(0),
        },
        "cache": {},
        "timings": { "blocked": -1, "dns": dns, "connect": tcp, "send": 0, "wait": ttfb, "receive": download, "ssl": tls },
    }))
}

fn history_body(detail: &HistoryDetail) -> AppResult<(Option<String>, Option<&'static str>)> {
    if let Some(body) = &detail.response_body {
        return Ok((Some(body.clone()), None));
    }
    let Some(path) = detail.response_body_file_path.as_deref() else {
        return Ok((None, None));
    };
    let bytes = std::fs::read(path).map_err(|error| AppError::Io(error.to_string()))?;
    match String::from_utf8(bytes.clone()) {
        Ok(text) => Ok((Some(text), None)),
        Err(_) => Ok((
            Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
            Some("base64"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::history::HistorySummary;

    const SAMPLE: &str = r#"{
      "log": { "version": "1.2", "creator": { "name": "HarTester" }, "entries": [
        { "request": { "method": "POST", "url": "https://api.example.test/pets?active=true", "headers": [{ "name": "Content-Type", "value": "application/json" }], "queryString": [{ "name": "active", "value": "true" }], "postData": { "mimeType": "application/json", "text": "{\"name\":\"Ada\"}" } } }
      ] }
    }"#;

    #[test]
    fn imports_har_request_url_headers_query_and_json_body() {
        let collection = parse_har(SAMPLE).unwrap();
        assert_eq!(collection.name, "HarTester HAR import");
        let PostmanItem::Request(request) = &collection.items[0] else {
            panic!("expected HTTP request")
        };
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.example.test/pets");
        assert_eq!(request.query_params[0].key, "active");
        assert_eq!(request.body.kind, "json");
    }

    #[test]
    fn exports_history_response_as_har_entry() {
        let detail = HistoryDetail {
            summary: HistorySummary {
                id: "entry-1".into(),
                protocol: "http".into(),
                name: Some("Create pet".into()),
                method: Some("POST".into()),
                url: Some("https://api.example.test/pets".into()),
                status_code: Some(201),
                duration_ms: Some(45),
                created_at: 1_700_000_000_000,
                request_file_path: None,
                has_body_file: false,
            },
            workspace_path: String::new(),
            request_snapshot: json!({ "resolved": { "headers": [], "bodyPreview": "{}" } }),
            response_snapshot: Some(
                json!({ "statusCode": 201, "statusText": "Created", "bodySize": 2, "contentType": "application/json", "headers": {}, "timing": { "totalMs": 45.0 } }),
            ),
            response_body: Some("{}".into()),
            response_body_file_path: None,
        };
        let exported = export_history(&[detail]).unwrap();
        let parsed: Value = serde_json::from_str(&exported).unwrap();
        assert_eq!(parsed["log"]["entries"][0]["response"]["status"], 201);
        assert_eq!(
            parsed["log"]["entries"][0]["response"]["content"]["text"],
            "{}"
        );
    }

    #[test]
    fn rejects_har_file_uploads_and_invalid_roots() {
        let upload = r#"{"log":{"version":"1.2","entries":[{"request":{"method":"POST","url":"https://x","postData":{"mimeType":"multipart/form-data","params":[{"name":"file","fileName":"/tmp/x"}]}}}]}}"#;
        assert!(parse_har(upload).is_err());
        assert!(parse_har("{}").is_err());
    }
}
