//! 把当前请求导出成可运行的 cURL 命令，方便在终端里复现。

use thread_core::services::http::HttpRequestConfig;
use thread_core::workspace::formats::{AuthSpec, HttpBody, KeyValue};

/// 单引号包裹的 shell 参数，内部的单引号按 `'\''` 转义。
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 已启用的键值行。
fn enabled_pairs(pairs: &[KeyValue]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter(|pair| pair.enabled && !pair.key.trim().is_empty())
        .map(|pair| (pair.key.clone(), pair.value.clone()))
        .collect()
}

/// 请求头里是否已经带了 Content-Type（不区分大小写）。
fn has_content_type(config: &HttpRequestConfig) -> bool {
    config
        .headers
        .iter()
        .any(|header| header.enabled && header.key.eq_ignore_ascii_case("content-type"))
}

/// 最小 percent-encoding：保留 RFC 3986 的 unreserved 字符。
fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// 把查询参数接在 URL 后面（已有 `?` 时用 `&`）。
fn url_with_query(config: &HttpRequestConfig, extra: &[(String, String)]) -> String {
    let mut url = config.url.clone();
    let mut pairs = enabled_pairs(&config.query_params);
    pairs.extend(extra.iter().cloned());
    if pairs.is_empty() {
        return url;
    }
    let encoded: Vec<String> = pairs
        .into_iter()
        .map(|(key, value)| format!("{}={}", urlencode(&key), urlencode(&value)))
        .collect();
    let joiner = if url.contains('?') { "&" } else { "?" };
    url.push_str(joiner);
    url.push_str(&encoded.join("&"));
    url
}

/// 认证对应的参数；apiKey 放 query 时把键值对记进 `extra_query`，由 URL 拼接处理。
fn auth_args(auth: &AuthSpec, extra_query: &mut Vec<(String, String)>) -> Vec<String> {
    let mut args = Vec::new();
    match auth.kind.as_str() {
        "bearer" => {
            if let Some(token) = auth.token.as_deref().filter(|token| !token.is_empty()) {
                args.push("-H".into());
                args.push(sh_quote(&format!("Authorization: Bearer {token}")));
            }
        }
        "basic" => {
            let username = auth.username.as_deref().unwrap_or_default();
            let password = auth.password.as_deref().unwrap_or_default();
            if !username.is_empty() || !password.is_empty() {
                args.push("-u".into());
                args.push(sh_quote(&format!("{username}:{password}")));
            }
        }
        "apiKey" => {
            let key = auth.key.as_deref().unwrap_or_default();
            let value = auth.value.as_deref().unwrap_or_default();
            if key.is_empty() {
                return args;
            }
            if auth.location.as_deref() == Some("query") {
                extra_query.push((key.to_string(), value.to_string()));
            } else {
                args.push("-H".into());
                args.push(sh_quote(&format!("{key}: {value}")));
            }
        }
        _ => {}
    }
    args
}

/// 请求体对应的参数。`json` 在没有显式 Content-Type 头时补一个默认值。
fn body_args(body: &HttpBody, has_content_type: bool) -> Vec<String> {
    let mut args = Vec::new();
    match body.kind.as_str() {
        "json" | "raw" => {
            if body.content.is_empty() {
                return args;
            }
            if body.kind == "json" && !has_content_type {
                args.push("-H".into());
                args.push(sh_quote("Content-Type: application/json"));
            }
            args.push("--data".into());
            args.push(sh_quote(&body.content));
        }
        "formUrlencoded" => {
            let fields: Vec<KeyValue> = body
                .fields
                .iter()
                .map(|field| KeyValue {
                    key: field.key.clone(),
                    value: if field.kind == "file" {
                        field.file_path.clone().unwrap_or_default()
                    } else {
                        field.value.clone()
                    },
                    enabled: field.enabled,
                    description: None,
                })
                .collect();
            for (key, value) in enabled_pairs(&fields) {
                args.push("--data-urlencode".into());
                args.push(sh_quote(&format!("{key}={value}")));
            }
        }
        "multipart" => {
            for field in &body.fields {
                if !field.enabled || field.key.trim().is_empty() {
                    continue;
                }
                args.push("-F".into());
                if field.kind == "file" {
                    let path = field.file_path.clone().unwrap_or_default();
                    args.push(sh_quote(&format!("{}=@{path}", field.key)));
                } else {
                    args.push(sh_quote(&format!("{}={}", field.key, field.value)));
                }
            }
        }
        _ => {}
    }
    args
}

/// 导出当前请求配置为 cURL 命令。变量占位符原样保留。
pub fn command(config: &HttpRequestConfig) -> String {
    let mut extra_query = Vec::new();
    let auth = auth_args(&config.auth, &mut extra_query);

    let mut args = vec![
        "curl".to_string(),
        "-X".to_string(),
        config.method.clone(),
        sh_quote(&url_with_query(config, &extra_query)),
    ];
    args.extend(auth);
    for (key, value) in enabled_pairs(&config.headers) {
        args.push("-H".into());
        args.push(sh_quote(&format!("{key}: {value}")));
    }
    args.extend(body_args(&config.body, has_content_type(config)));
    args.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use thread_core::workspace::formats::HttpBodyField;

    fn config() -> HttpRequestConfig {
        HttpRequestConfig {
            method: "GET".into(),
            url: "https://example.com/api".into(),
            query_params: Vec::new(),
            headers: Vec::new(),
            body: HttpBody::default(),
            auth: AuthSpec::default(),
            settings: Default::default(),
        }
    }

    fn kv(key: &str, value: &str, enabled: bool) -> KeyValue {
        KeyValue {
            key: key.into(),
            value: value.into(),
            enabled,
            description: None,
        }
    }

    #[test]
    fn simple_get() {
        assert_eq!(command(&config()), "curl -X GET 'https://example.com/api'");
    }

    #[test]
    fn appends_query_params_and_skips_disabled() {
        let mut cfg = config();
        cfg.query_params = vec![kv("page", "2", true), kv("skip", "off", false)];
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api?page=2'"
        );
    }

    #[test]
    fn joins_with_ampersand_when_url_has_query() {
        let mut cfg = config();
        cfg.url = "https://example.com/api?x=1".into();
        cfg.query_params = vec![kv("page", "2", true)];
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api?x=1&page=2'"
        );
    }

    #[test]
    fn headers_skip_disabled_rows() {
        let mut cfg = config();
        cfg.method = "POST".into();
        cfg.headers = vec![kv("X-Trace", "abc", true), kv("X-Skip", "no", false)];
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' -H 'X-Trace: abc'"
        );
    }

    #[test]
    fn json_body_sets_content_type_once() {
        let mut cfg = config();
        cfg.method = "POST".into();
        cfg.body = HttpBody {
            kind: "json".into(),
            content: "{\"a\":1}".into(),
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' -H 'Content-Type: application/json' --data '{\"a\":1}'"
        );
        // 显式给了 Content-Type（即使大小写不同）就不再补。
        cfg.headers = vec![kv("content-type", "application/vnd.api+json", true)];
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' -H 'content-type: application/vnd.api+json' --data '{\"a\":1}'"
        );
    }

    #[test]
    fn raw_body_has_no_content_type() {
        let mut cfg = config();
        cfg.method = "POST".into();
        cfg.body = HttpBody {
            kind: "raw".into(),
            content: "hello".into(),
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' --data 'hello'"
        );
    }

    #[test]
    fn form_fields_use_urlencode() {
        let mut cfg = config();
        cfg.method = "POST".into();
        cfg.body = HttpBody {
            kind: "formUrlencoded".into(),
            fields: vec![
                HttpBodyField {
                    key: "name".into(),
                    value: "张 三".into(),
                    kind: "text".into(),
                    enabled: true,
                    file_path: None,
                    content_type: None,
                },
                HttpBodyField {
                    key: "skip".into(),
                    value: "x".into(),
                    kind: "text".into(),
                    enabled: false,
                    file_path: None,
                    content_type: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' --data-urlencode 'name=张 三'"
        );
    }

    #[test]
    fn multipart_file_uses_at_syntax() {
        let mut cfg = config();
        cfg.method = "POST".into();
        cfg.body = HttpBody {
            kind: "multipart".into(),
            fields: vec![
                HttpBodyField {
                    key: "avatar".into(),
                    value: String::new(),
                    kind: "file".into(),
                    enabled: true,
                    file_path: Some("/tmp/a.png".into()),
                    content_type: None,
                },
                HttpBodyField {
                    key: "note".into(),
                    value: "hi".into(),
                    kind: "text".into(),
                    enabled: true,
                    file_path: None,
                    content_type: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X POST 'https://example.com/api' -F 'avatar=@/tmp/a.png' -F 'note=hi'"
        );
    }

    #[test]
    fn quotes_shell_metacharacters() {
        let mut cfg = config();
        cfg.url = "https://example.com/it's".into();
        assert_eq!(command(&cfg), "curl -X GET 'https://example.com/it'\\''s'");
    }

    #[test]
    fn auth_variants() {
        let mut cfg = config();
        cfg.auth = AuthSpec {
            kind: "bearer".into(),
            token: Some("t k".into()),
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api' -H 'Authorization: Bearer t k'"
        );

        cfg.auth = AuthSpec {
            kind: "basic".into(),
            username: Some("user".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api' -u 'user:pass'"
        );

        cfg.auth = AuthSpec {
            kind: "apiKey".into(),
            key: Some("api_key".into()),
            value: Some("secret".into()),
            location: Some("query".into()),
            ..Default::default()
        };
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api?api_key=secret'"
        );

        cfg.auth.location = Some("header".into());
        assert_eq!(
            command(&cfg),
            "curl -X GET 'https://example.com/api' -H 'api_key: secret'"
        );
    }
}
