//! On-disk file formats for workspace configuration.
//!
//! 与 docs/03.database.md §2 一一对应。所有文件带 `version`，集合/请求/环境
//! 带可选 `id`（缺失时由相对路径派生确定性 ID，见 `layout::PATH_ID_PREFIX`）。

use serde::{Deserialize, Serialize};

use super::layout::FILE_VERSION;

fn default_version() -> u32 {
    FILE_VERSION
}

fn default_true() -> bool {
    true
}

/// Key/value row shared by query params, headers, metadata and form fields.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl KeyValue {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
            description: None,
        }
    }
}

/// Per-request overrides; `None` falls back to the collection/app default.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_ssl: Option<bool>,
}

/// Auth configuration. `kind == "inherit"` means "use the collection/app default".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthSpec {
    #[serde(rename = "type", default = "default_auth_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// API key header/query name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// `header` (default) or `query` for API key auth.
    #[serde(rename = "in", default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

fn default_auth_kind() -> String {
    "inherit".to_string()
}

impl Default for AuthSpec {
    fn default() -> Self {
        Self {
            kind: default_auth_kind(),
            token: None,
            username: None,
            password: None,
            key: None,
            value: None,
            location: None,
        }
    }
}

impl AuthSpec {
    pub fn is_inherit(&self) -> bool {
        self.kind == "inherit"
    }

    pub fn is_none_type(&self) -> bool {
        self.kind == "none"
    }

    /// Resolves inheritance: request auth wins, otherwise the collection auth applies.
    pub fn resolve(request: &AuthSpec, collection: Option<&AuthSpec>) -> AuthSpec {
        if request.is_inherit() {
            return collection.cloned().unwrap_or_default();
        }
        request.clone()
    }
}

/// Response/request body definition for HTTP requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpBody {
    #[serde(rename = "type", default = "default_body_kind")]
    pub kind: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<HttpBodyField>,
    /// Absolute path used by `binary` bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

fn default_body_kind() -> String {
    "none".to_string()
}

impl Default for HttpBody {
    fn default() -> Self {
        Self {
            kind: default_body_kind(),
            content: String::new(),
            content_type: None,
            fields: Vec::new(),
            file_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpBodyField {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    /// `text` (default) or `file`.
    #[serde(default = "default_field_kind")]
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

fn default_field_kind() -> String {
    "text".to_string()
}

// ---------------------------------------------------------------------------
// Workspace configuration — thread.workspace.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    /// Environment id, name or file stem; resolved leniently by the index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub settings: RequestSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub auto_start: bool,
}

fn default_proxy_port() -> u16 {
    9090
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: default_proxy_port(),
            auto_start: false,
        }
    }
}

impl WorkspaceConfig {
    pub fn new(name: &str) -> Self {
        Self {
            version: FILE_VERSION,
            id: None,
            name: name.to_string(),
            active_environment: None,
            proxy: ProxyConfig::default(),
            settings: RequestSettings::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Collection — collection.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMeta {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_script: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub post_script: String,
}

impl CollectionMeta {
    pub fn new(name: &str) -> Self {
        Self {
            version: FILE_VERSION,
            id: None,
            name: name.to_string(),
            description: None,
            auth: AuthSpec::default(),
            headers: Vec::new(),
            pre_script: String::new(),
            post_script: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Environments — environments/*.env.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub variables: Vec<EnvVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVariable {
    #[serde(default)]
    pub key: String,
    /// Secret values are never written here: the file keeps an empty value and
    /// the real value lives in the app data directory (`secrets/`).
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl EnvironmentFile {
    pub fn new(name: &str) -> Self {
        Self {
            version: FILE_VERSION,
            id: None,
            name: name.to_string(),
            variables: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Requests — *.http.json / *.grpc.json / *.ws.json / *.graphql.json / *.mqtt.json
// ---------------------------------------------------------------------------

/// Protocol-specific request documents, tagged by the `protocol` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum RequestFile {
    Http(HttpRequestDoc),
    Grpc(GrpcRequestDoc),
    Websocket(WsRequestDoc),
    Graphql(GraphqlRequestDoc),
    Mqtt(MqttRequestDoc),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequestDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: HttpBody,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub settings: RequestSettings,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_script: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub post_script: String,
}

fn default_method() -> String {
    "GET".to_string()
}

impl HttpRequestDoc {
    pub fn new(name: &str) -> Self {
        Self {
            version: FILE_VERSION,
            id: None,
            name: name.to_string(),
            method: default_method(),
            url: String::new(),
            query_params: Vec::new(),
            headers: Vec::new(),
            body: HttpBody::default(),
            auth: AuthSpec::default(),
            settings: RequestSettings::default(),
            pre_script: String::new(),
            post_script: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRequestDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub proto_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<KeyValue>,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default = "default_grpc_timeout")]
    pub timeout_ms: u64,
}

fn default_grpc_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsRequestDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,
    #[serde(default)]
    pub auto_reconnect: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_messages: Vec<WsSavedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsSavedMessage {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlRequestDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub variables: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub auth: AuthSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttRequestDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub broker_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<MqttSubscription>,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default = "default_keep_alive")]
    pub keep_alive_secs: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttSubscription {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub qos: u8,
}

fn default_keep_alive() -> u16 {
    60
}

impl RequestFile {
    pub fn protocol(&self) -> &'static str {
        match self {
            Self::Http(_) => "http",
            Self::Grpc(_) => "grpc",
            Self::Websocket(_) => "websocket",
            Self::Graphql(_) => "graphql",
            Self::Mqtt(_) => "mqtt",
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Http(doc) => doc.id.as_deref(),
            Self::Grpc(doc) => doc.id.as_deref(),
            Self::Websocket(doc) => doc.id.as_deref(),
            Self::Graphql(doc) => doc.id.as_deref(),
            Self::Mqtt(doc) => doc.id.as_deref(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Http(doc) => &doc.name,
            Self::Grpc(doc) => &doc.name,
            Self::Websocket(doc) => &doc.name,
            Self::Graphql(doc) => &doc.name,
            Self::Mqtt(doc) => &doc.name,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        match self {
            Self::Http(doc) => doc.name = name.to_string(),
            Self::Grpc(doc) => doc.name = name.to_string(),
            Self::Websocket(doc) => doc.name = name.to_string(),
            Self::Graphql(doc) => doc.name = name.to_string(),
            Self::Mqtt(doc) => doc.name = name.to_string(),
        }
    }

    pub fn set_id(&mut self, id: &str) {
        match self {
            Self::Http(doc) => doc.id = Some(id.to_string()),
            Self::Grpc(doc) => doc.id = Some(id.to_string()),
            Self::Websocket(doc) => doc.id = Some(id.to_string()),
            Self::Graphql(doc) => doc.id = Some(id.to_string()),
            Self::Mqtt(doc) => doc.id = Some(id.to_string()),
        }
    }

    /// Method/URL summary used by list views (HTTP only for now).
    pub fn summary(&self) -> (Option<String>, Option<String>) {
        match self {
            Self::Http(doc) => (Some(doc.method.clone()), Some(doc.url.clone())),
            Self::Grpc(doc) => (Some(doc.method.clone()), Some(doc.host.clone())),
            Self::Graphql(doc) => (Some("POST".into()), Some(doc.endpoint.clone())),
            Self::Websocket(doc) => (Some("WS".into()), Some(doc.url.clone())),
            Self::Mqtt(doc) => (Some("MQTT".into()), Some(doc.broker_url.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_document_round_trips_documented_shape() {
        let raw = r#"{
            "version": 1,
            "protocol": "http",
            "name": "Create User",
            "method": "POST",
            "url": "{{baseUrl}}/api/users",
            "queryParams": [
                { "key": "page", "value": "1", "enabled": false, "description": "" }
            ],
            "headers": [
                { "key": "Authorization", "value": "Bearer {{apiKey}}", "enabled": true }
            ],
            "body": { "type": "json", "content": "{}" },
            "auth": { "type": "inherit" },
            "settings": { "timeoutMs": 10000, "followRedirects": true, "verifySsl": true }
        }"#;

        let parsed: RequestFile = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.protocol(), "http");
        assert_eq!(parsed.name(), "Create User");
        assert_eq!(parsed.id(), None);

        let serialized = serde_json::to_string_pretty(&parsed).unwrap();
        assert!(serialized.contains("\"protocol\": \"http\""));
        assert!(serialized.contains("\"method\": \"POST\""));

        // The tag is not duplicated inside the payload.
        let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(value["protocol"], "http");
        assert_eq!(value["settings"]["timeoutMs"], 10000);
        assert_eq!(value["queryParams"][0]["enabled"], false);
    }

    #[test]
    fn defaults_fill_missing_fields() {
        let parsed: RequestFile =
            serde_json::from_str(r#"{"protocol":"http","name":"Ping"}"#).unwrap();
        let RequestFile::Http(doc) = &parsed else {
            panic!("expected http document")
        };
        assert_eq!(doc.method, "GET");
        assert_eq!(doc.body.kind, "none");
        assert!(doc.auth.is_inherit());
    }

    #[test]
    fn all_documented_protocols_deserialize() {
        let samples = [
            r#"{"protocol":"grpc","name":"SayHello","host":"localhost:50051","service":"helloworld.Greeter","method":"SayHello"}"#,
            r#"{"protocol":"websocket","name":"Chat","url":"wss://example.com/ws"}"#,
            r#"{"protocol":"graphql","name":"Get Users","endpoint":"{{baseUrl}}/graphql","query":"query { users { id } }"}"#,
            r#"{"protocol":"mqtt","name":"IoT","brokerUrl":"mqtt://localhost:1883","subscriptions":[{"topic":"a/+","qos":1}]}"#,
        ];
        let expected = ["grpc", "websocket", "graphql", "mqtt"];
        for (sample, protocol) in samples.iter().zip(expected) {
            let parsed: RequestFile = serde_json::from_str(sample).unwrap();
            assert_eq!(parsed.protocol(), protocol);
        }
    }

    #[test]
    fn auth_inheritance_prefers_request_then_collection() {
        let inherit = AuthSpec::default();
        let collection = AuthSpec {
            kind: "bearer".into(),
            token: Some("{{apiKey}}".into()),
            ..AuthSpec::default()
        };
        let resolved = AuthSpec::resolve(&inherit, Some(&collection));
        assert_eq!(resolved.kind, "bearer");

        let explicit = AuthSpec {
            kind: "basic".into(),
            username: Some("u".into()),
            ..AuthSpec::default()
        };
        let resolved = AuthSpec::resolve(&explicit, Some(&collection));
        assert_eq!(resolved.kind, "basic");
    }

    #[test]
    fn environment_file_keeps_secret_flags_without_values() {
        let raw = r#"{
            "version": 1,
            "name": "dev",
            "variables": [
                { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true, "secret": false },
                { "key": "apiKey", "value": "", "enabled": true, "secret": true }
            ]
        }"#;
        let parsed: EnvironmentFile = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.variables.len(), 2);
        assert!(parsed.variables[1].secret);
        assert_eq!(parsed.variables[1].value, "");
    }
}
