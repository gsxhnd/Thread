//! OpenAPI 3.x import/export for HTTP request collections.

use std::path::Path;

use serde_json::{Map, Number, Value, json};

use crate::error::{AppError, AppResult};

use super::formats::{AuthSpec, HttpBody, HttpBodyField, HttpRequestDoc, KeyValue};
use super::postman::{self, PostmanCollection, PostmanFolder, PostmanItem};

pub const MAX_OPENAPI_BYTES: usize = 32 * 1024 * 1024;
const HTTP_METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];
type ParsedHttpUrl = (String, String, Vec<(String, String)>);

/// Parses JSON or YAML OpenAPI 3.x into the shared HTTP collection model.
pub fn parse_document(source: &str) -> AppResult<PostmanCollection> {
    if source.len() > MAX_OPENAPI_BYTES {
        return Err(AppError::Validation(
            "OpenAPI document exceeds the 32 MiB import limit".into(),
        ));
    }
    let document = match serde_json::from_str::<Value>(source) {
        Ok(value) => value,
        Err(json_error) => {
            let yaml = serde_yaml::from_str::<serde_yaml::Value>(source).map_err(|yaml_error| {
                AppError::Validation(format!(
                    "invalid OpenAPI JSON/YAML: {json_error}; {yaml_error}"
                ))
            })?;
            serde_json::to_value(yaml).map_err(|error| {
                AppError::Validation(format!("invalid OpenAPI YAML value: {error}"))
            })?
        }
    };
    convert_document(&document)
}

/// Imports JSON or YAML OpenAPI 3.x as a new workspace collection.
pub fn import_collection(
    root: &Path,
    parent_rel: Option<&str>,
    source: &str,
) -> AppResult<postman::ImportReport> {
    let collection = parse_document(source)?;
    let postman_json = postman::serialize_collection(&collection)?;
    postman::import_collection(root, parent_rel, &postman_json)
}

/// Exports a workspace HTTP collection as OpenAPI 3.0 JSON.
pub fn export_collection(root: &Path, collection_rel: &str) -> AppResult<String> {
    let postman_json = postman::export_collection(root, collection_rel)?;
    let collection = postman::parse_collection(&postman_json)?;
    let document = collection_to_document(&collection)?;
    let mut output = serde_json::to_string_pretty(&document)?;
    output.push('\n');
    Ok(output)
}

fn convert_document(document: &Value) -> AppResult<PostmanCollection> {
    let root = document
        .as_object()
        .ok_or_else(|| AppError::Validation("OpenAPI document must be an object".into()))?;
    let version = root
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !version.starts_with("3.") {
        return Err(AppError::Validation(format!(
            "expected OpenAPI 3.x, got {version:?}"
        )));
    }
    let info = root
        .get("info")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Validation("OpenAPI document is missing info".into()))?;
    let title = info
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| AppError::Validation("OpenAPI info.title is required".into()))?;
    let components = root.get("components").and_then(Value::as_object);
    let global_auth = security_auth(document, root.get("security"), components)?;
    let global_servers = server_url(document, root.get("servers"))?;
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Validation("OpenAPI paths must be an object".into()))?;

    let mut untagged = Vec::new();
    let mut groups: Vec<(String, Vec<PostmanItem>)> = Vec::new();
    for (path, raw_path_item) in paths {
        let path_item = resolve(document, raw_path_item)?;
        let path_object = path_item.as_object().ok_or_else(|| {
            AppError::Validation(format!("OpenAPI path item {path:?} must be an object"))
        })?;
        let path_parameters = path_object
            .get("parameters")
            .map(|value| parse_parameters(document, value))
            .transpose()?
            .unwrap_or_default();
        for method in HTTP_METHODS {
            let Some(raw_operation) = path_object.get(method) else {
                continue;
            };
            let operation = resolve(document, raw_operation)?;
            let operation_object = operation.as_object().ok_or_else(|| {
                AppError::Validation(format!(
                    "OpenAPI operation {method} {path} must be an object"
                ))
            })?;
            let operation_parameters = operation_object
                .get("parameters")
                .map(|value| parse_parameters(document, value))
                .transpose()?
                .unwrap_or_default();
            let parameters = merge_parameters(path_parameters.clone(), operation_parameters);
            let server = server_url(
                document,
                operation_object
                    .get("servers")
                    .or(path_object.get("servers")),
            )?
            .unwrap_or_else(|| {
                global_servers
                    .clone()
                    .unwrap_or_else(|| "https://example.com".into())
            });
            let mut url = join_server_path(&server, path);
            let mut query_params = Vec::new();
            let mut headers = Vec::new();
            let mut cookies = Vec::new();
            for parameter in parameters {
                match parameter.location.as_str() {
                    "path" => {
                        let placeholder = format!("{{{}}}", parameter.name);
                        url = url.replace(&placeholder, &format!("{{{{{}}}}}", parameter.name));
                    }
                    "query" => query_params.push(KeyValue {
                        key: parameter.name,
                        value: parameter.value,
                        enabled: true,
                        description: parameter.description,
                    }),
                    "header" => headers.push(KeyValue {
                        key: parameter.name,
                        value: parameter.value,
                        enabled: true,
                        description: parameter.description,
                    }),
                    "cookie" => cookies.push(format!("{}={}", parameter.name, parameter.value)),
                    _ => {}
                }
            }
            if !cookies.is_empty() {
                headers.push(KeyValue::new("Cookie", &cookies.join("; ")));
            }
            let security = operation_object.get("security").or(root.get("security"));
            let auth = security_auth(document, security, components)?.unwrap_or_default();
            let body =
                parse_request_body(document, operation_object.get("requestBody"), &mut headers)?;
            let name = operation_object
                .get("operationId")
                .and_then(Value::as_str)
                .or_else(|| operation_object.get("summary").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{} {}", method.to_ascii_uppercase(), path));
            let request = HttpRequestDoc {
                version: super::layout::FILE_VERSION,
                id: None,
                name,
                method: method.to_ascii_uppercase(),
                url,
                query_params,
                headers,
                body,
                auth,
                settings: Default::default(),
                pre_script: String::new(),
                post_script: String::new(),
            };
            let item = PostmanItem::Request(Box::new(request));
            let tag = operation_object
                .get("tags")
                .and_then(Value::as_array)
                .and_then(|tags| tags.first())
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(tag) = tag {
                if let Some((_, items)) = groups.iter_mut().find(|(name, _)| name == &tag) {
                    items.push(item);
                } else {
                    groups.push((tag, vec![item]));
                }
            } else {
                untagged.push(item);
            }
        }
    }
    let mut items = untagged;
    items.extend(groups.into_iter().map(|(name, items)| {
        PostmanItem::Folder(Box::new(PostmanFolder {
            name,
            description: None,
            auth: AuthSpec::default(),
            headers: Vec::new(),
            items,
        }))
    }));
    Ok(PostmanCollection {
        name: title.to_string(),
        description: info
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        auth: global_auth.unwrap_or_default(),
        headers: Vec::new(),
        items,
    })
}

#[derive(Clone)]
struct Parameter {
    name: String,
    location: String,
    value: String,
    description: Option<String>,
}

fn parse_parameters(root: &Value, value: &Value) -> AppResult<Vec<Parameter>> {
    let values = value
        .as_array()
        .ok_or_else(|| AppError::Validation("OpenAPI parameters must be an array".into()))?;
    values
        .iter()
        .map(|raw| {
            let parameter = resolve(root, raw)?;
            let object = parameter.as_object().ok_or_else(|| {
                AppError::Validation("OpenAPI parameter must be an object".into())
            })?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| AppError::Validation("OpenAPI parameter name is required".into()))?;
            let location = object.get("in").and_then(Value::as_str).ok_or_else(|| {
                AppError::Validation(format!("OpenAPI parameter {name:?} is missing in"))
            })?;
            let schema = object
                .get("schema")
                .map(|schema| resolve(root, schema))
                .transpose()?;
            let value = object
                .get("example")
                .or_else(|| schema.and_then(|schema| schema.get("example")))
                .or_else(|| schema.and_then(|schema| schema.get("default")))
                .map(value_to_text)
                .unwrap_or_else(|| {
                    if object
                        .get("required")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        format!("{{{{{name}}}}}")
                    } else {
                        String::new()
                    }
                });
            Ok(Parameter {
                name: name.to_string(),
                location: location.to_string(),
                value,
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn merge_parameters(path: Vec<Parameter>, operation: Vec<Parameter>) -> Vec<Parameter> {
    let mut merged = path;
    for parameter in operation {
        merged.retain(|existing| {
            existing.name != parameter.name || existing.location != parameter.location
        });
        merged.push(parameter);
    }
    merged
}

fn parse_request_body(
    root: &Value,
    request_body: Option<&Value>,
    headers: &mut Vec<KeyValue>,
) -> AppResult<HttpBody> {
    let Some(raw) = request_body else {
        return Ok(HttpBody::default());
    };
    let body = resolve(root, raw)?;
    let content = body
        .get("content")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::Validation("OpenAPI requestBody.content must be an object".into())
        })?;
    let Some((media_type, media)) = choose_media(content) else {
        return Ok(HttpBody::default());
    };
    let example = media.get("example").cloned().or_else(|| {
        media
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|examples| examples.values().next())
            .and_then(|example| example.get("value"))
            .cloned()
    });
    let example = match (example, media.get("schema")) {
        (Some(example), _) => Some(example),
        (None, Some(schema)) => Some(schema_example(root, schema, 0)?),
        (None, None) => None,
    };
    let media_lower = media_type.to_ascii_lowercase();
    if media_lower == "application/json" || media_lower.ends_with("+json") {
        let content = example
            .map(|value| match value {
                Value::String(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(value) => serde_json::to_string_pretty(&value).unwrap_or(text),
                    Err(_) => text,
                },
                value => serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
            })
            .unwrap_or_else(|| "{}".into());
        if media_lower != "application/json" && !has_header(headers, "content-type") {
            headers.push(KeyValue::new("Content-Type", media_type));
        }
        return Ok(HttpBody {
            kind: "json".into(),
            content,
            ..HttpBody::default()
        });
    }
    if media_lower == "application/x-www-form-urlencoded" || media_lower == "multipart/form-data" {
        let formdata = media_lower == "multipart/form-data";
        let schema = media
            .get("schema")
            .map(|raw_schema| resolve(root, raw_schema))
            .transpose()?;
        let schema_properties = schema
            .and_then(|schema| schema.get("properties"))
            .and_then(Value::as_object);
        let values = if let Some(values) = example.as_ref().and_then(Value::as_object) {
            values.clone()
        } else {
            let mut values = Map::new();
            if let Some(properties) = schema_properties {
                for (key, property_schema) in properties {
                    values.insert(key.clone(), schema_example(root, property_schema, 0)?);
                }
            }
            values
        };
        let mut fields = Vec::new();
        for (key, value) in values {
            let property = schema_properties.and_then(|properties| properties.get(&key));
            if formdata
                && property
                    .and_then(|property| property.get("format"))
                    .and_then(Value::as_str)
                    == Some("binary")
            {
                return Err(AppError::Validation(format!(
                    "OpenAPI binary form field {key:?} requires manual file selection"
                )));
            }
            fields.push(HttpBodyField {
                key,
                value: value_to_text(&value),
                kind: "text".into(),
                enabled: true,
                file_path: None,
                content_type: None,
            });
        }
        return Ok(HttpBody {
            kind: if formdata {
                "multipart"
            } else {
                "formUrlencoded"
            }
            .into(),
            fields,
            ..HttpBody::default()
        });
    }
    if media_lower == "application/octet-stream" {
        return Err(AppError::Validation(
            "OpenAPI binary request bodies require manual file selection".into(),
        ));
    }
    let text = example
        .map(|value| value_to_text(&value))
        .unwrap_or_default();
    if !has_header(headers, "content-type") {
        headers.push(KeyValue::new("Content-Type", media_type));
    }
    Ok(HttpBody {
        kind: "raw".into(),
        content: text,
        ..HttpBody::default()
    })
}

fn choose_media(content: &Map<String, Value>) -> Option<(&str, &Value)> {
    if let Some(value) = content.get("application/json") {
        return Some(("application/json", value));
    }
    if let Some((media, value)) = content.iter().find(|(media, _)| media.ends_with("+json")) {
        return Some((media.as_str(), value));
    }
    content
        .iter()
        .next()
        .map(|(media, value)| (media.as_str(), value))
}

fn schema_example(root: &Value, raw_schema: &Value, depth: usize) -> AppResult<Value> {
    if depth >= 8 {
        return Ok(Value::Null);
    }
    let schema = resolve(root, raw_schema)?;
    for key in ["example", "default"] {
        if let Some(value) = schema.get(key) {
            return Ok(value.clone());
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && let Some(value) = values.first()
    {
        return Ok(value.clone());
    }
    let value = match schema.get("type").and_then(Value::as_str).unwrap_or("") {
        "object" => {
            let mut object = Map::new();
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, child) in properties {
                    object.insert(name.clone(), schema_example(root, child, depth + 1)?);
                }
            }
            Value::Object(object)
        }
        "array" => Value::Array(Vec::new()),
        "string" => Value::String(String::new()),
        "integer" => Value::Number(Number::from(0)),
        "number" => Number::from_f64(0.0)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "boolean" => Value::Bool(false),
        _ => Value::Null,
    };
    Ok(value)
}

fn value_to_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn has_header(headers: &[KeyValue], name: &str) -> bool {
    headers
        .iter()
        .any(|header| header.key.eq_ignore_ascii_case(name))
}

fn server_url(root: &Value, servers: Option<&Value>) -> AppResult<Option<String>> {
    let Some(servers) = servers else {
        return Ok(None);
    };
    let servers = servers
        .as_array()
        .ok_or_else(|| AppError::Validation("OpenAPI servers must be an array".into()))?;
    let Some(server) = servers.first() else {
        return Ok(None);
    };
    let server = resolve(root, server)?;
    let Some(mut url) = server
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Err(AppError::Validation(
            "OpenAPI server.url is required".into(),
        ));
    };
    if let Some(variables) = server.get("variables").and_then(Value::as_object) {
        for (name, definition) in variables {
            let replacement = definition
                .get("default")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{{{{{name}}}}}"));
            url = url.replace(&format!("{{{name}}}"), &replacement);
        }
    }
    url = convert_braces(&url);
    if !url.contains("://") && url.starts_with('/') {
        url = format!("https://example.com{url}");
    } else if !url.contains("://") {
        url = format!("https://example.com/{url}");
    }
    Ok(Some(url))
}

fn join_server_path(server: &str, path: &str) -> String {
    format!(
        "{}/{}",
        server.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn convert_braces(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            output.push('{');
            output.push_str(rest);
            return output;
        };
        let name = &rest[..end];
        if name.starts_with('{') {
            output.push('{');
            output.push_str(name);
            output.push('}');
        } else {
            output.push_str("{{");
            output.push_str(name);
            output.push_str("}}");
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

fn resolve<'a>(root: &'a Value, value: &'a Value) -> AppResult<&'a Value> {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return Ok(value);
    };
    let pointer = reference.strip_prefix('#').ok_or_else(|| {
        AppError::Validation(format!(
            "external OpenAPI reference is not supported: {reference}"
        ))
    })?;
    root.pointer(pointer)
        .ok_or_else(|| AppError::Validation(format!("unresolved OpenAPI reference: {reference}")))
}

fn security_auth(
    root: &Value,
    security: Option<&Value>,
    components: Option<&Map<String, Value>>,
) -> AppResult<Option<AuthSpec>> {
    let Some(security) = security else {
        return Ok(None);
    };
    let requirements = security
        .as_array()
        .ok_or_else(|| AppError::Validation("OpenAPI security must be an array".into()))?;
    if requirements.is_empty() {
        return Ok(Some(AuthSpec {
            kind: "none".into(),
            ..AuthSpec::default()
        }));
    }
    let requirement = requirements[0].as_object().ok_or_else(|| {
        AppError::Validation("OpenAPI security requirement must be an object".into())
    })?;
    let Some((name, _scopes)) = requirement.iter().next() else {
        return Ok(None);
    };
    let scheme = components
        .and_then(|components| components.get("securitySchemes"))
        .and_then(Value::as_object)
        .and_then(|schemes| schemes.get(name))
        .ok_or_else(|| {
            AppError::Validation(format!("OpenAPI security scheme {name:?} was not found"))
        })?;
    let scheme = resolve(root, scheme)?;
    let kind = scheme
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "http" => match scheme
            .get("scheme")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "bearer" => Ok(Some(AuthSpec {
                kind: "bearer".into(),
                ..AuthSpec::default()
            })),
            "basic" => Ok(Some(AuthSpec {
                kind: "basic".into(),
                ..AuthSpec::default()
            })),
            other => Err(AppError::Validation(format!(
                "unsupported OpenAPI HTTP auth scheme: {other}"
            ))),
        },
        "apiKey" => {
            let location = scheme.get("in").and_then(Value::as_str).unwrap_or_default();
            if location == "cookie" {
                return Err(AppError::Validation(
                    "OpenAPI cookie apiKey authentication is not supported yet".into(),
                ));
            }
            Ok(Some(AuthSpec {
                kind: "apiKey".into(),
                key: scheme
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                location: Some(
                    if location == "query" {
                        "query"
                    } else {
                        "header"
                    }
                    .into(),
                ),
                ..AuthSpec::default()
            }))
        }
        "oauth2" | "openIdConnect" => Ok(Some(AuthSpec {
            kind: "bearer".into(),
            ..AuthSpec::default()
        })),
        other => Err(AppError::Validation(format!(
            "unsupported OpenAPI security type: {other}"
        ))),
    }
}

fn collection_to_document(collection: &PostmanCollection) -> AppResult<Value> {
    let mut paths = Map::new();
    let mut schemes = Map::new();
    let global_security = add_security(&collection.auth, &mut schemes);
    let mut path_ops = Vec::new();
    collect_openapi_operations(
        &collection.items,
        &[],
        &collection.auth,
        &mut schemes,
        &mut path_ops,
    )?;
    for (path, method, operation) in path_ops {
        let path_item = paths
            .entry(path)
            .or_insert_with(|| Value::Object(Map::new()));
        let object = path_item.as_object_mut().expect("path item is an object");
        if object.insert(method.clone(), operation).is_some() {
            return Err(AppError::Validation(format!(
                "multiple collection requests map to the same OpenAPI operation: {method}"
            )));
        }
    }
    let mut components = Map::new();
    if !schemes.is_empty() {
        components.insert("securitySchemes".into(), Value::Object(schemes));
    }
    let mut info = json!({ "title": collection.name, "version": "1.0.0" });
    if let Some(description) = &collection.description {
        info["description"] = Value::String(description.clone());
    }
    let mut document = json!({
        "openapi": "3.0.3",
        "info": info,
        "paths": Value::Object(paths),
    });
    if !components.is_empty() {
        document["components"] = Value::Object(components);
    }
    if let Some(security) = global_security {
        document["security"] = security;
    }
    Ok(document)
}

fn collect_openapi_operations(
    items: &[PostmanItem],
    parent_tags: &[String],
    inherited_auth: &AuthSpec,
    schemes: &mut Map<String, Value>,
    output: &mut Vec<(String, String, Value)>,
) -> AppResult<()> {
    for item in items {
        match item {
            PostmanItem::Folder(folder) => {
                let mut tags = parent_tags.to_vec();
                tags.push(folder.name.clone());
                let auth = if folder.auth.kind == "inherit" {
                    inherited_auth
                } else {
                    &folder.auth
                };
                collect_openapi_operations(&folder.items, &tags, auth, schemes, output)?;
            }
            PostmanItem::Request(request) => {
                let (server, path, url_query) = split_http_url(&request.url)?;
                let mut parameters = Vec::new();
                for pair in &request.query_params {
                    parameters.push(json!({ "name": pair.key, "in": "query", "required": false, "schema": { "type": "string" }, "example": pair.value }));
                }
                for (key, value) in url_query {
                    parameters.push(json!({ "name": key, "in": "query", "required": false, "schema": { "type": "string" }, "example": value }));
                }
                for header in &request.headers {
                    if !header.key.eq_ignore_ascii_case("content-type")
                        && !header.key.eq_ignore_ascii_case("authorization")
                    {
                        parameters.push(json!({ "name": header.key, "in": "header", "required": false, "schema": { "type": "string" }, "example": header.value }));
                    }
                }
                for segment in path.split('/') {
                    if let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
                        && !name.contains('{')
                    {
                        parameters.push(json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } }));
                    }
                }
                let mut operation = json!({
                    "operationId": unique_operation_id(&request.name, output),
                    "responses": { "default": { "description": "Imported response" } },
                });
                if !parent_tags.is_empty() {
                    operation["tags"] = json!(parent_tags);
                }
                if !parameters.is_empty() {
                    operation["parameters"] = Value::Array(parameters);
                }
                operation["servers"] = json!([{ "url": server }]);
                if let Some(body) = http_body_to_openapi(request)? {
                    operation["requestBody"] = body;
                }
                let auth = if request.auth.kind == "inherit" {
                    inherited_auth
                } else {
                    &request.auth
                };
                if auth.kind == "none" {
                    operation["security"] = json!([]);
                } else if let Some(security) = add_security(auth, schemes) {
                    operation["security"] = security;
                }
                output.push((path, request.method.to_ascii_lowercase(), operation));
            }
        }
    }
    Ok(())
}

fn unique_operation_id(name: &str, existing: &[(String, String, Value)]) -> String {
    let base = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let base = base.trim_matches('_');
    let mut candidate = if base.is_empty() {
        "request".to_string()
    } else {
        base.into()
    };
    let mut suffix = 2;
    while existing
        .iter()
        .any(|(_, _, op)| op.get("operationId").and_then(Value::as_str) == Some(candidate.as_str()))
    {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

fn split_http_url(raw: &str) -> AppResult<ParsedHttpUrl> {
    let (without_fragment, _) = raw.split_once('#').unwrap_or((raw, ""));
    let (without_query, query) = without_fragment
        .split_once('?')
        .unwrap_or((without_fragment, ""));
    let Some(scheme_end) = without_query.find("://") else {
        return Err(AppError::Validation(format!(
            "OpenAPI export requires an absolute HTTP URL: {raw}"
        )));
    };
    let authority_start = scheme_end + 3;
    let path_start = without_query[authority_start..]
        .find('/')
        .map(|at| at + authority_start)
        .unwrap_or(without_query.len());
    let server = without_query[..path_start].to_string();
    let mut path = if path_start == without_query.len() {
        "/".to_string()
    } else {
        without_query[path_start..].to_string()
    };
    path = convert_thread_variables(&path);
    let query = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    Ok((convert_thread_variables(&server), path, query))
}

fn convert_thread_variables(value: &str) -> String {
    value.replace("{{", "{").replace("}}", "}")
}

fn add_security(auth: &AuthSpec, schemes: &mut Map<String, Value>) -> Option<Value> {
    let (name, scheme) = match auth.kind.as_str() {
        "inherit" | "none" => return None,
        "bearer" => (
            "ThreadBearer".to_string(),
            json!({ "type": "http", "scheme": "bearer" }),
        ),
        "basic" => (
            "ThreadBasic".to_string(),
            json!({ "type": "http", "scheme": "basic" }),
        ),
        "apiKey" => {
            let location = if auth.location.as_deref() == Some("query") {
                "query"
            } else {
                "header"
            };
            let key = auth.key.as_deref().unwrap_or("api_key");
            let name = format!(
                "ThreadApiKey{}",
                key.chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .collect::<String>()
            );
            (
                name,
                json!({ "type": "apiKey", "in": location, "name": key }),
            )
        }
        _ => return None,
    };
    schemes.insert(name.clone(), scheme);
    Some(json!([{ (name): [] }]))
}

fn http_body_to_openapi(request: &HttpRequestDoc) -> AppResult<Option<Value>> {
    let media_type = request
        .headers
        .iter()
        .find(|header| header.key.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.as_str())
        .unwrap_or(match request.body.kind.as_str() {
            "json" => "application/json",
            "formUrlencoded" => "application/x-www-form-urlencoded",
            "multipart" => "multipart/form-data",
            _ => "text/plain",
        });
    let content = match request.body.kind.as_str() {
        "none" | "" => return Ok(None),
        "json" | "raw" => {
            let example = if request.body.kind == "json" {
                serde_json::from_str::<Value>(&request.body.content)
                    .unwrap_or(Value::String(request.body.content.clone()))
            } else {
                Value::String(request.body.content.clone())
            };
            json!({ (media_type): { "example": example } })
        }
        "formUrlencoded" | "form" => {
            let example: Map<String, Value> = request
                .body
                .fields
                .iter()
                .filter(|field| field.enabled)
                .map(|field| (field.key.clone(), Value::String(field.value.clone())))
                .collect();
            json!({ "application/x-www-form-urlencoded": { "example": Value::Object(example) } })
        }
        "multipart" | "form-data" => {
            let mut properties = Map::new();
            for field in request.body.fields.iter().filter(|field| field.enabled) {
                properties.insert(
                    field.key.clone(),
                    if field.kind == "file" {
                        json!({ "type": "string", "format": "binary" })
                    } else {
                        json!({ "type": "string", "example": field.value })
                    },
                );
            }
            json!({ "multipart/form-data": { "schema": { "type": "object", "properties": Value::Object(properties) } } })
        }
        _ => return Ok(None),
    };
    Ok(Some(json!({ "required": false, "content": content })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"openapi: 3.0.3
info:
  title: Pet API
  version: 1.0.0
servers:
  - url: https://api.example.test/v1
paths:
  /pets/{petId}:
    get:
      operationId: getPet
      tags: [Pets]
      parameters:
        - name: petId
          in: path
          required: true
          schema: { type: string }
        - name: include
          in: query
          schema: { type: string }
          example: owner
      responses:
        '200': { description: OK }
    post:
      summary: Create pet
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string, example: Ada }
      responses:
        '201': { description: Created }
"#;

    #[test]
    fn imports_openapi_yaml_path_params_query_and_json_body() {
        let collection = parse_document(SPEC).unwrap();
        assert_eq!(collection.name, "Pet API");
        let items = collection
            .items
            .iter()
            .find_map(|item| match item {
                PostmanItem::Folder(folder) if folder.name == "Pets" => Some(&folder.items),
                _ => None,
            })
            .expect("expected the Pets tag folder");
        let PostmanItem::Request(get) = &items[0] else {
            panic!("expected GET")
        };
        assert_eq!(get.url, "https://api.example.test/v1/pets/{{petId}}");
        assert_eq!(get.query_params[0].value, "owner");
        let post = collection
            .items
            .iter()
            .find_map(|item| match item {
                PostmanItem::Request(request) if request.method == "POST" => Some(request),
                _ => None,
            })
            .expect("expected the untagged POST request");
        assert_eq!(post.body.kind, "json");
        assert!(post.body.content.contains("\"name\": \"Ada\""));
    }

    #[test]
    fn exports_collection_to_openapi_three_json() {
        let collection = parse_document(SPEC).unwrap();
        let document = collection_to_document(&collection).unwrap();
        assert_eq!(document["openapi"], "3.0.3");
        assert!(
            document["paths"]["/v1/pets/{petId}"]["get"].is_object(),
            "unexpected OpenAPI paths: {}",
            document["paths"]
        );
    }

    #[test]
    fn rejects_openapi_two_and_external_refs() {
        let error = parse_document("{\"swagger\":\"2.0\"}").unwrap_err();
        assert!(error.to_string().contains("OpenAPI 3.x"));
        let external = json!({"$ref":"https://example.test/schema.json"});
        assert!(resolve(&external, &external).is_err());
    }
}
