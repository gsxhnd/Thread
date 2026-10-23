//! 把当前 HTTP 请求生成成不同语言的调用代码。变量占位符原样保留。

use base64::Engine as _;
use thread_core::services::http::HttpRequestConfig;
use thread_core::workspace::formats::HttpBody;

/// 生成代码菜单里的语言。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnippetLanguage {
    Curl,
    JavaScript,
    Python,
    Go,
    Java,
}

impl SnippetLanguage {
    pub const ALL: [Self; 5] = [
        Self::Curl,
        Self::JavaScript,
        Self::Python,
        Self::Go,
        Self::Java,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Curl => "cURL",
            Self::JavaScript => "JavaScript",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
        }
    }
}

/// 按所选语言生成一段可以直接参考的请求代码。
pub fn render(config: &HttpRequestConfig, language: SnippetLanguage) -> String {
    match language {
        SnippetLanguage::Curl => crate::curl::command(config),
        SnippetLanguage::JavaScript => javascript(&prepare(config)),
        SnippetLanguage::Python => python(&prepare(config)),
        SnippetLanguage::Go => go(&prepare(config)),
        SnippetLanguage::Java => java(&prepare(config)),
    }
}

struct Call {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    /// 需要补上、而请求头里还没有的 Content-Type。
    content_type: Option<String>,
    /// 无法展开成普通正文时的说明，例如 multipart。
    note: Option<String>,
}

fn prepare(config: &HttpRequestConfig) -> Call {
    let mut headers: Vec<(String, String)> = config
        .headers
        .iter()
        .filter(|header| header.enabled && !header.key.trim().is_empty())
        .map(|header| (header.key.clone(), header.value.clone()))
        .collect();
    let mut query: Vec<(String, String)> = config
        .query_params
        .iter()
        .filter(|param| param.enabled && !param.key.trim().is_empty())
        .map(|param| (param.key.clone(), param.value.clone()))
        .collect();

    match config.auth.kind.as_str() {
        "bearer" => {
            if let Some(token) = config
                .auth
                .token
                .as_deref()
                .filter(|token| !token.is_empty())
            {
                headers.push(("Authorization".into(), format!("Bearer {token}")));
            }
        }
        "basic" => {
            let username = config.auth.username.as_deref().unwrap_or_default();
            let password = config.auth.password.as_deref().unwrap_or_default();
            if !username.is_empty() || !password.is_empty() {
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                headers.push(("Authorization".into(), format!("Basic {encoded}")));
            }
        }
        "apiKey" => {
            let key = config.auth.key.as_deref().unwrap_or_default();
            let value = config.auth.value.as_deref().unwrap_or_default();
            if !key.is_empty() {
                if config.auth.location.as_deref() == Some("query") {
                    query.push((key.to_string(), value.to_string()));
                } else {
                    headers.push((key.to_string(), value.to_string()));
                }
            }
        }
        _ => {}
    }

    let (body, mut content_type, note) = body_parts(&config.body, &headers);
    if let Some(explicit) = config
        .body
        .content_type
        .clone()
        .filter(|value| !value.is_empty())
        && !has_header(&headers, "content-type")
    {
        content_type = Some(explicit);
    }

    Call {
        method: config.method.clone(),
        url: url_with_query(&config.url, &query),
        headers,
        body,
        content_type,
        note,
    }
}

fn body_parts(
    body: &HttpBody,
    headers: &[(String, String)],
) -> (Option<String>, Option<String>, Option<String>) {
    match body.kind.as_str() {
        "json" | "raw" if !body.content.is_empty() => {
            let content_type = (body.kind == "json" && !has_header(headers, "content-type"))
                .then(|| "application/json".to_string());
            (Some(body.content.clone()), content_type, None)
        }
        "formUrlencoded" => {
            let encoded: Vec<String> = body
                .fields
                .iter()
                .filter(|field| field.enabled && !field.key.trim().is_empty())
                .map(|field| {
                    let value = if field.kind == "file" {
                        field.file_path.clone().unwrap_or_default()
                    } else {
                        field.value.clone()
                    };
                    format!("{}={}", urlencode(&field.key), urlencode(&value))
                })
                .collect();
            if encoded.is_empty() {
                (None, None, None)
            } else {
                let content_type = (!has_header(headers, "content-type"))
                    .then(|| "application/x-www-form-urlencoded".to_string());
                (Some(encoded.join("&")), content_type, None)
            }
        }
        "multipart" => {
            let fields: Vec<String> = body
                .fields
                .iter()
                .filter(|field| field.enabled && !field.key.trim().is_empty())
                .map(|field| {
                    if field.kind == "file" {
                        format!(
                            "{}=@{}",
                            field.key,
                            field.file_path.clone().unwrap_or_default()
                        )
                    } else {
                        format!("{}={}", field.key, field.value)
                    }
                })
                .collect();
            let listed = if fields.is_empty() {
                "(none)".to_string()
            } else {
                fields.join(", ")
            };
            (
                None,
                None,
                Some(format!("multipart/form-data fields: {listed}")),
            )
        }
        "binary" => (
            None,
            None,
            Some(format!(
                "binary body: {}",
                body.file_path.clone().unwrap_or_else(|| "(no file)".into())
            )),
        ),
        _ => (None, None, None),
    }
}

fn javascript(call: &Call) -> String {
    let mut lines = Vec::new();
    if let Some(note) = &call.note {
        lines.push(format!("// {note}"));
    }
    lines.push(format!("fetch({}, {{", quote(&call.url)));
    lines.push(format!("  method: {},", quote(&call.method)));
    let headers = headers_with_type(call);
    if !headers.is_empty() {
        lines.push("  headers: {".into());
        for (key, value) in headers {
            lines.push(format!("    {}: {},", quote(&key), quote(&value)));
        }
        lines.push("  },".into());
    }
    if let Some(body) = &call.body {
        lines.push(format!("  body: {},", quote(body)));
    }
    lines.push("})".into());
    lines.push("  .then((response) => response.text())".into());
    lines.push("  .then(console.log);".into());
    lines.join("\n")
}

fn python(call: &Call) -> String {
    let mut lines = vec![
        "import requests".to_string(),
        String::new(),
        "response = requests.request(".into(),
        format!("    {},", quote(&call.method)),
        format!("    {},", quote(&call.url)),
    ];
    if let Some(note) = &call.note {
        lines.insert(0, format!("# {note}"));
    }
    let headers = headers_with_type(call);
    if !headers.is_empty() {
        lines.push("    headers={".into());
        for (key, value) in headers {
            lines.push(format!("        {}: {},", quote(&key), quote(&value)));
        }
        lines.push("    },".into());
    }
    if let Some(body) = &call.body {
        lines.push(format!("    data={},", quote(body)));
    }
    lines.push(")".into());
    lines.push("print(response.text)".into());
    lines.join("\n")
}

fn go(call: &Call) -> String {
    let reader = match &call.body {
        Some(body) => format!("strings.NewReader({})", quote(body)),
        None => "nil".to_string(),
    };
    let mut lines = Vec::new();
    if let Some(note) = &call.note {
        lines.push(format!("// {note}"));
    }
    lines.push("package main".into());
    lines.push(String::new());
    lines.push("import (".into());
    lines.push("\t\"fmt\"".into());
    lines.push("\t\"io\"".into());
    lines.push("\t\"net/http\"".into());
    if call.body.is_some() {
        lines.push("\t\"strings\"".into());
    }
    lines.push(")".into());
    lines.push(String::new());
    lines.push("func main() {".into());
    lines.push(format!(
        "\treq, err := http.NewRequest({}, {}, {reader})",
        quote(&call.method),
        quote(&call.url)
    ));
    lines.push("\tif err != nil {".into());
    lines.push("\t\tpanic(err)".into());
    lines.push("\t}".into());
    for (key, value) in headers_with_type(call) {
        lines.push(format!(
            "\treq.Header.Set({}, {})",
            quote(&key),
            quote(&value)
        ));
    }
    lines.push("\tresp, err := http.DefaultClient.Do(req)".into());
    lines.push("\tif err != nil {".into());
    lines.push("\t\tpanic(err)".into());
    lines.push("\t}".into());
    lines.push("\tdefer resp.Body.Close()".into());
    lines.push("\tbody, _ := io.ReadAll(resp.Body)".into());
    lines.push("\tfmt.Println(string(body))".into());
    lines.push("}".into());
    lines.join("\n")
}

fn java(call: &Call) -> String {
    let publisher = match &call.body {
        Some(body) => format!("HttpRequest.BodyPublishers.ofString({})", quote(body)),
        None => "HttpRequest.BodyPublishers.noBody()".to_string(),
    };
    let mut lines = Vec::new();
    if let Some(note) = &call.note {
        lines.push(format!("// {note}"));
    }
    lines.push("import java.net.URI;".into());
    lines.push("import java.net.http.HttpClient;".into());
    lines.push("import java.net.http.HttpRequest;".into());
    lines.push("import java.net.http.HttpResponse;".into());
    lines.push(String::new());
    lines.push("HttpClient client = HttpClient.newHttpClient();".into());
    lines.push("HttpRequest request = HttpRequest.newBuilder()".into());
    lines.push(format!("    .uri(URI.create({}))", quote(&call.url)));
    lines.push(format!("    .method({}, {publisher})", quote(&call.method)));
    for (key, value) in headers_with_type(call) {
        lines.push(format!("    .header({}, {})", quote(&key), quote(&value)));
    }
    lines.push("    .build();".into());
    lines.push(
        "HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());"
            .into(),
    );
    lines.push("System.out.println(response.body());".into());
    lines.join("\n")
}

fn headers_with_type(call: &Call) -> Vec<(String, String)> {
    let mut headers = call.headers.clone();
    if let Some(content_type) = &call.content_type
        && !has_header(&headers, "content-type")
    {
        headers.push(("Content-Type".into(), content_type.clone()));
    }
    headers
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case(name))
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn url_with_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let encoded: Vec<String> = query
        .iter()
        .map(|(key, value)| format!("{}={}", urlencode(key), urlencode(value)))
        .collect();
    let joiner = if url.contains('?') { "&" } else { "?" };
    format!("{url}{joiner}{}", encoded.join("&"))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use thread_core::workspace::formats::{AuthSpec, HttpBody};

    fn sample() -> HttpRequestConfig {
        let mut config = HttpRequestConfig::default();
        config.method = "POST".into();
        config.url = "https://example.com/items".into();
        config.body = HttpBody {
            kind: "json".into(),
            content: r#"{"name":"thread"}"#.into(),
            ..HttpBody::default()
        };
        config.auth = AuthSpec {
            kind: "bearer".into(),
            token: Some("secret".into()),
            ..AuthSpec::default()
        };
        config
    }

    #[test]
    fn javascript_includes_method_url_and_bearer() {
        let code = render(&sample(), SnippetLanguage::JavaScript);
        assert!(code.contains("https://example.com/items"));
        assert!(code.contains("POST"));
        assert!(code.contains("Bearer secret"));
        assert!(code.contains("application/json"));
    }

    #[test]
    fn python_and_go_quote_the_json_body() {
        let config = sample();
        let python = render(&config, SnippetLanguage::Python);
        let go = render(&config, SnippetLanguage::Go);
        assert!(python.contains(r#"{\"name\":\"thread\"}"#));
        assert!(go.contains("strings.NewReader"));
        assert!(go.contains("Bearer secret"));
    }
}
