//! 把当前请求导出成可运行的 cURL 命令，方便在终端里复现。

use thread_core::error::{AppError, AppResult};
use thread_core::services::http::HttpRequestConfig;
use thread_core::workspace::formats::{
    AuthSpec, HttpBody, HttpBodyField, HttpRequestDoc, KeyValue,
};

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

/// Imports a conservative subset of shell-style cURL commands without executing a shell.
/// Unsupported options are rejected rather than silently changing request semantics.
pub fn parse_command(command: &str) -> AppResult<HttpRequestDoc> {
    let words = shell_words(command).map_err(AppError::Validation)?;
    if words.first().map(String::as_str) != Some("curl") {
        return Err(AppError::Validation(
            "Expected a command beginning with `curl`.".into(),
        ));
    }

    let mut document = HttpRequestDoc::new("Imported cURL");
    let mut data = Vec::new();
    let mut urlencoded_fields = Vec::new();
    let mut multipart_fields = Vec::new();
    let mut explicit_method = false;
    let mut get_mode = false;
    let mut index = 1;

    while index < words.len() {
        let argument = &words[index];
        index += 1;
        if matches!(argument.as_str(), ";" | "&&" | "||" | "|" | ">" | ">>") {
            return Err(AppError::Validation(
                "Shell operators are not accepted in imported cURL commands.".into(),
            ));
        }

        let (option, inline_value) = argument
            .split_once('=')
            .filter(|(option, _)| option.starts_with("--"))
            .map_or((argument.as_str(), None), |(option, value)| {
                (option, Some(value.to_string()))
            });
        let mut value = |name: &str| -> AppResult<String> {
            if let Some(value) = inline_value.clone() {
                return Ok(value);
            }
            let Some(next) = words.get(index) else {
                return Err(AppError::Validation(format!("Missing value for {name}.")));
            };
            index += 1;
            Ok(next.clone())
        };

        match option {
            "--url" => document.url = value(option)?,
            "-X" | "--request" => {
                document.method = value(option)?.to_ascii_uppercase();
                explicit_method = true;
            }
            "-H" | "--header" => {
                let header = value(option)?;
                let Some((key, value)) = header.split_once(':') else {
                    return Err(AppError::Validation(format!(
                        "Header must have a name and value: {header}"
                    )));
                };
                document
                    .headers
                    .push(KeyValue::new(key.trim(), value.trim()));
            }
            "-u" | "--user" => {
                let credentials = value(option)?;
                let (username, password) = credentials
                    .split_once(':')
                    .unwrap_or((credentials.as_str(), ""));
                document.auth = AuthSpec {
                    kind: "basic".into(),
                    username: Some(username.to_string()),
                    password: Some(password.to_string()),
                    ..AuthSpec::default()
                };
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                let payload = value(option)?;
                if payload.starts_with('@') && option != "--data-raw" {
                    return Err(AppError::Validation(
                        "Reading request bodies from files is not supported yet.".into(),
                    ));
                }
                data.push(payload);
            }
            "--data-urlencode" => {
                let field = value(option)?;
                if field.starts_with('@') {
                    return Err(AppError::Validation(
                        "Reading --data-urlencode values from files is not supported yet.".into(),
                    ));
                }
                let Some((key, value)) = field.split_once('=') else {
                    return Err(AppError::Validation(
                        "--data-urlencode currently requires `name=value`.".into(),
                    ));
                };
                urlencoded_fields.push(HttpBodyField {
                    key: key.to_string(),
                    value: value.to_string(),
                    kind: "text".into(),
                    enabled: true,
                    file_path: None,
                    content_type: None,
                });
            }
            "-F" | "--form" => {
                let field = value(option)?;
                let Some((key, value)) = field.split_once('=') else {
                    return Err(AppError::Validation(format!(
                        "Form field must have a name and value: {field}"
                    )));
                };
                if value.starts_with('@') || value.starts_with('<') {
                    return Err(AppError::Validation(
                        "File-backed multipart fields are not imported. Re-select the file in the request editor.".into(),
                    ));
                }
                multipart_fields.push(HttpBodyField {
                    key: key.to_string(),
                    value: value.to_string(),
                    kind: "text".into(),
                    enabled: true,
                    file_path: None,
                    content_type: None,
                });
            }
            "-b" | "--cookie" => {
                document
                    .headers
                    .push(KeyValue::new("Cookie", &value(option)?));
            }
            "-A" | "--user-agent" => {
                document
                    .headers
                    .push(KeyValue::new("User-Agent", &value(option)?));
            }
            "-e" | "--referer" => {
                document
                    .headers
                    .push(KeyValue::new("Referer", &value(option)?));
            }
            "--max-time" | "-m" | "--connect-timeout" => {
                let seconds = value(option)?
                    .parse::<f64>()
                    .ok()
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                    .ok_or_else(|| {
                        AppError::Validation(format!("Invalid timeout value for {option}."))
                    })?;
                document.settings.timeout_ms =
                    Some((seconds * 1000.0).ceil().min(u64::MAX as f64) as u64);
            }
            "-I" | "--head" => {
                document.method = "HEAD".into();
                explicit_method = true;
            }
            "-G" | "--get" => get_mode = true,
            "-L" | "--location" => document.settings.follow_redirects = Some(true),
            "-k" | "--insecure" => document.settings.verify_ssl = Some(false),
            "--compressed" | "-s" | "--silent" | "-S" | "--show-error" | "--fail"
            | "--fail-with-body" | "-i" | "--include" | "--globoff" | "--path-as-is" => {}
            "--max-redirs" | "--connect-to" => {
                let _ = value(option)?;
                if option == "--connect-to" {
                    return Err(AppError::Validation(
                        "--connect-to cannot be represented by a request tab.".into(),
                    ));
                }
            }
            "--" => {
                if let Some(url) = words.get(index)
                    && document.url.is_empty()
                {
                    document.url = url.clone();
                }
                break;
            }
            _ if option.starts_with("-X") && option.len() > 2 => {
                document.method = option[2..].to_ascii_uppercase();
                explicit_method = true;
            }
            _ if option.starts_with("-H") && option.len() > 2 => {
                let header = &option[2..];
                let Some((key, value)) = header.split_once(':') else {
                    return Err(AppError::Validation(format!(
                        "Header must have a name and value: {header}"
                    )));
                };
                document
                    .headers
                    .push(KeyValue::new(key.trim(), value.trim()));
            }
            _ if option.starts_with('-') => {
                return Err(AppError::Validation(format!(
                    "Unsupported cURL option: {option}"
                )));
            }
            _ if document.url.is_empty() => document.url = argument.clone(),
            _ => {
                return Err(AppError::Validation(format!(
                    "Unexpected argument in cURL command: {argument}"
                )));
            }
        }
    }

    if document.url.trim().is_empty() {
        return Err(AppError::Validation(
            "The cURL command does not include a URL.".into(),
        ));
    }
    if !multipart_fields.is_empty() && (!data.is_empty() || !urlencoded_fields.is_empty()) {
        return Err(AppError::Validation(
            "Combining multipart fields with --data is not supported.".into(),
        ));
    }
    if !urlencoded_fields.is_empty() && !data.is_empty() {
        return Err(AppError::Validation(
            "Combining --data-urlencode with other --data options is not supported.".into(),
        ));
    }

    if get_mode {
        if !multipart_fields.is_empty() {
            return Err(AppError::Validation(
                "--get cannot be combined with multipart form data.".into(),
            ));
        }
        let mut query = data.join("&");
        if !urlencoded_fields.is_empty() {
            query = urlencoded_fields
                .iter()
                .map(|field| format!("{}={}", urlencode(&field.key), urlencode(&field.value)))
                .collect::<Vec<_>>()
                .join("&");
        }
        if !query.is_empty() {
            let separator = if document.url.contains('?') { '&' } else { '?' };
            document.url.push(separator);
            document.url.push_str(&query);
        }
        document.method = "GET".into();
        document.body = HttpBody::default();
    } else if !multipart_fields.is_empty() {
        document.body = HttpBody {
            kind: "multipart".into(),
            fields: multipart_fields,
            ..HttpBody::default()
        };
        if !explicit_method {
            document.method = "POST".into();
        }
    } else if !urlencoded_fields.is_empty() {
        document.body = HttpBody {
            kind: "formUrlencoded".into(),
            fields: urlencoded_fields,
            ..HttpBody::default()
        };
        if !explicit_method {
            document.method = "POST".into();
        }
    } else if !data.is_empty() {
        let content = data.join("&");
        let is_json = document.headers.iter().any(|header| {
            header.key.eq_ignore_ascii_case("content-type")
                && header.value.to_ascii_lowercase().contains("json")
        });
        document.body = HttpBody {
            kind: if is_json { "json" } else { "raw" }.into(),
            content,
            ..HttpBody::default()
        };
        if !explicit_method {
            document.method = "POST".into();
        }
    }

    Ok(document)
}

fn shell_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if quote == Some('\'') {
            if ch == '\'' {
                quote = None;
            } else {
                word.push(ch);
            }
            continue;
        }
        if ch == '\\' {
            let Some(next) = chars.next() else {
                return Err("Trailing backslash in cURL command.".into());
            };
            if next != '\n' && next != '\r' {
                word.push(next);
                started = true;
            }
            continue;
        }
        if let Some(delimiter) = quote {
            if ch == delimiter {
                quote = None;
            } else {
                word.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                started = true;
            }
            ch if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("Unclosed quote in cURL command.".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
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

    #[test]
    fn imports_json_post_headers_and_shell_escaped_url() {
        let document = parse_command(
            "curl -X POST 'https://example.com/it'\\''s' -H 'Content-Type: application/json' -H 'X-Test: a b' --data '{\"ok\":true}'",
        )
        .unwrap();
        assert_eq!(document.method, "POST");
        assert_eq!(document.url, "https://example.com/it's");
        assert_eq!(document.body.kind, "json");
        assert_eq!(document.body.content, "{\"ok\":true}");
        assert_eq!(document.headers[1].value, "a b");
    }

    #[test]
    fn imports_basic_auth_and_urlencoded_fields() {
        let document = parse_command(
            "curl https://example.com -u 'alice:p a s s' --data-urlencode 'name=two words'",
        )
        .unwrap();
        assert_eq!(document.method, "POST");
        assert_eq!(document.auth.kind, "basic");
        assert_eq!(document.auth.username.as_deref(), Some("alice"));
        assert_eq!(document.auth.password.as_deref(), Some("p a s s"));
        assert_eq!(document.body.kind, "formUrlencoded");
        assert_eq!(document.body.fields[0].value, "two words");
    }

    #[test]
    fn imports_get_query_data_and_common_request_options() {
        let document = parse_command(
            "curl -G --data-urlencode 'q=rust lang' --max-time 2.5 -L --insecure https://example.com/search",
        )
        .unwrap();
        assert_eq!(document.method, "GET");
        assert_eq!(document.url, "https://example.com/search?q=rust%20lang");
        assert_eq!(document.settings.timeout_ms, Some(2500));
        assert_eq!(document.settings.follow_redirects, Some(true));
        assert_eq!(document.settings.verify_ssl, Some(false));
        assert_eq!(document.body.kind, "none");
    }

    #[test]
    fn rejects_unknown_options_instead_of_dropping_semantics() {
        let error =
            parse_command("curl --some-unmapped-option value https://example.com").unwrap_err();
        assert!(error.to_string().contains("Unsupported cURL option"));
    }

    #[test]
    fn rejects_file_backed_form_fields_for_explicit_reselection() {
        let error =
            parse_command("curl -F 'upload=@/tmp/private.txt' https://example.com").unwrap_err();
        assert!(error.to_string().contains("Re-select the file"));
    }
}
