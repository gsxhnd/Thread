//! Postman Collection v2.1 import/export mapping for workspace HTTP collections.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::db::new_id;
use crate::error::{AppError, AppResult};

use super::files;
use super::formats::{
    AuthSpec, CollectionMeta, HttpBody, HttpBodyField, HttpRequestDoc, KeyValue, RequestFile,
};
use super::index::{CollectionNode, WorkspaceIndex, scan};
use super::layout;

const POSTMAN_SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";
pub const MAX_COLLECTION_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct PostmanCollection {
    pub name: String,
    pub description: Option<String>,
    pub auth: AuthSpec,
    pub headers: Vec<KeyValue>,
    pub items: Vec<PostmanItem>,
}

#[derive(Debug, Clone)]
pub struct PostmanFolder {
    pub name: String,
    pub description: Option<String>,
    pub auth: AuthSpec,
    pub headers: Vec<KeyValue>,
    pub items: Vec<PostmanItem>,
}

#[derive(Debug, Clone)]
pub enum PostmanItem {
    Folder(Box<PostmanFolder>),
    Request(Box<HttpRequestDoc>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub collection_path: String,
    pub request_count: usize,
    pub redacted_values: usize,
}

/// Parse and validate a Postman Collection v2.1 document without executing scripts.
pub fn parse_collection(source: &str) -> AppResult<PostmanCollection> {
    if source.len() > MAX_COLLECTION_BYTES {
        return Err(AppError::Validation(format!(
            "Postman collection exceeds the {} MiB import limit",
            MAX_COLLECTION_BYTES / (1024 * 1024)
        )));
    }
    let root = serde_json::from_str::<Value>(source)
        .map_err(|error| AppError::Validation(format!("invalid Postman JSON: {error}")))?;
    let object = root
        .as_object()
        .ok_or_else(|| AppError::Validation("Postman collection must be a JSON object".into()))?;
    let info = object
        .get("info")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Validation("Postman collection is missing info".into()))?;
    let name = info
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::Validation("Postman collection info.name is required".into()))?;
    if let Some(schema) = info.get("schema").and_then(Value::as_str)
        && !schema.contains("collection/v2.1")
    {
        return Err(AppError::Validation(format!(
            "unsupported Postman collection schema: {schema}"
        )));
    }
    let items = parse_items(
        object
            .get("item")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    )?;
    Ok(PostmanCollection {
        name: name.to_string(),
        description: description_text(info.get("description")),
        auth: parse_auth(object.get("auth"))?.unwrap_or_default(),
        headers: parse_pairs(object.get("header"))?,
        items,
    })
}

/// Create an imported collection in the workspace using a staging directory and atomic rename.
/// Invalid input is fully parsed before any workspace files are written.
pub fn import_collection(
    root: &Path,
    parent_rel: Option<&str>,
    source: &str,
) -> AppResult<ImportReport> {
    let mut collection = parse_collection(source)?;
    crate::db::require_non_empty("collection name", &collection.name)?;
    let redacted_values = redact_collection_secrets(&mut collection);
    let parent = match parent_rel.filter(|rel| !rel.trim().is_empty()) {
        Some(rel) => layout::resolve_inside(root, rel)?,
        None => root.to_path_buf(),
    };
    if !parent.is_dir() {
        return Err(AppError::NotFound(format!(
            "parent collection does not exist: {}",
            parent.display()
        )));
    }

    let final_dir = layout::unique_path(&parent, &layout::slugify(&collection.name), "");
    let stage_dir = parent.join(format!(".thread-postman-import-{}", new_id()));
    fs::create_dir(&stage_dir)?;
    let final_rel = layout::to_relative_string(root, &final_dir);
    let staged = write_collection_dir(&stage_dir, Path::new(&final_rel), &collection);
    let request_count = match staged {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&stage_dir, &final_dir) {
        let _ = fs::remove_dir_all(&stage_dir);
        return Err(AppError::Io(error.to_string()));
    }
    Ok(ImportReport {
        collection_path: layout::to_relative_string(root, &final_dir),
        request_count,
        redacted_values,
    })
}

/// Export one workspace collection (including nested folders) as Postman v2.1 JSON.
pub fn export_collection(root: &Path, collection_rel: &str) -> AppResult<String> {
    let scanned = scan(root, None)?;
    let node = find_collection(&scanned.index, collection_rel)
        .ok_or_else(|| AppError::NotFound(format!("collection not found: {collection_rel}")))?;
    let item = export_folder(root, node, &[])?;
    let PostmanItem::Folder(folder) = item else {
        unreachable!("export_folder always returns a folder")
    };
    let PostmanFolder {
        name,
        description,
        auth,
        items,
        ..
    } = *folder;

    let mut info = Map::new();
    info.insert("name".into(), Value::String(name));
    info.insert("schema".into(), Value::String(POSTMAN_SCHEMA.into()));
    if let Some(description) = description {
        info.insert("description".into(), Value::String(description));
    }
    if let Ok(dir) = layout::resolve_inside(root, collection_rel) {
        let meta_path = dir.join(layout::COLLECTION_FILE);
        if meta_path.is_file()
            && let Ok(meta) = files::read_json::<CollectionMeta>(&meta_path)
            && let Some(id) = meta.id
        {
            info.insert("_postman_id".into(), Value::String(id));
        }
    }
    let mut exported = json!({
        "info": Value::Object(info),
        "item": items.iter().map(item_to_value).collect::<Vec<_>>(),
    });
    if let Some(auth) = auth_to_value(&auth) {
        exported["auth"] = auth;
    }
    let mut text = serde_json::to_string_pretty(&exported)?;
    text.push('\n');
    Ok(text)
}

fn parse_items(items: &[Value]) -> AppResult<Vec<PostmanItem>> {
    items.iter().map(parse_item).collect()
}

fn parse_item(item: &Value) -> AppResult<PostmanItem> {
    let object = item
        .as_object()
        .ok_or_else(|| AppError::Validation("Postman item must be an object".into()))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::Validation("Postman item name is required".into()))?;
    if let Some(children) = object.get("item") {
        let children = children.as_array().ok_or_else(|| {
            AppError::Validation(format!(
                "Postman folder {name:?} has a non-array item field"
            ))
        })?;
        return Ok(PostmanItem::Folder(Box::new(PostmanFolder {
            name: name.to_string(),
            description: description_text(object.get("description")),
            auth: parse_auth(object.get("auth"))?.unwrap_or_default(),
            headers: parse_pairs(object.get("header"))?,
            items: parse_items(children)?,
        })));
    }
    let request = object.get("request").ok_or_else(|| {
        AppError::Validation(format!("Postman item {name:?} has no request or item"))
    })?;
    Ok(PostmanItem::Request(Box::new(parse_request(
        name, request,
    )?)))
}

fn parse_request(name: &str, request: &Value) -> AppResult<HttpRequestDoc> {
    let object = request.as_object().ok_or_else(|| {
        AppError::Validation(format!("Postman request {name:?} must be an object"))
    })?;
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let (url, query_params) = parse_url(object.get("url"))?;
    let headers = parse_pairs(object.get("header"))?;
    let auth = parse_auth(object.get("auth"))?.unwrap_or_default();
    let body = parse_body(object.get("body"), &headers)?;
    Ok(HttpRequestDoc {
        version: layout::FILE_VERSION,
        id: None,
        name: name.to_string(),
        method,
        url,
        query_params,
        headers,
        body,
        auth,
        settings: Default::default(),
        pre_script: String::new(),
        post_script: String::new(),
    })
}

fn parse_url(url: Option<&Value>) -> AppResult<(String, Vec<KeyValue>)> {
    let Some(url) = url else {
        return Err(AppError::Validation(
            "Postman request URL is required".into(),
        ));
    };
    if let Some(raw) = url.as_str() {
        return Ok((raw.to_string(), Vec::new()));
    }
    let object = url
        .as_object()
        .ok_or_else(|| AppError::Validation("Postman URL must be a string or object".into()))?;
    let query = parse_pairs(object.get("query"))?;
    let raw = object
        .get("raw")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| reconstruct_url(object));
    let base = if query.is_empty() {
        raw
    } else {
        strip_query(&raw)
    };
    Ok((base, query))
}

fn reconstruct_url(object: &Map<String, Value>) -> String {
    let protocol = object
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("https");
    let host = join_segments(object.get("host"), ".");
    let port = object
        .get("port")
        .map(value_as_string)
        .filter(|port| !port.is_empty());
    let authority = port.map_or(host.clone(), |port| format!("{host}:{port}"));
    let path = join_segments(object.get("path"), "/");
    let mut url = format!("{protocol}://{authority}");
    if !path.is_empty() {
        url.push('/');
        url.push_str(&path);
    }
    url
}

fn join_segments(value: Option<&Value>, separator: &str) -> String {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(separator),
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn strip_query(raw: &str) -> String {
    let fragment_at = raw.find('#').unwrap_or(raw.len());
    let (before_fragment, fragment) = raw.split_at(fragment_at);
    match before_fragment.find('?') {
        Some(query_at) => format!("{}{}", &before_fragment[..query_at], fragment),
        None => raw.to_string(),
    }
}

fn parse_body(body: Option<&Value>, headers: &[KeyValue]) -> AppResult<HttpBody> {
    let Some(body) = body else {
        return Ok(HttpBody::default());
    };
    let object = body
        .as_object()
        .ok_or_else(|| AppError::Validation("Postman body must be an object".into()))?;
    let mode = object.get("mode").and_then(Value::as_str).unwrap_or("raw");
    match mode {
        "raw" => {
            let content = object.get("raw").and_then(Value::as_str).unwrap_or_default();
            let language = object
                .get("options")
                .and_then(|options| options.pointer("/raw/language"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let is_json = language.eq_ignore_ascii_case("json")
                || headers.iter().any(|header| {
                    header.key.eq_ignore_ascii_case("content-type")
                        && header.value.to_ascii_lowercase().contains("json")
                });
            Ok(HttpBody {
                kind: if is_json { "json" } else { "raw" }.into(),
                content: content.to_string(),
                ..HttpBody::default()
            })
        }
        "urlencoded" | "formdata" => {
            let is_form_data = mode == "formdata";
            let values = object
                .get(if is_form_data { "formdata" } else { "urlencoded" })
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::Validation(format!("Postman {mode} body is not an array")))?;
            let fields = values
                .iter()
                .map(|entry| {
                    let entry = entry.as_object().ok_or_else(|| {
                        AppError::Validation(format!("Postman {mode} field must be an object"))
                    })?;
                    let kind = entry.get("type").and_then(Value::as_str).unwrap_or("text");
                    if is_form_data && kind == "file" {
                        return Err(AppError::Validation(
                            "Postman file fields are not imported. Remove them or reselect files manually after importing a text-only collection.".into(),
                        ));
                    }
                    Ok(HttpBodyField {
                        key: entry.get("key").and_then(Value::as_str).unwrap_or_default().into(),
                        value: entry.get("value").and_then(Value::as_str).unwrap_or_default().into(),
                        kind: "text".into(),
                        enabled: !entry.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                        file_path: None,
                        content_type: entry.get("contentType").and_then(Value::as_str).map(str::to_string),
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            Ok(HttpBody {
                kind: if is_form_data { "multipart" } else { "formUrlencoded" }.into(),
                fields,
                ..HttpBody::default()
            })
        }
        "graphql" => {
            let graphql = object.get("graphql").and_then(Value::as_object).ok_or_else(|| {
                AppError::Validation("Postman GraphQL body is missing graphql fields".into())
            })?;
            let body = json!({
                "query": graphql.get("query").and_then(Value::as_str).unwrap_or_default(),
                "variables": graphql.get("variables").cloned().unwrap_or_else(|| json!({})),
            });
            Ok(HttpBody {
                kind: "json".into(),
                content: serde_json::to_string_pretty(&body)?,
                ..HttpBody::default()
            })
        }
        "file" => Err(AppError::Validation(
            "Postman file bodies are not imported. Remove the file body or import it manually in the request editor.".into(),
        )),
        other => Err(AppError::Validation(format!("unsupported Postman body mode: {other}"))),
    }
}

fn parse_pairs(values: Option<&Value>) -> AppResult<Vec<KeyValue>> {
    let Some(values) = values else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| AppError::Validation("Postman key/value field must be an array".into()))?;
    values
        .iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(|| {
                AppError::Validation("Postman key/value entry must be an object".into())
            })?;
            Ok(KeyValue {
                key: object
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                value: object
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                enabled: !object
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                description: description_text(object.get("description")),
            })
        })
        .collect()
}

fn parse_auth(auth: Option<&Value>) -> AppResult<Option<AuthSpec>> {
    let Some(auth) = auth else {
        return Ok(None);
    };
    let object = auth
        .as_object()
        .ok_or_else(|| AppError::Validation("Postman auth must be an object".into()))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("inherit");
    let fields = object
        .get(kind)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.get("key").and_then(Value::as_str) == Some(name))
            .and_then(|field| field.get("value"))
            .map(value_as_string)
    };
    let auth = match kind {
        "noauth" => AuthSpec {
            kind: "none".into(),
            ..AuthSpec::default()
        },
        "inherit" => AuthSpec::default(),
        "bearer" => AuthSpec {
            kind: "bearer".into(),
            token: field("token"),
            ..AuthSpec::default()
        },
        "basic" => AuthSpec {
            kind: "basic".into(),
            username: field("username"),
            password: field("password"),
            ..AuthSpec::default()
        },
        "apikey" => AuthSpec {
            kind: "apiKey".into(),
            key: field("key"),
            value: field("value"),
            location: field("in"),
            ..AuthSpec::default()
        },
        other => {
            return Err(AppError::Validation(format!(
                "unsupported Postman auth type: {other}"
            )));
        }
    };
    Ok(Some(auth))
}

fn value_as_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn description_text(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| {
        value.as_str().map(str::to_string).or_else(|| {
            value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    })
}

fn redact_collection_secrets(collection: &mut PostmanCollection) -> usize {
    let mut redacted = redact_auth(&mut collection.auth);
    redacted += redact_headers(&mut collection.headers);
    for item in &mut collection.items {
        redacted += redact_item(item);
    }
    redacted
}

fn redact_item(item: &mut PostmanItem) -> usize {
    match item {
        PostmanItem::Folder(folder) => {
            let mut redacted = redact_auth(&mut folder.auth) + redact_headers(&mut folder.headers);
            for item in &mut folder.items {
                redacted += redact_item(item);
            }
            redacted
        }
        PostmanItem::Request(document) => {
            let mut redacted = redact_auth(&mut document.auth);
            redacted += redact_headers(&mut document.headers);
            for pair in &mut document.query_params {
                if is_sensitive_name(&pair.key) && !pair.value.is_empty() {
                    pair.value.clear();
                    redacted += 1;
                }
            }
            let (safe_url, url_redactions) = redact_url(&document.url);
            document.url = safe_url;
            redacted += url_redactions;
            for field in &mut document.body.fields {
                if is_sensitive_name(&field.key) && !field.value.is_empty() {
                    field.value.clear();
                    redacted += 1;
                }
            }
            if matches!(document.body.kind.as_str(), "json" | "raw")
                && let Ok(mut value) = serde_json::from_str::<Value>(&document.body.content)
            {
                let count = redact_json_value(&mut value);
                if count > 0 {
                    document.body.content = serde_json::to_string_pretty(&value)
                        .unwrap_or_else(|_| document.body.content.clone());
                    redacted += count;
                }
            }
            redacted
        }
    }
}

fn redact_auth(auth: &mut AuthSpec) -> usize {
    let mut redacted = 0;
    for secret in [&mut auth.token, &mut auth.password, &mut auth.value] {
        if secret.as_ref().is_some_and(|value| !value.is_empty()) {
            *secret = Some(String::new());
            redacted += 1;
        }
    }
    redacted
}

fn redact_headers(headers: &mut [KeyValue]) -> usize {
    let mut redacted = 0;
    for header in headers {
        if is_sensitive_name(&header.key) && !header.value.is_empty() {
            header.value.clear();
            redacted += 1;
        }
    }
    redacted
}

fn is_sensitive_name(name: &str) -> bool {
    let normalized = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "apikey",
        "credential",
        "signature",
        "privatekey",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn redact_url(url: &str) -> (String, usize) {
    let fragment_start = url.find('#').unwrap_or(url.len());
    let Some(query_start) = url[..fragment_start].find('?') else {
        return (url.to_string(), 0);
    };
    let query = &url[query_start + 1..fragment_start];
    let mut count = 0;
    let query = query
        .split('&')
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            if is_sensitive_name(&decode_query_key(key)) && !value.is_empty() {
                count += 1;
                format!("{key}=")
            } else {
                pair.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    let mut safe = String::with_capacity(url.len());
    safe.push_str(&url[..query_start + 1]);
    safe.push_str(&query);
    safe.push_str(&url[fragment_start..]);
    (safe, count)
}

fn decode_query_key(key: &str) -> String {
    let bytes = key.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(if bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn redact_json_value(value: &mut Value) -> usize {
    match value {
        Value::Object(object) => {
            let mut redacted = 0;
            for (key, value) in object.iter_mut() {
                if is_sensitive_name(key) {
                    if !value.is_null() && value.as_str() != Some("") {
                        *value = Value::String(String::new());
                        redacted += 1;
                    }
                } else {
                    redacted += redact_json_value(value);
                }
            }
            redacted
        }
        Value::Array(values) => values.iter_mut().map(redact_json_value).sum(),
        _ => 0,
    }
}

fn write_collection_dir(
    actual_dir: &Path,
    logical_rel: &Path,
    collection: &PostmanCollection,
) -> AppResult<usize> {
    let mut direct_order = Vec::new();
    let mut count = 0;
    for item in &collection.items {
        match item {
            PostmanItem::Folder(folder) => {
                let folder_dir =
                    layout::unique_path(actual_dir, &layout::slugify(&folder.name), "");
                fs::create_dir(&folder_dir)?;
                let folder_rel = logical_rel.join(folder_dir.file_name().unwrap_or_default());
                let child_collection = PostmanCollection {
                    name: folder.name.clone(),
                    description: folder.description.clone(),
                    auth: folder.auth.clone(),
                    headers: folder.headers.clone(),
                    items: folder.items.clone(),
                };
                count += write_collection_dir(&folder_dir, &folder_rel, &child_collection)?;
            }
            PostmanItem::Request(document) => {
                let suffix = layout::file_suffix_for_protocol("http")?;
                let path =
                    layout::unique_path(actual_dir, &layout::slugify(&document.name), suffix);
                let mut doc = (**document).clone();
                doc.version = layout::FILE_VERSION;
                doc.id = Some(new_id());
                files::write_json(&path, &RequestFile::Http(doc))?;
                let relative = logical_rel.join(path.file_name().unwrap_or_default());
                direct_order.push(relative.to_string_lossy().replace('\\', "/"));
                count += 1;
            }
        }
    }
    let mut meta = CollectionMeta::new(&collection.name);
    meta.id = Some(new_id());
    meta.description = collection.description.clone();
    meta.auth = collection.auth.clone();
    meta.headers = collection.headers.clone();
    meta.request_order = direct_order;
    files::write_json(&actual_dir.join(layout::COLLECTION_FILE), &meta)?;
    Ok(count)
}

fn export_folder(
    root: &Path,
    node: &CollectionNode,
    inherited_headers: &[KeyValue],
) -> AppResult<PostmanItem> {
    let dir = layout::resolve_inside(root, &node.path)?;
    let meta_path = dir.join(layout::COLLECTION_FILE);
    let meta = if meta_path.is_file() {
        files::read_json::<CollectionMeta>(&meta_path)?
    } else {
        CollectionMeta::new(&node.name)
    };
    let effective_headers = merge_headers(inherited_headers, &meta.headers);
    let mut items = Vec::new();
    for request in &node.requests {
        let file = files::read_relative_json::<RequestFile>(root, &request.path)?;
        let RequestFile::Http(document) = file else {
            return Err(AppError::Validation(format!(
                "Postman export supports HTTP requests only: {}",
                request.path
            )));
        };
        items.push(PostmanItem::Request(Box::new(with_inherited_headers(
            document,
            &effective_headers,
        ))));
    }
    for child in &node.children {
        items.push(export_folder(root, child, &effective_headers)?);
    }
    Ok(PostmanItem::Folder(Box::new(PostmanFolder {
        name: meta.name,
        description: meta.description,
        auth: meta.auth,
        headers: meta.headers,
        items,
    })))
}

fn merge_headers(inherited: &[KeyValue], local: &[KeyValue]) -> Vec<KeyValue> {
    let mut merged: Vec<KeyValue> = inherited
        .iter()
        .filter(|header| {
            !local
                .iter()
                .any(|candidate| candidate.key.eq_ignore_ascii_case(&header.key))
        })
        .cloned()
        .collect();
    merged.extend(local.iter().cloned());
    merged
}

fn with_inherited_headers(mut document: HttpRequestDoc, inherited: &[KeyValue]) -> HttpRequestDoc {
    document.headers = merge_headers(inherited, &document.headers);
    document
}

fn find_collection<'a>(index: &'a WorkspaceIndex, rel: &str) -> Option<&'a CollectionNode> {
    fn walk<'a>(nodes: &'a [CollectionNode], rel: &str) -> Option<&'a CollectionNode> {
        nodes.iter().find_map(|node| {
            if node.path == rel {
                Some(node)
            } else {
                walk(&node.children, rel)
            }
        })
    }
    walk(&index.collections, rel)
}

fn item_to_value(item: &PostmanItem) -> Value {
    match item {
        PostmanItem::Folder(folder) => {
            let mut value = json!({ "name": folder.name, "item": folder.items.iter().map(item_to_value).collect::<Vec<_>>() });
            if let Some(description) = &folder.description {
                value["description"] = Value::String(description.clone());
            }
            if let Some(auth) = auth_to_value(&folder.auth) {
                value["auth"] = auth;
            }
            value
        }
        PostmanItem::Request(document) => request_to_value(document),
    }
}

fn request_to_value(document: &HttpRequestDoc) -> Value {
    let mut url = document.url.clone();
    let query = document
        .query_params
        .iter()
        .map(|pair| {
            json!({
                "key": pair.key,
                "value": pair.value,
                "disabled": !pair.enabled,
            })
        })
        .collect::<Vec<_>>();
    if !query.is_empty() {
        let joined = document
            .query_params
            .iter()
            .map(|pair| {
                format!(
                    "{}={}",
                    encode_component(&pair.key),
                    encode_component(&pair.value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let separator = if url.contains('?') { '&' } else { '?' };
        url.push(separator);
        url.push_str(&joined);
    }
    let headers = document
        .headers
        .iter()
        .map(|header| {
            json!({
                "key": header.key,
                "value": header.value,
                "disabled": !header.enabled,
                "description": header.description,
            })
        })
        .collect::<Vec<_>>();
    let mut request = json!({
        "method": document.method,
        "header": headers,
        "url": { "raw": url, "query": query },
    });
    if let Some(auth) = auth_to_value(&document.auth) {
        request["auth"] = auth;
    }
    match document.body.kind.as_str() {
        "json" | "raw" if !document.body.content.is_empty() => {
            let language = if document.body.kind == "json" {
                "json"
            } else {
                "text"
            };
            request["body"] = json!({
                "mode": "raw",
                "raw": document.body.content,
                "options": { "raw": { "language": language } },
            });
        }
        "formUrlencoded" | "form" => {
            request["body"] = json!({
                "mode": "urlencoded",
                "urlencoded": document.body.fields.iter().map(field_to_value).collect::<Vec<_>>(),
            });
        }
        "multipart" | "form-data" => {
            request["body"] = json!({
                "mode": "formdata",
                "formdata": document.body.fields.iter().map(field_to_value).collect::<Vec<_>>(),
            });
        }
        _ => {}
    }
    json!({ "name": document.name, "request": request })
}

fn field_to_value(field: &HttpBodyField) -> Value {
    let mut value = json!({
        "key": field.key,
        "value": field.value,
        "disabled": !field.enabled,
        "type": field.kind,
    });
    if let Some(path) = &field.file_path {
        value["src"] = Value::String(path.clone());
        value["type"] = Value::String("file".into());
    }
    if let Some(content_type) = &field.content_type {
        value["contentType"] = Value::String(content_type.clone());
    }
    value
}

fn auth_to_value(auth: &AuthSpec) -> Option<Value> {
    match auth.kind.as_str() {
        "inherit" => None,
        "none" => Some(json!({ "type": "noauth" })),
        "bearer" => Some(json!({
            "type": "bearer",
            "bearer": [{ "key": "token", "value": auth.token.as_deref().unwrap_or_default(), "type": "string" }],
        })),
        "basic" => Some(json!({
            "type": "basic",
            "basic": [
                { "key": "username", "value": auth.username.as_deref().unwrap_or_default(), "type": "string" },
                { "key": "password", "value": auth.password.as_deref().unwrap_or_default(), "type": "string" },
            ],
        })),
        "apiKey" => Some(json!({
            "type": "apikey",
            "apikey": [
                { "key": "key", "value": auth.key.as_deref().unwrap_or_default(), "type": "string" },
                { "key": "value", "value": auth.value.as_deref().unwrap_or_default(), "type": "string" },
                { "key": "in", "value": auth.location.as_deref().unwrap_or("header"), "type": "string" },
            ],
        })),
        _ => None,
    }
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Serialize an in-memory collection to the Postman Collection v2.1 shape.
pub fn serialize_collection(collection: &PostmanCollection) -> AppResult<String> {
    let mut info = json!({ "name": collection.name, "schema": POSTMAN_SCHEMA });
    if let Some(description) = &collection.description {
        info["description"] = Value::String(description.clone());
    }
    let mut value = json!({
        "info": info,
        "item": collection.items.iter().map(item_to_value).collect::<Vec<_>>(),
    });
    if let Some(auth) = auth_to_value(&collection.auth) {
        value["auth"] = auth;
    }
    let mut text = serde_json::to_string_pretty(&value)?;
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "info": { "name": "Service", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
      "item": [
        { "name": "Users", "item": [
          { "name": "List", "request": {
            "method": "GET",
            "header": [{ "key": "X-Test", "value": "yes" }],
            "url": { "raw": "https://api.example.test/users?active=true&skip=x", "query": [{ "key": "active", "value": "true" }, { "key": "skip", "value": "x", "disabled": true }] }
          } },
          { "name": "Create", "request": {
            "method": "POST",
            "header": [{ "key": "Content-Type", "value": "application/json" }],
            "body": { "mode": "raw", "raw": "{\"name\":\"Ada\"}", "options": { "raw": { "language": "json" } } },
            "url": "https://api.example.test/users"
          } }
        ] }
      ]
    }"#;

    #[test]
    fn parses_nested_collection_and_http_requests() {
        let collection = parse_collection(SAMPLE).unwrap();
        assert_eq!(collection.name, "Service");
        let PostmanItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.name, "Users");
        let items = &folder.items;
        let PostmanItem::Request(list) = &items[0] else {
            panic!("expected request");
        };
        assert_eq!(list.url, "https://api.example.test/users");
        assert_eq!(list.query_params[0].key, "active");
        assert!(!list.query_params[1].enabled);
    }

    #[test]
    fn export_import_round_trip_preserves_http_fields() {
        let collection = parse_collection(SAMPLE).unwrap();
        let text = serialize_collection(&collection).unwrap();
        let restored = parse_collection(&text).unwrap();
        assert_eq!(serialize_collection(&restored).unwrap(), text);
    }

    #[test]
    fn imports_then_exports_nested_collection_from_workspace() {
        let root = std::env::temp_dir().join(format!("thread-postman-{}", new_id()));
        fs::create_dir_all(&root).unwrap();
        let report = import_collection(&root, None, SAMPLE).unwrap();
        assert_eq!(report.request_count, 2);
        let exported = export_collection(&root, &report.collection_path).unwrap();
        let collection = parse_collection(&exported).unwrap();
        assert_eq!(collection.name, "Service");
        let PostmanItem::Folder(folder) = &collection.items[0] else {
            panic!("expected nested folder");
        };
        assert_eq!(folder.items.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn strips_known_secrets_before_workspace_import() {
        let source = r#"{
          "info":{"name":"Private","schema":"collection/v2.1.0"},
          "item":[{"name":"Call","request":{
            "method":"POST",
            "auth":{"type":"bearer","bearer":[{"key":"token","value":"token-value"}]},
            "header":[{"key":"Authorization","value":"Bearer header-token"}],
            "url":{"raw":"https://example.test?api_key=key-value","query":[{"key":"api_key","value":"key-value"}]},
            "body":{"mode":"raw","raw":"{\"password\":\"body-secret\",\"name\":\"Ada\"}","options":{"raw":{"language":"json"}}}
          }}]
        }"#;
        let mut collection = parse_collection(source).unwrap();
        assert_eq!(redact_collection_secrets(&mut collection), 4);
        let PostmanItem::Request(request) = &collection.items[0] else {
            panic!("expected request");
        };
        assert_eq!(request.auth.token.as_deref(), Some(""));
        assert_eq!(request.headers[0].value, "");
        assert_eq!(request.query_params[0].value, "");
        assert!(request.body.content.contains("\"password\": \"\""));
    }

    #[test]
    fn rejects_file_uploads_and_non_v21_schemas() {
        let file_body = r#"{"info":{"name":"X","schema":"collection/v2.1.0"},"item":[{"name":"upload","request":{"method":"POST","url":"https://x","body":{"mode":"formdata","formdata":[{"key":"f","type":"file","src":"/secret"}]}}}]}"#;
        assert!(parse_collection(file_body).is_err());
        let old_schema = r#"{"info":{"name":"X","schema":"collection/v2.0.0"},"item":[]}"#;
        assert!(parse_collection(old_schema).is_err());
    }
}
