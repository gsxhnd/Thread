//! 请求视图：标签页 + 方法/URL/发送 + 参数/请求头/请求体 + 响应面板。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState, TabSize};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IndexPath, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    PathPromptOptions, SharedString, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use std::collections::BTreeMap;

use thread_core::error::AppError;
use thread_core::services::history::HistoryDetail;
use thread_core::services::http::{HttpRequestConfig, HttpResponse, Timing};
use thread_core::workspace::formats::{
    AuthSpec, GraphqlRequestDoc, GrpcRequestDoc, HttpBody, HttpBodyField, HttpRequestDoc, KeyValue,
    RequestFile, WsRequestDoc,
};
use uuid::Uuid;

use crate::app::{AppView, RequestTab};
use crate::format::HTTP_METHODS;
use crate::views::{history, protocols, response};
use crate::widgets;

/// 请求编辑区上方的分区。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RequestSection {
    Params,
    Auth,
    Headers,
    Body,
    Timeline,
}

impl RequestSection {
    pub fn all() -> [RequestSection; 5] {
        [
            Self::Params,
            Self::Auth,
            Self::Headers,
            Self::Body,
            Self::Timeline,
        ]
    }

    fn label(self, cx: &App) -> &'static str {
        match self {
            Self::Params => tr!(cx, "参数", "Params"),
            Self::Auth => tr!(cx, "认证", "Auth"),
            Self::Headers => tr!(cx, "请求头", "Headers"),
            Self::Body => tr!(cx, "请求体", "Body"),
            Self::Timeline => tr!(cx, "时间线", "Timeline"),
        }
    }
}

/// 标签页协议。MQTT 只展示，不发送。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protocol {
    Http,
    Graphql,
    Websocket,
    Grpc,
    Mqtt,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Graphql => "GQL",
            Self::Websocket => "WS",
            Self::Grpc => "gRPC",
            Self::Mqtt => "MQTT",
        }
    }

    pub fn draft_name(self, cx: &App) -> &'static str {
        match self {
            Self::Http => tr!(cx, "临时请求", "Untitled request"),
            Self::Graphql => "GraphQL",
            Self::Websocket => "WebSocket",
            Self::Grpc => "gRPC",
            Self::Mqtt => "MQTT",
        }
    }

    pub fn file_protocol(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
            Self::Websocket => "websocket",
            Self::Grpc => "grpc",
            Self::Mqtt => "mqtt",
        }
    }
}

/// GraphQL / WebSocket / gRPC 编辑区的分区。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorPane {
    Primary,
    Secondary,
    Headers,
}

/// 键值表对应哪一组数据。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KvKind {
    Params,
    Headers,
    Form,
}

pub const BODY_KINDS: [&str; 5] = ["none", "json", "raw", "form", "form-data"];

/// 回填一个 HTTP 请求时需要的全部字段（来自文件文档或历史快照）。
struct HttpFields<'a> {
    name: &'a str,
    method: &'a str,
    url: &'a str,
    params: &'a [KeyValue],
    headers: &'a [KeyValue],
    body: &'a HttpBody,
    auth: &'a AuthSpec,
}

/// 渲染键值行时用的快照，拷出实体句柄后就不再借用标签页。
struct KvRowView {
    key: Entity<InputState>,
    value: Entity<InputState>,
    enabled: bool,
    file: bool,
    file_path: Option<String>,
}

/// 一行键值编辑（参数 / 请求头 / 表单字段共用）。
pub struct KvRow {
    pub key: Entity<InputState>,
    pub value: Entity<InputState>,
    pub enabled: bool,
    /// form-data 里这一行选的是本地文件，而不是文本。
    pub file: bool,
    pub file_path: Option<String>,
}

impl KvRow {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<AppView>,
        key: &str,
        value: &str,
        enabled: bool,
    ) -> Self {
        Self {
            key: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr!(cx, "名称", "Name"))
                    .default_value(key)
            }),
            value: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr!(cx, "值", "Value"))
                    .default_value(value)
            }),
            enabled,
            file: false,
            file_path: None,
        }
    }

    pub fn to_kv(&self, cx: &App) -> KeyValue {
        KeyValue {
            key: self.key.read(cx).value().to_string(),
            value: self.value.read(cx).value().to_string(),
            enabled: self.enabled,
            description: None,
        }
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.key.read(cx).value().trim().is_empty()
    }
}

impl RequestTab {
    fn new_common(
        path: Option<String>,
        name: String,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        Self {
            path,
            name,
            dirty: false,
            protocol: Protocol::Http,
            method: cx.new(|cx| {
                SelectState::new(
                    HTTP_METHODS.to_vec(),
                    Some(IndexPath::default().row(0)),
                    window,
                    cx,
                )
            }),
            id: Uuid::new_v4().to_string(),
            url: cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/api")),
            url_watch: None,
            edit_watches: Vec::new(),
            local_runs: Vec::new(),
            timeline_selected: None,
            timeline_request: None,
            timeline_wire: false,
            params: Vec::new(),
            headers: Vec::new(),
            form_fields: Vec::new(),
            auth_kind: "inherit".into(),
            auth_token: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "令牌", "Token"))),
            auth_username: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "用户名", "Username"))),
            auth_password: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "密码", "Password"))),
            auth_key: cx.new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "键", "Key"))),
            auth_value: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!(cx, "值", "Value"))),
            auth_in: "header".into(),
            body_kind: cx.new(|cx| {
                SelectState::new(
                    BODY_KINDS.to_vec(),
                    Some(IndexPath::default().row(0)),
                    window,
                    cx,
                )
            }),
            body: cx.new(|cx| {
                let mut state = EditorState::new(window, cx)
                    .language("json")
                    .folding(true)
                    .tab_size(TabSize {
                        tab_size: 2,
                        hard_tabs: false,
                    });
                // 括号折叠：没有 tree-sitter 语法时由 json_fold 给出范围。
                state.set_highlighter_factory(crate::json_fold::factory(), cx);
                state
            }),
            variables: cx.new(|cx| {
                let mut state = EditorState::new(window, cx)
                    .language("json")
                    .folding(true)
                    .tab_size(TabSize {
                        tab_size: 2,
                        hard_tabs: false,
                    });
                // GraphQL variables 同样是 JSON，折叠行为与请求体一致。
                state.set_highlighter_factory(crate::json_fold::factory(), cx);
                state
            }),
            service: cx.new(|cx| InputState::new(window, cx).placeholder("Service")),
            rpc_method: cx.new(|cx| InputState::new(window, cx).placeholder("Method")),
            operation_name: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr!(
                    cx,
                    "操作名，可空",
                    "Operation name, optional"
                ))
            }),
            message: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr!(cx, "要发送的消息", "Message to send"))
            }),
            section: RequestSection::Body,
            pane: EditorPane::Primary,
            response_section: response::ResponseSection::Body,
            task_id: None,
            sending: false,
            response: None,
            error: None,
            started_at: None,
            tls: false,
            auto_reconnect: false,
            binary_message: false,
            pool_id: None,
            services: Vec::new(),
            connection_id: None,
            log: Vec::new(),
            schema: None,
            suggestions: Vec::new(),
            ws_protocols: Vec::new(),
            saved_messages: Vec::new(),
            proto_source: "file".into(),
            proto_file_path: None,
            timeout_ms: 30_000,
            preserved: None,
        }
    }

    /// 新建临时（未保存）请求。
    pub fn draft(protocol: Protocol, window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let mut tab = Self::new_common(None, protocol.draft_name(cx).to_string(), window, cx);
        tab.protocol = protocol;
        tab
    }

    /// 从磁盘上的请求文档构建标签页。
    pub fn from_detail(
        path: String,
        name: String,
        document: Option<HttpRequestDoc>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let mut tab = Self::new_common(Some(path), name.clone(), window, cx);
        tab.error = error.map(|message| {
            tr!(
                cx,
                format!("请求文件解析失败：{message}"),
                format!("Could not parse the request file: {message}")
            )
        });
        let Some(doc) = document else {
            return tab;
        };

        tab.apply_http_doc(&doc, window, cx);
        tab
    }

    /// 用文件里的 HTTP 文档填这一页。
    pub fn apply_http_doc(
        &mut self,
        doc: &HttpRequestDoc,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) {
        self.fill_http(
            HttpFields {
                name: &doc.name,
                method: &doc.method,
                url: &doc.url,
                params: &doc.query_params,
                headers: &doc.headers,
                body: &doc.body,
                auth: &doc.auth,
            },
            window,
            cx,
        );
    }

    /// 用上一次实际发出的配置填这一页，并带回当时的响应。
    pub fn restore_last_run(
        &mut self,
        detail: &HistoryDetail,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) {
        if self.protocol != Protocol::Http {
            return;
        }
        let snapshot = &detail.request_snapshot;
        let restored = snapshot
            .get("sent")
            .and_then(|sent| serde_json::from_value::<HttpRequestConfig>(sent.clone()).ok())
            .map(|config| {
                let name = self.name.clone();
                self.fill_http(
                    HttpFields {
                        name: &name,
                        method: &config.method,
                        url: &config.url,
                        params: &config.query_params,
                        headers: &config.headers,
                        body: &config.body,
                        auth: &config.auth,
                    },
                    window,
                    cx,
                );
            })
            .is_some()
            || snapshot
                .get("document")
                .and_then(|document| {
                    serde_json::from_value::<HttpRequestDoc>(document.clone()).ok()
                })
                .map(|doc| self.apply_http_doc(&doc, window, cx))
                .is_some()
            || snapshot
                .get("document")
                .and_then(|document| {
                    serde_json::from_value::<HttpRequestConfig>(document.clone()).ok()
                })
                .map(|config| {
                    let name = snapshot
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or(&self.name)
                        .to_string();
                    self.fill_http(
                        HttpFields {
                            name: &name,
                            method: &config.method,
                            url: &config.url,
                            params: &config.query_params,
                            headers: &config.headers,
                            body: &config.body,
                            auth: &config.auth,
                        },
                        window,
                        cx,
                    );
                })
                .is_some();
        if !restored {
            return;
        }
        if let Some(response) = response_from_history(detail) {
            self.response = Some(response);
        }
        self.dirty = false;
    }

    fn fill_http(
        &mut self,
        fields: HttpFields<'_>,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) {
        let HttpFields {
            name,
            method,
            url,
            params,
            headers,
            body,
            auth,
        } = fields;
        if !name.is_empty() {
            self.name = name.to_string();
        }
        let method_name = method.to_ascii_uppercase();
        if let Some(ix) = HTTP_METHODS
            .iter()
            .position(|candidate| *candidate == method_name)
        {
            let method = self.method.clone();
            method.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::default().row(ix)), window, cx);
            });
        }
        set_input(&self.url, url, window, cx);
        self.params.clear();
        for pair in params {
            self.params
                .push(KvRow::new(window, cx, &pair.key, &pair.value, pair.enabled));
        }
        self.headers.clear();
        for pair in headers {
            self.headers
                .push(KvRow::new(window, cx, &pair.key, &pair.value, pair.enabled));
        }
        let ui_kind = match body.kind.as_str() {
            "form" | "formUrlencoded" => "form",
            "multipart" | "formData" => "form-data",
            other => other,
        };
        if let Some(ix) = BODY_KINDS
            .iter()
            .position(|candidate| *candidate == ui_kind)
        {
            let body_kind = self.body_kind.clone();
            body_kind.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::default().row(ix)), window, cx);
            });
        }
        let form = matches!(ui_kind, "form" | "form-data");
        let body_entity = self.body.clone();
        body_entity.update(cx, |state, cx| {
            state.set_placeholder(body_placeholder(ui_kind, cx), window, cx)
        });
        self.form_fields.clear();
        if form {
            for field in &body.fields {
                let shown = if field.kind == "file" {
                    field.file_path.clone().unwrap_or_default()
                } else {
                    field.value.clone()
                };
                let mut row = KvRow::new(window, cx, &field.key, &shown, field.enabled);
                if field.kind == "file" {
                    row.file = true;
                    row.file_path = field.file_path.clone();
                }
                self.form_fields.push(row);
            }
        } else {
            let content = if ui_kind == "json" {
                pretty_json(&body.content).unwrap_or_else(|| body.content.clone())
            } else {
                body.content.clone()
            };
            set_editor(&self.body, &content, window, cx);
        }
        self.apply_auth(auth, window, cx);
    }

    fn apply_auth(&mut self, auth: &AuthSpec, window: &mut Window, cx: &mut Context<AppView>) {
        self.auth_kind = match auth.kind.as_str() {
            "none" | "bearer" | "basic" | "apiKey" | "inherit" => auth.kind.clone(),
            "oauth2" => "bearer".into(),
            _ => "inherit".into(),
        };
        self.auth_in = if auth.location.as_deref() == Some("query") {
            "query".into()
        } else {
            "header".into()
        };
        set_input(
            &self.auth_token,
            auth.token.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.auth_username,
            auth.username.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.auth_password,
            auth.password.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.auth_key,
            auth.key.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.auth_value,
            auth.value.as_deref().unwrap_or(""),
            window,
            cx,
        );
    }

    fn auth_spec(&self, cx: &App) -> AuthSpec {
        let text = |input: &Entity<InputState>| {
            let value = input.read(cx).value().trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        };
        AuthSpec {
            kind: self.auth_kind.clone(),
            token: text(&self.auth_token),
            username: text(&self.auth_username),
            password: text(&self.auth_password),
            key: text(&self.auth_key),
            value: text(&self.auth_value),
            location: if self.auth_kind == "apiKey" {
                Some(self.auth_in.clone())
            } else {
                None
            },
        }
    }

    /// 按文件里的协议打开标签页。解析失败时仍打开一个带错误说明的页。
    pub fn from_file(
        path: String,
        loaded: Result<RequestFile, AppError>,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let file = match loaded {
            Ok(file) => file,
            Err(error) => {
                let mut tab = Self::new_common(
                    Some(path),
                    tr!(cx, "无法读取", "Unreadable").into(),
                    window,
                    cx,
                );
                tab.error = Some(tr!(
                    cx,
                    format!("请求文件解析失败：{error}"),
                    format!("Could not parse the request file: {error}")
                ));
                return tab;
            }
        };
        match file {
            RequestFile::Http(doc) => {
                let name = doc.name.clone();
                Self::from_detail(path, name, Some(doc), None, window, cx)
            }
            RequestFile::Graphql(doc) => {
                let mut tab = Self::draft(Protocol::Graphql, window, cx);
                tab.path = Some(path);
                tab.name = doc.name.clone();
                set_input(&tab.url, &doc.endpoint, window, cx);
                set_editor(&tab.body, &doc.query, window, cx);
                set_editor(&tab.variables, &doc.variables, window, cx);
                set_input(
                    &tab.operation_name,
                    doc.operation_name.as_deref().unwrap_or(""),
                    window,
                    cx,
                );
                push_pairs(&mut tab.headers, &doc.headers, window, cx);
                tab
            }
            RequestFile::Websocket(doc) => {
                let mut tab = Self::draft(Protocol::Websocket, window, cx);
                tab.path = Some(path);
                tab.name = doc.name.clone();
                set_input(&tab.url, &doc.url, window, cx);
                tab.auto_reconnect = doc.auto_reconnect;
                tab.ws_protocols = doc.protocols.clone();
                tab.saved_messages = doc.saved_messages.clone();
                push_pairs(&mut tab.headers, &doc.headers, window, cx);
                tab
            }
            RequestFile::Grpc(doc) => {
                let mut tab = Self::draft(Protocol::Grpc, window, cx);
                tab.path = Some(path);
                tab.name = doc.name.clone();
                set_input(&tab.url, &doc.host, window, cx);
                set_input(&tab.service, &doc.service, window, cx);
                set_input(&tab.rpc_method, &doc.method, window, cx);
                set_editor(&tab.body, &doc.payload, window, cx);
                tab.tls = doc.tls_enabled;
                tab.proto_source = doc.proto_source.clone();
                tab.proto_file_path = doc.proto_file_path.clone();
                tab.timeout_ms = doc.timeout_ms;
                push_pairs(&mut tab.headers, &doc.metadata, window, cx);
                tab
            }
            RequestFile::Mqtt(doc) => {
                let mut tab = Self::draft(Protocol::Mqtt, window, cx);
                tab.path = Some(path);
                tab.name = doc.name.clone();
                set_input(&tab.url, &doc.broker_url, window, cx);
                tab.preserved = serde_json::to_value(RequestFile::Mqtt(doc)).ok();
                tab
            }
        }
    }

    /// 从当前 UI 收集发送配置。
    pub fn collect_config(&self, cx: &App) -> HttpRequestConfig {
        let method = self
            .method
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("GET");
        let body_kind = self
            .body_kind
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("none");

        let body = match body_kind {
            "none" => HttpBody::default(),
            "form" | "form-data" => HttpBody {
                kind: if body_kind == "form-data" {
                    "multipart".into()
                } else {
                    "formUrlencoded".into()
                },
                fields: self
                    .form_fields
                    .iter()
                    .filter(|row| row.enabled && !row.is_empty(cx))
                    .filter_map(|row| {
                        if body_kind == "form-data" && row.file {
                            let file_path = row.file_path.clone().filter(|path| !path.is_empty());
                            let file_path = file_path?;
                            Some(HttpBodyField {
                                key: row.key.read(cx).value().to_string(),
                                value: String::new(),
                                kind: "file".into(),
                                enabled: true,
                                file_path: Some(file_path),
                                content_type: None,
                            })
                        } else {
                            Some(HttpBodyField {
                                key: row.key.read(cx).value().to_string(),
                                value: row.value.read(cx).value().to_string(),
                                kind: "text".into(),
                                enabled: true,
                                file_path: None,
                                content_type: None,
                            })
                        }
                    })
                    .collect(),
                ..HttpBody::default()
            },
            other => HttpBody {
                kind: other.to_string(),
                content: self.body.read(cx).value().to_string(),
                ..HttpBody::default()
            },
        };

        HttpRequestConfig {
            method: method.to_string(),
            url: self.url.read(cx).value().to_string(),
            query_params: self.params.iter().map(|row| row.to_kv(cx)).collect(),
            headers: self.headers.iter().map(|row| row.to_kv(cx)).collect(),
            body,
            auth: self.auth_spec(cx),
            settings: Default::default(),
        }
    }

    /// 序列化成保存到磁盘的请求文档。
    pub fn to_document(&self, cx: &App) -> serde_json::Value {
        // MQTT 只展示不发送，保存时原样写回读入的文档，不丢字段。
        if self.protocol == Protocol::Mqtt
            && let Some(value) = self.preserved.clone()
        {
            return value;
        }
        let file = match self.protocol {
            Protocol::Http | Protocol::Mqtt => {
                let config = self.collect_config(cx);
                RequestFile::Http(HttpRequestDoc {
                    version: 1,
                    id: None,
                    name: self.name.clone(),
                    method: config.method,
                    url: config.url,
                    query_params: config.query_params,
                    headers: config.headers,
                    body: config.body,
                    auth: config.auth,
                    settings: config.settings,
                    pre_script: String::new(),
                    post_script: String::new(),
                })
            }
            Protocol::Graphql => {
                let operation = self.operation_name.read(cx).value().trim().to_string();
                RequestFile::Graphql(GraphqlRequestDoc {
                    version: 1,
                    id: None,
                    name: self.name.clone(),
                    endpoint: self.url.read(cx).value().to_string(),
                    query: self.body.read(cx).value().to_string(),
                    variables: self.variables.read(cx).value().to_string(),
                    headers: self.headers.iter().map(|row| row.to_kv(cx)).collect(),
                    auth: Default::default(),
                    operation_name: if operation.is_empty() {
                        None
                    } else {
                        Some(operation)
                    },
                })
            }
            Protocol::Websocket => RequestFile::Websocket(WsRequestDoc {
                version: 1,
                id: None,
                name: self.name.clone(),
                url: self.url.read(cx).value().to_string(),
                headers: self.headers.iter().map(|row| row.to_kv(cx)).collect(),
                protocols: self.ws_protocols.clone(),
                auto_reconnect: self.auto_reconnect,
                saved_messages: self.saved_messages.clone(),
            }),
            Protocol::Grpc => RequestFile::Grpc(GrpcRequestDoc {
                version: 1,
                id: None,
                name: self.name.clone(),
                host: self.url.read(cx).value().to_string(),
                service: self.service.read(cx).value().to_string(),
                method: self.rpc_method.read(cx).value().to_string(),
                proto_source: self.proto_source.clone(),
                proto_file_path: self.proto_file_path.clone(),
                metadata: self.headers.iter().map(|row| row.to_kv(cx)).collect(),
                payload: self.body.read(cx).value().to_string(),
                tls_enabled: self.tls,
                timeout_ms: self.timeout_ms,
            }),
        };
        serde_json::to_value(file).unwrap_or(serde_json::Value::Null)
    }
}

fn set_input(
    input: &Entity<InputState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let value = value.to_string();
    input.update(cx, |state, cx| state.set_value(value, window, cx));
}

fn set_editor(
    editor: &Entity<EditorState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let value = value.to_string();
    editor.update(cx, |state, cx| state.set_value(value, window, cx));
}

fn push_pairs(
    rows: &mut Vec<KvRow>,
    pairs: &[KeyValue],
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    for pair in pairs {
        rows.push(KvRow::new(window, cx, &pair.key, &pair.value, pair.enabled));
    }
}

/// 请求主区（标签条 + 编辑区 + 响应面板）。
pub fn render(this: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(active) = this.active_tab.filter(|ix| *ix < this.tabs.len()) else {
        return v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .child(widgets::empty_state(
                cx,
                IconName::Send,
                tr!(cx, "从一个请求开始", "Start with a request"),
                tr!(
                    cx,
                    "在左侧集合树里点开一个请求，或新建临时请求。HTTP、GraphQL、WebSocket 和 gRPC 都能在这里编辑。",
                    "Open one from the collection tree, or start an untitled request. HTTP, GraphQL, WebSocket, and gRPC can all be edited here."
                ),
            ))
            .child(
                h_flex().gap_2().child(draft_button(
                    "draft-http",
                    "HTTP",
                    Protocol::Http,
                    true,
                    cx,
                )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(draft_button(
                        "draft-gql",
                        "GraphQL",
                        Protocol::Graphql,
                        false,
                        cx,
                    ))
                    .child(draft_button(
                        "draft-ws",
                        "WebSocket",
                        Protocol::Websocket,
                        false,
                        cx,
                    ))
                    .child(draft_button("draft-grpc", "gRPC", Protocol::Grpc, false, cx)),
            )
            .into_any_element();
    };

    // 先把要用的实体句柄拷出来，避免下面与标签条遍历打架。
    let (method, url, body, body_kind, section, error, sending, has_path) = {
        let tab = &this.tabs[active];
        (
            tab.method.clone(),
            tab.url.clone(),
            tab.body.clone(),
            tab.body_kind.clone(),
            tab.section,
            tab.error.clone(),
            tab.sending,
            tab.path.is_some(),
        )
    };

    let tab_count = this.tabs.len();
    let theme_for_tabs = theme.clone();

    let tab_bar = h_flex()
        .w_full()
        .flex_shrink_0()
        .items_end()
        .gap_1()
        .px_2()
        .bg(theme_for_tabs.sidebar)
        .border_b_1()
        .border_color(theme_for_tabs.border)
        .id("request-tab-bar")
        .overflow_x_scroll()
        .children((0..tab_count).map(|ix| {
            let name = this.tabs[ix].name.clone();
            let dirty = this.tabs[ix].dirty;
            let is_active = this.active_tab == Some(ix);
            let tab_id = this.tabs[ix].id.clone();
            let close_id = tab_id.clone();
            h_flex()
                .id(SharedString::from(format!("tab-{tab_id}")))
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .text_sm()
                .border_b_2()
                .when(is_active, |this| {
                    this.border_color(theme_for_tabs.primary)
                        .bg(theme_for_tabs.background)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!is_active, |this| {
                    this.border_color(theme_for_tabs.sidebar)
                        .text_color(theme_for_tabs.muted_foreground)
                        .hover(|this| this.bg(theme_for_tabs.muted))
                })
                .cursor_pointer()
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_active {
                            theme_for_tabs.primary
                        } else {
                            theme_for_tabs.muted_foreground
                        })
                        .child(this.tabs[ix].protocol.label()),
                )
                .when(dirty, |this| {
                    this.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(div().size(px(6.)).rounded_full().bg(theme_for_tabs.primary))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme_for_tabs.primary)
                                    .child(tr!(cx, "未保存", "Unsaved")),
                            ),
                    )
                })
                .child(div().text_ellipsis().max_w(px(180.)).child(name))
                .child(
                    div()
                        .id(SharedString::from(format!("tab-close-{close_id}")))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_4()
                        .rounded_sm()
                        .hover(|this| this.bg(theme_for_tabs.border))
                        .cursor_pointer()
                        .child(Icon::new(IconName::X).xsmall())
                        .on_click(cx.listener(
                            move |this, _: &gpui_kit::ClickEvent, window, cx| {
                                cx.stop_propagation();
                                let Some(index) =
                                    this.tabs.iter().position(|tab| tab.id == close_id)
                                else {
                                    return;
                                };
                                this.close_tab(index, window, cx);
                            },
                        )),
                )
                .on_click(cx.listener(move |this, _: &gpui_kit::ClickEvent, _, cx| {
                    this.active_tab = Some(ix);
                    cx.notify();
                }))
                .on_aux_click(
                    cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let Some(index) = this.tabs.iter().position(|tab| tab.id == tab_id) else {
                            return;
                        };
                        this.close_tab(index, window, cx);
                    }),
                )
        }))
        .child(
            Button::new("tab-new")
                .ghost()
                .xsmall()
                .icon(IconName::Plus)
                .tooltip_with_action(
                    tr!(cx, "新建临时请求", "New untitled request"),
                    &crate::NewRequest,
                    None,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.new_draft_tab(window, cx);
                })),
        );

    if this.tabs[active].protocol != Protocol::Http {
        let show_timeline = this.tabs[active].section == RequestSection::Timeline;
        return v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(tab_bar)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(widgets::band(
                        theme.border,
                        theme.background,
                        Button::new("proto-timeline")
                            .ghost()
                            .xsmall()
                            .label(if show_timeline {
                                tr!(cx, "返回请求", "Back to request")
                            } else {
                                tr!(cx, "时间线", "Timeline")
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(tab) = this.tabs.get_mut(active) {
                                    tab.section = if tab.section == RequestSection::Timeline {
                                        RequestSection::Body
                                    } else {
                                        RequestSection::Timeline
                                    };
                                }
                                cx.notify();
                            })),
                    ))
                    .child(if show_timeline {
                        history::request_timeline(this, active, cx)
                    } else {
                        protocols::body(this, active, window, cx)
                    }),
            )
            .into_any_element();
    }

    // 动作行
    let send_button = if sending {
        Button::new("cancel")
            .outline()
            .label(tr!(cx, "取消", "Cancel"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.cancel_request(active, window, cx);
            }))
    } else {
        Button::new("send")
            .primary()
            .label(tr!(cx, "发送", "Send"))
            .tooltip_with_action(tr!(cx, "发送", "Send"), &crate::SendRequest, None)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.send_request(active, window, cx);
            }))
    };

    let save_button = if has_path {
        Button::new("save")
            .outline()
            .label(tr!(cx, "保存", "Save"))
            .tooltip_with_action(tr!(cx, "保存", "Save"), &crate::SaveRequest, None)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.save_request(active, window, cx);
            }))
    } else {
        Button::new("save-as")
            .outline()
            .label(tr!(cx, "保存到工作区", "Save to workspace"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.save_request_as(active, window, cx);
            }))
    };

    let section_bar =
        h_flex()
            .gap_1()
            .pb_1()
            .children(RequestSection::all().map(|section_candidate| {
                let selected = section == section_candidate;
                div()
                    .id(SharedString::from(format!("section-{section_candidate:?}")))
                    .px_2()
                    .py_1p5()
                    .text_xs()
                    .border_b_2()
                    .when(selected, |this| {
                        this.border_color(theme.primary)
                            .text_color(theme.foreground)
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |this| {
                        this.border_color(gpui_kit::transparent_black())
                            .text_color(theme.muted_foreground)
                            .hover(|this| this.text_color(theme.foreground))
                    })
                    .cursor_pointer()
                    .child(section_label_with_count(
                        section_candidate,
                        active,
                        &this.tabs,
                        cx,
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(tab) = this.tabs.get_mut(active) {
                            tab.section = section_candidate;
                        }
                        if section_candidate == RequestSection::Timeline {
                            this.reload_request_timeline(active, cx);
                        }
                        cx.notify();
                    }))
            }));

    let editor = match section {
        RequestSection::Params => div()
            .size_full()
            .id("request-editor")
            .overflow_y_scroll()
            .child(kv_editor(this, active, KvKind::Params, cx))
            .into_any_element(),
        RequestSection::Headers => div()
            .size_full()
            .id("request-editor")
            .overflow_y_scroll()
            .child(kv_editor(this, active, KvKind::Headers, cx))
            .into_any_element(),
        RequestSection::Auth => auth_editor(this, active, cx),
        RequestSection::Body => body_editor(this, active, &body_kind, &body, cx),
        RequestSection::Timeline => crate::views::history::request_timeline(this, active, cx),
    };
    let response_panel = Some(response::render(this, active, window, cx));

    v_flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(tab_bar)
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .when_some(error, |this, error| {
                    this.child(widgets::band(
                        theme.border,
                        theme.background,
                        widgets::notice_line(cx, error),
                    ))
                })
                .child(widgets::band(
                    theme.border,
                    theme.background,
                    v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            h_flex()
                                .w_full()
                                .flex_shrink_0()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(108.))
                                        .flex_shrink_0()
                                        .child(Select::new(&method).small()),
                                )
                                .child(crate::views::url_vars::field(this, active, &url, cx))
                                .child(
                                    // 复制的是输入框当前内容，含未替换的 {{变量}}。
                                    Clipboard::new("copy-request-url")
                                        .small()
                                        .value_fn(move |_, cx| url.read(cx).value())
                                        .tooltip(tr!(cx, "复制 URL", "Copy URL")),
                                )
                                .child(
                                    // 点击时按当前表单导出 cURL，含请求体与认证。
                                    Clipboard::new("copy-curl")
                                        .small()
                                        .value_fn(move |_, cx| {
                                            let Some(main) = cx.try_global::<crate::app::MainApp>()
                                            else {
                                                return SharedString::default();
                                            };
                                            let Some(view) = main.0.upgrade() else {
                                                return SharedString::default();
                                            };
                                            let Some(tab) = view.read(cx).tabs.get(active) else {
                                                return SharedString::default();
                                            };
                                            SharedString::from(crate::curl::command(
                                                &tab.collect_config(cx),
                                            ))
                                        })
                                        .tooltip(tr!(cx, "复制为 cURL", "Copy as cURL")),
                                )
                                .child(save_button)
                                .child(send_button),
                        )
                        .child(section_bar),
                ))
                .when(sending, |this| {
                    this.child(widgets::band(
                        theme.border,
                        theme.background,
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .items_center()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(Spinner::new())
                            .child(tr!(cx, "请求发送中…", "Sending…")),
                    ))
                })
                .child(widgets::editor_split(
                    theme.muted,
                    this.editor_layout == crate::theme::EditorLayout::Stacked,
                    if this.editor_layout == crate::theme::EditorLayout::Stacked {
                        "http-editor-split"
                    } else {
                        "http-editor-split-side"
                    },
                    widgets::pane_frame(theme.border, theme.background, editor),
                    response_panel,
                )),
        )
        .into_any_element()
}

fn section_label_with_count(
    section: RequestSection,
    tab_ix: usize,
    tabs: &[RequestTab],
    cx: &App,
) -> SharedString {
    let Some(tab) = tabs.get(tab_ix) else {
        return SharedString::from(section.label(cx));
    };
    let count = match section {
        RequestSection::Params => tab
            .params
            .iter()
            .filter(|row| row.enabled && !row.is_empty(cx))
            .count(),
        RequestSection::Headers => tab
            .headers
            .iter()
            .filter(|row| row.enabled && !row.is_empty(cx))
            .count(),
        RequestSection::Body => {
            // 有请求体时在标签上提示：表单按启用的字段计数，其它类型有内容记为 1。
            match tab
                .body_kind
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or("none")
            {
                "none" => 0,
                "form" | "form-data" => tab
                    .form_fields
                    .iter()
                    .filter(|row| row.enabled && !row.is_empty(cx))
                    .count(),
                _ => usize::from(!tab.body.read(cx).value().trim().is_empty()),
            }
        }
        RequestSection::Auth | RequestSection::Timeline => 0,
    };
    if count == 0 {
        SharedString::from(section.label(cx))
    } else {
        SharedString::from(format!("{} {}", section.label(cx), count))
    }
}

/// 键值编辑表（参数 / 请求头 / 表单字段）。
pub(crate) fn kv_editor(
    this: &mut AppView,
    active: usize,
    kind: KvKind,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let form_files = kind == KvKind::Form
        && this
            .tabs
            .get(active)
            .and_then(|tab| tab.body_kind.read(cx).selected_value().copied())
            == Some("form-data");
    let rows: Vec<KvRowView> = {
        let Some(tab) = this.tabs.get(active) else {
            return div().into_any_element();
        };
        let rows = match kind {
            KvKind::Params => &tab.params,
            KvKind::Headers => &tab.headers,
            KvKind::Form => &tab.form_fields,
        };
        rows.iter()
            .map(|row| KvRowView {
                key: row.key.clone(),
                value: row.value.clone(),
                enabled: row.enabled,
                file: row.file,
                file_path: row.file_path.clone(),
            })
            .collect()
    };
    let empty_hint = match kind {
        KvKind::Headers => tr!(
            cx,
            "还没有请求头。点下面的按钮添加一行。",
            "No headers yet. Add a row below."
        ),
        KvKind::Params => tr!(
            cx,
            "还没有查询参数。点下面的按钮添加一行。",
            "No query parameters yet. Add a row below."
        ),
        KvKind::Form => tr!(
            cx,
            "还没有表单字段。点下面的按钮添加一行。",
            "No form fields yet. Add a row below."
        ),
    };
    let add_label = match kind {
        KvKind::Headers => tr!(cx, "添加请求头", "Add header"),
        KvKind::Params => tr!(cx, "添加参数", "Add parameter"),
        KvKind::Form => tr!(cx, "添加字段", "Add field"),
    };

    v_flex()
        .gap_1()
        .child(if rows.is_empty() {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(empty_hint)
                .into_any_element()
        } else {
            v_flex()
                .gap_1()
                .children(rows.into_iter().enumerate().map(|(ix, row)| {
                    let KvRowView {
                        key,
                        value,
                        enabled,
                        file,
                        file_path,
                    } = row;
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(kv_toggle(active, ix, kind, enabled, &theme, cx))
                        .child(div().w(px(200.)).child(Input::new(&key).small()))
                        .when(form_files, |row| {
                            row.child(form_mode_button(active, ix, file, cx))
                        })
                        .child(if form_files && file {
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .text_ellipsis()
                                        .text_color(theme.muted_foreground)
                                        .child(file_path.unwrap_or_else(|| {
                                            tr!(cx, "未选择文件", "No file chosen").to_string()
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "kv-file-{}-{ix}",
                                        kind_tag(kind)
                                    )))
                                    .outline()
                                    .small()
                                    .label(tr!(cx, "选择文件", "Choose file"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.pick_form_file(active, ix, window, cx);
                                    })),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex_1()
                                .child(Input::new(&value).small())
                                .into_any_element()
                        })
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "kv-remove-{}-{ix}",
                                    kind_tag(kind)
                                )))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size_5()
                                .rounded_sm()
                                .hover(|this| this.bg(theme.muted))
                                .cursor_pointer()
                                .child(Icon::new(IconName::Delete).xsmall())
                                .on_click(cx.listener(
                                    move |this, _: &gpui_kit::ClickEvent, _, cx| {
                                        this.remove_kv_row(active, ix, kind, cx);
                                    },
                                )),
                        )
                        .into_any_element()
                }))
                .into_any_element()
        })
        .child(
            Button::new(SharedString::from(format!("add-kv-{}", kind_tag(kind))))
                .outline()
                .small()
                .icon(IconName::Plus)
                .label(add_label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_kv_row(active, kind, window, cx);
                })),
        )
        .into_any_element()
}

fn draft_button(
    id: &'static str,
    label: &'static str,
    protocol: Protocol,
    primary: bool,
    cx: &mut Context<AppView>,
) -> Button {
    let button = Button::new(id)
        .label(label)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.new_draft_tab_for(protocol, window, cx);
        }));
    if primary {
        button.primary()
    } else {
        button.outline()
    }
}

fn kind_tag(kind: KvKind) -> &'static str {
    match kind {
        KvKind::Params => "params",
        KvKind::Headers => "headers",
        KvKind::Form => "form",
    }
}

fn kv_toggle(
    active: usize,
    ix: usize,
    kind: KvKind,
    enabled: bool,
    theme: &gpui_kit::component::Theme,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let check_icon = if enabled {
        Some(
            Icon::new(IconName::Check)
                .xsmall()
                .text_color(theme.primary_foreground),
        )
    } else {
        None
    };
    div()
        .id(SharedString::from(format!(
            "kv-toggle-{}-{ix}",
            kind_tag(kind)
        )))
        .flex()
        .items_center()
        .justify_center()
        .size_5()
        .rounded_sm()
        .when(enabled, |this| this.bg(theme.primary))
        .when(!enabled, |this| this.border_1().border_color(theme.border))
        .children(check_icon)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &gpui_kit::ClickEvent, _, cx| {
            this.toggle_kv_row(active, ix, kind, cx);
        }))
}

fn form_mode_button(
    active: usize,
    ix: usize,
    file: bool,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    Button::new(SharedString::from(format!("kv-mode-{ix}")))
        .small()
        .outline()
        .label(if file {
            tr!(cx, "文件", "File")
        } else {
            tr!(cx, "文本", "Text")
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_form_file_mode(active, ix, !file, cx);
        }))
}

/// 认证：继承集合、无、Bearer、Basic、API 密钥。
fn auth_editor(this: &mut AppView, active: usize, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(tab) = this.tabs.get(active) else {
        return div().into_any_element();
    };
    let kind = tab.auth_kind.clone();
    let auth_in = tab.auth_in.clone();
    let token = tab.auth_token.clone();
    let username = tab.auth_username.clone();
    let password = tab.auth_password.clone();
    let key = tab.auth_key.clone();
    let value = tab.auth_value.clone();
    let choices = [
        ("inherit", tr!(cx, "继承集合", "Inherit")),
        ("none", tr!(cx, "无", "None")),
        ("bearer", tr!(cx, "Bearer", "Bearer")),
        ("basic", tr!(cx, "Basic", "Basic")),
        ("apiKey", tr!(cx, "API 密钥", "API key")),
    ];

    v_flex()
        .size_full()
        .gap_3()
        .id("request-auth")
        .overflow_y_scroll()
        .child(h_flex().gap_1().children(choices.map(|(id, label)| {
            let selected = kind == id;
            let button = Button::new(SharedString::from(format!("auth-kind-{id}")))
                .small()
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_auth_kind(active, id, cx);
                }));
            if selected {
                button.primary()
            } else {
                button.ghost()
            }
        })))
        .child(match kind.as_str() {
            "bearer" => labeled_input(cx, tr!(cx, "令牌", "Token"), &token),
            "basic" => v_flex()
                .gap_2()
                .child(labeled_input(cx, tr!(cx, "用户名", "Username"), &username))
                .child(labeled_input(cx, tr!(cx, "密码", "Password"), &password))
                .into_any_element(),
            "apiKey" => v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_1()
                        .children(["header", "query"].map(|location| {
                            let selected = auth_in == location;
                            let label = if location == "header" {
                                tr!(cx, "请求头", "Header")
                            } else {
                                tr!(cx, "查询参数", "Query")
                            };
                            let button =
                                Button::new(SharedString::from(format!("auth-in-{location}")))
                                    .small()
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_auth_in(active, location, cx);
                                    }));
                            if selected {
                                button.primary()
                            } else {
                                button.ghost()
                            }
                        })),
                )
                .child(labeled_input(cx, tr!(cx, "键", "Key"), &key))
                .child(labeled_input(cx, tr!(cx, "值", "Value"), &value))
                .into_any_element(),
            "none" => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(
                    cx,
                    "这个请求不附带认证。",
                    "This request sends no auth."
                ))
                .into_any_element(),
            _ => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(
                    cx,
                    "发送时使用所在集合的认证。",
                    "Sending uses the collection's auth."
                ))
                .into_any_element(),
        })
        .into_any_element()
}

fn labeled_input(cx: &App, label: &'static str, input: &Entity<InputState>) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(Input::new(input).small())
        .into_any_element()
}

fn pretty_json(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

fn compact_json(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    serde_json::to_string(&value).ok()
}

/// 请求体编辑器为空时的占位提示，随 body 类型切换。
fn body_placeholder(kind: &str, cx: &App) -> SharedString {
    match kind {
        "json" => "{\"name\": \"value\"}".into(),
        "raw" => tr!(cx, "输入原始文本内容", "Raw text content").into(),
        _ => SharedString::default(),
    }
}

/// Body 子标签的显示名。
fn body_kind_label(kind: &'static str, cx: &App) -> &'static str {
    match kind {
        "none" => tr!(cx, "无", "None"),
        "json" => "JSON",
        "raw" => tr!(cx, "文本", "Raw"),
        "form" => tr!(cx, "表单", "Form"),
        "form-data" => tr!(cx, "表单数据", "Form data"),
        other => other,
    }
}

pub(crate) fn response_from_history(detail: &HistoryDetail) -> Option<HttpResponse> {
    let snap = detail.response_snapshot.as_ref()?;
    let timing = snap
        .get("timing")
        .and_then(|value| serde_json::from_value::<Timing>(value.clone()).ok())
        .unwrap_or_default();
    let headers = snap
        .get("headers")
        .and_then(|value| {
            serde_json::from_value::<BTreeMap<String, Vec<String>>>(value.clone()).ok()
        })
        .unwrap_or_default();
    let body = detail.response_body.clone().or_else(|| {
        snap.get("body")
            .and_then(|value| value.as_str().map(str::to_string))
    });
    let unresolved = snap
        .get("unresolvedVariables")
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
        .unwrap_or_default();
    Some(HttpResponse {
        task_id: detail.summary.id.clone(),
        url: snap
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        status_code: snap
            .get("statusCode")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u16,
        status_text: snap
            .get("statusText")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        headers,
        body,
        body_file_path: detail.response_body_file_path.clone(),
        body_size: snap
            .get("bodySize")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        content_type: snap
            .get("contentType")
            .and_then(|value| value.as_str().map(str::to_string)),
        timing,
        redirect_count: snap
            .get("redirectCount")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize,
        unresolved_variables: unresolved,
    })
}

/// 请求体编辑器 + 类型选择。
fn body_editor(
    this: &mut AppView,
    active: usize,
    body_kind: &Entity<SelectState<Vec<&'static str>>>,
    body: &Entity<EditorState>,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let kind = body_kind
        .read(cx)
        .selected_value()
        .copied()
        .unwrap_or("none");

    // 类型分段按钮在左，JSON 的格式化/压缩动作靠右，两组不混在一起。
    let kind_tabs = h_flex()
        .gap_0p5()
        .items_center()
        .children(BODY_KINDS.map(|candidate| {
            let selected = kind == candidate;
            let button = Button::new(SharedString::from(format!("body-kind-{candidate}")))
                .small()
                .label(body_kind_label(candidate, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_body_kind(active, candidate, window, cx);
                }));
            if selected {
                button.primary()
            } else {
                button.ghost()
            }
        }));

    v_flex()
        .size_full()
        .min_h_0()
        .gap_2()
        .child(
            h_flex()
                .flex_shrink_0()
                .justify_between()
                .items_center()
                .child(kind_tabs)
                .when(kind == "json", |row| {
                    row.child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("format-json")
                                    .ghost()
                                    .small()
                                    .label(tr!(cx, "格式化", "Format"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.format_json_body(active, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("compact-json")
                                    .ghost()
                                    .small()
                                    .label(tr!(cx, "压缩", "Compact"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.compact_json_body(active, window, cx);
                                    })),
                            ),
                    )
                }),
        )
        .child(match kind {
            "none" => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .py_4()
                .child(tr!(cx, "该请求没有请求体。", "This request has no body."))
                .into_any_element(),
            "form" | "form-data" => div()
                .flex_1()
                .min_h_0()
                .id("request-form")
                .overflow_y_scroll()
                .child(kv_editor(this, active, KvKind::Form, cx))
                .into_any_element(),
            _ => div()
                .flex_1()
                .min_h(px(120.))
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .child(Editor::new(body).h_full())
                .into_any_element(),
        })
        .when(kind == "json", |column| {
            column.child(json_status_line(&body.read(cx).value(), cx))
        })
        .into_any_element()
}

/// 请求体下方的一行状态：JSON 是否合法、顶层有几项。空内容不提示。
fn json_status_line(text: &str, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return div().into_any_element();
    }
    let parsed = serde_json::from_str::<serde_json::Value>(trimmed);
    let (line, color) = match &parsed {
        Ok(serde_json::Value::Object(map)) => (
            tr!(
                cx,
                format!("合法 JSON · 对象，{} 个键", map.len()),
                format!("Valid JSON · object with {} keys", map.len())
            )
            .to_string(),
            theme.success,
        ),
        Ok(serde_json::Value::Array(items)) => (
            tr!(
                cx,
                format!("合法 JSON · 数组，{} 项", items.len()),
                format!("Valid JSON · array with {} items", items.len())
            )
            .to_string(),
            theme.success,
        ),
        Ok(_) => (
            tr!(cx, "合法 JSON", "Valid JSON").to_string(),
            theme.success,
        ),
        Err(error) => (
            tr!(
                cx,
                format!("JSON 语法错误：{error}"),
                format!("JSON syntax error: {error}")
            )
            .to_string(),
            theme.danger,
        ),
    };
    div()
        .flex_shrink_0()
        .text_xs()
        .text_color(color)
        .child(line)
        .into_any_element()
}

impl AppView {
    pub fn add_kv_row(
        &mut self,
        tab_ix: usize,
        kind: KvKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = KvRow::new(window, cx, "", "", true);
        {
            let Some(tab) = self.tabs.get_mut(tab_ix) else {
                return;
            };
            match kind {
                KvKind::Params => tab.params.push(row),
                KvKind::Headers => tab.headers.push(row),
                KvKind::Form => tab.form_fields.push(row),
            }
            tab.dirty = true;
        }
        watch_tab_edits(self, tab_ix, window, cx);
        cx.notify();
    }

    pub fn remove_kv_row(
        &mut self,
        tab_ix: usize,
        row_ix: usize,
        kind: KvKind,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        let rows = match kind {
            KvKind::Params => &mut tab.params,
            KvKind::Headers => &mut tab.headers,
            KvKind::Form => &mut tab.form_fields,
        };
        if row_ix < rows.len() {
            rows.remove(row_ix);
            tab.dirty = true;
            cx.notify();
        }
    }

    pub fn toggle_kv_row(
        &mut self,
        tab_ix: usize,
        row_ix: usize,
        kind: KvKind,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        let rows = match kind {
            KvKind::Params => &mut tab.params,
            KvKind::Headers => &mut tab.headers,
            KvKind::Form => &mut tab.form_fields,
        };
        if let Some(row) = rows.get_mut(row_ix) {
            row.enabled = !row.enabled;
            tab.dirty = true;
            cx.notify();
        }
    }

    pub fn set_auth_kind(&mut self, tab_ix: usize, kind: &'static str, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        if tab.auth_kind == kind {
            return;
        }
        tab.auth_kind = kind.to_string();
        tab.dirty = true;
        cx.notify();
    }

    pub fn set_auth_in(&mut self, tab_ix: usize, location: &'static str, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        if tab.auth_in == location {
            return;
        }
        tab.auth_in = location.to_string();
        tab.dirty = true;
        cx.notify();
    }

    /// 切换请求体类型（Body 子标签）。select 是数据源，程序化设置不触发
    /// Confirm 事件，所以标脏和占位提示在这里一并处理。
    pub fn set_body_kind(
        &mut self,
        tab_ix: usize,
        kind: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = BODY_KINDS.iter().position(|candidate| *candidate == kind) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        if tab.body_kind.read(cx).selected_value().copied() == Some(kind) {
            return;
        }
        let select = tab.body_kind.clone();
        let body = tab.body.clone();
        tab.dirty = true;
        select.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::default().row(index)), window, cx);
        });
        sync_body_placeholder(&body, kind, window, cx);
        cx.notify();
    }

    pub fn set_form_file_mode(
        &mut self,
        tab_ix: usize,
        row_ix: usize,
        file: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        let Some(row) = tab.form_fields.get_mut(row_ix) else {
            return;
        };
        row.file = file;
        if !file {
            row.file_path = None;
        }
        tab.dirty = true;
        cx.notify();
    }

    pub fn pick_form_file(
        &mut self,
        tab_ix: usize,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr!(cx, "选择文件", "Choose a file").into()),
        });
        cx.spawn(async move |view, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = path.to_string_lossy().into_owned();
            view.update_in(cx, |view, window, cx| {
                let Some(tab) = view.tabs.get_mut(tab_ix) else {
                    return;
                };
                let Some(row) = tab.form_fields.get_mut(row_ix) else {
                    return;
                };
                row.file = true;
                row.file_path = Some(text.clone());
                let input = row.value.clone();
                input.update(cx, |state, cx| state.set_value(text, window, cx));
                tab.dirty = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn format_json_body(&mut self, tab_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.rewrite_json_body(tab_ix, pretty_json, window, cx);
    }

    /// 压缩成单行 JSON，方便复制粘贴。
    pub fn compact_json_body(
        &mut self,
        tab_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rewrite_json_body(tab_ix, compact_json, window, cx);
    }

    /// 用 `transform` 重写请求体；解析失败时在请求区显示错误，不改动原文。
    fn rewrite_json_body(
        &mut self,
        tab_ix: usize,
        transform: fn(&str) -> Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let raw = tab.body.read(cx).value().to_string();
        let rewritten = match transform(&raw) {
            Some(text) => text,
            None => {
                let message = tr!(
                    cx,
                    "JSON 无法处理，请先改成合法的 JSON。",
                    "Could not process JSON. Fix the syntax first."
                )
                .to_string();
                if let Some(tab) = self.tabs.get_mut(tab_ix) {
                    tab.error = Some(message);
                }
                cx.notify();
                return;
            }
        };
        let body = tab.body.clone();
        body.update(cx, |state, cx| state.set_value(rewritten, window, cx));
        if let Some(tab) = self.tabs.get_mut(tab_ix) {
            tab.error = None;
            tab.dirty = true;
        }
        cx.notify();
    }

    /// 发送请求：tokio 上执行，回填响应。
    pub fn send_request(&mut self, tab_ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        if tab.sending {
            return;
        }
        if tab.protocol != Protocol::Http {
            protocols::send(self, tab_ix, cx);
            return;
        }
        let config = tab.collect_config(cx);
        let request_path = tab.path.clone();
        let values = self.current_variables(cx);
        let recorded = crate::views::url_vars::concrete_config(&config, &values);
        let task_id = Uuid::new_v4().to_string();

        tracing::info!(
            method = %config.method,
            url = %config.url,
            "http request started"
        );
        let payload = thread_core::HttpSendPayload {
            task_id: task_id.clone(),
            request_path,
            config,
            environment_path: None,
            save_history: true,
        };

        let started_at = thread_core::db::now_ms();
        {
            let tab = &mut self.tabs[tab_ix];
            tab.sending = true;
            tab.task_id = Some(task_id);
            tab.error = None;
            tab.response = None;
            tab.started_at = Some(started_at);
        }
        cx.notify();

        let core = self.core.clone();
        let receiver = self
            .core
            .spawn(async move { core.send_http(payload).await });

        cx.spawn(async move |this, cx| {
            let result: thread_core::AppResult<thread_core::services::http::HttpResponse> =
                match receiver.await {
                    Ok(outcome) => outcome,
                    Err(error) => Err(AppError::Internal(error.to_string())),
                };
            let _ = this.update_in(cx, |view, window, cx| {
                let saved = {
                    let Some(tab) = view.tabs.get_mut(tab_ix) else {
                        return;
                    };
                    if tab.task_id.is_none() {
                        return; // 标签页已被替换或取消重置
                    }
                    tab.sending = false;
                    match result {
                        Ok(response) => {
                            tracing::info!(
                                status = response.status_code,
                                elapsed_ms = response.timing.total_ms,
                                "http request finished"
                            );
                            let run = crate::app::LocalRun {
                                id: Uuid::new_v4().to_string(),
                                at: thread_core::db::now_ms(),
                                status: response.status_code,
                                duration_ms: response.timing.total_ms.round() as i64,
                                url: response.url.clone(),
                                request: Some(recorded),
                                response: Some(response.clone()),
                            };
                            let saved = tab.path.is_some();
                            tab.timeline_request = run.request.clone();
                            tab.timeline_wire = false;
                            if saved {
                                tab.timeline_selected = None;
                            } else {
                                tab.timeline_selected = Some(run.id.clone());
                                tab.local_runs.insert(0, run);
                            }
                            tab.response = Some(response);
                            response::auto_select_section(tab);
                            saved
                        }
                        Err(error) => {
                            tracing::warn!(%error, "http request failed");
                            tab.error = Some(error.to_string());
                            window.push_notification(
                                (NotificationType::Error, error.to_string()),
                                cx,
                            );
                            false
                        }
                    }
                };
                if saved {
                    view.reload_request_timeline(tab_ix, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn cancel_request(&mut self, tab_ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let Some(task_id) = tab.task_id.clone() else {
            return;
        };
        if self.core.cancel_http(&task_id) {
            cx.notify();
        }
    }

    /// 保存已存在的请求文件。
    pub fn save_request(&mut self, tab_ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let Some(path) = tab.path.clone() else {
            return;
        };
        let document = tab.to_document(cx);
        if let Some(tab) = self.tabs.get_mut(tab_ix) {
            tab.dirty = false;
        }
        let locale = crate::i18n::current(cx);
        tracing::info!(path = %path, "request saved");
        self.workspace.update(cx, |model, cx| {
            model.run(cx, move |root| {
                thread_core::workspace::ops::update_request_document(&root, &path, document).map(
                    |_| {
                        crate::i18n::owned(
                            locale,
                            "请求已保存。".to_string(),
                            "Request saved.".to_string(),
                        )
                    },
                )
            });
        });
        cx.notify();
    }

    /// 把临时请求保存为工作区根目录下的新文件（当前版本无弹窗，使用请求名）。
    pub fn save_request_as(&mut self, tab_ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let document = tab.to_document(cx);
        let name = tab.name.clone();
        let protocol = tab.protocol.file_protocol();
        let tab_id = tab.id.clone();

        let core = self.core.clone();
        let receiver = core.spawn(async move {
            thread_core::workspace::ops::create_request(&root, "", &name, protocol)
        });
        cx.spawn(async move |this, cx| {
            let created = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| match created {
                Ok((path, _)) => {
                    let locale = crate::i18n::current(cx);
                    view.workspace.update(cx, |model, cx| {
                        let document = document.clone();
                        let path = path.clone();
                        model.run(cx, move |root| {
                            thread_core::workspace::ops::update_request_document(
                                &root, &path, document,
                            )
                            .map(|_| {
                                crate::i18n::owned(
                                    locale,
                                    format!("已保存到 {path}"),
                                    format!("Saved to {path}"),
                                )
                            })
                        });
                    });
                    tracing::info!(%path, "request saved as");
                    if let Some(tab) = view.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                        tab.path = Some(path);
                        tab.dirty = false;
                    }
                    if view.close_after_save.as_deref() == Some(tab_id.as_str()) {
                        view.close_after_save = None;
                        if let Some(index) = view.tabs.iter().position(|tab| tab.id == tab_id) {
                            view.finish_close_tab(index, cx);
                        }
                    }
                    cx.notify();
                }
                Err(error) => {
                    view.close_after_save = None;
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }
}

/// 订阅这个标签上的输入。用户改了内容就标成未保存，不会自动写盘。
pub fn watch_tab_edits(
    this: &mut AppView,
    tab_ix: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let mut inputs = vec![
        tab.url.clone(),
        tab.service.clone(),
        tab.rpc_method.clone(),
        tab.operation_name.clone(),
        tab.message.clone(),
        tab.auth_token.clone(),
        tab.auth_username.clone(),
        tab.auth_password.clone(),
        tab.auth_key.clone(),
        tab.auth_value.clone(),
    ];
    for row in tab.kv_rows() {
        inputs.push(row.key.clone());
        inputs.push(row.value.clone());
    }
    let editors = [tab.body.clone(), tab.variables.clone()];
    let selects = [tab.method.clone(), tab.body_kind.clone()];

    let mut watches = Vec::new();
    for input in inputs {
        watches.push(
            cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    mark_dirty(this, cx, |tab| tab.owns_input(input));
                }
            }),
        );
    }
    for editor in editors {
        watches.push(cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    mark_dirty(this, cx, |tab| tab.owns_editor(editor));
                }
            },
        ));
    }
    for select in selects {
        watches.push(cx.subscribe_in(
            &select,
            window,
            |this, select, event: &SelectEvent<Vec<&'static str>>, window, cx| {
                if !matches!(event, SelectEvent::Confirm(_)) {
                    return;
                }
                mark_dirty(this, cx, |tab| tab.owns_select(select));
                // 切换请求体类型时同步占位提示。按实体找标签页，不依赖可能已变动的下标。
                if let Some(tab) = this.tabs.iter().find(|tab| tab.body_kind == *select) {
                    let kind = select.read(cx).selected_value().copied().unwrap_or("none");
                    sync_body_placeholder(&tab.body, kind, window, cx);
                }
            },
        ));
    }
    if let Some(tab) = this.tabs.get_mut(tab_ix) {
        tab.edit_watches = watches;
    }
}

/// 切换 body 类型后，编辑器的占位提示跟着变。
fn sync_body_placeholder(
    body: &Entity<EditorState>,
    kind: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let placeholder = body_placeholder(kind, cx);
    body.update(cx, |state, cx| {
        state.set_placeholder(placeholder, window, cx)
    });
}

/// 找到拥有该控件的标签页并标记为未保存。已经是脏的标签页不重复通知。
fn mark_dirty(this: &mut AppView, cx: &mut Context<AppView>, owns: impl Fn(&RequestTab) -> bool) {
    let Some(tab) = this.tabs.iter_mut().find(|tab| owns(tab)) else {
        return;
    };
    if tab.dirty {
        return;
    }
    tab.dirty = true;
    cx.notify();
}

impl RequestTab {
    /// 所有键值行：参数、请求头、表单字段。
    fn kv_rows(&self) -> impl Iterator<Item = &KvRow> {
        self.params
            .iter()
            .chain(self.headers.iter())
            .chain(self.form_fields.iter())
    }

    /// 这个输入框是否属于本标签页。
    fn owns_input(&self, input: &Entity<InputState>) -> bool {
        self.url == *input
            || self.service == *input
            || self.rpc_method == *input
            || self.operation_name == *input
            || self.message == *input
            || self.auth_token == *input
            || self.auth_username == *input
            || self.auth_password == *input
            || self.auth_key == *input
            || self.auth_value == *input
            || self
                .kv_rows()
                .any(|row| row.key == *input || row.value == *input)
    }

    /// 这个多行编辑器是否属于本标签页。
    fn owns_editor(&self, editor: &Entity<EditorState>) -> bool {
        self.body == *editor || self.variables == *editor
    }

    /// 这个下拉框是否属于本标签页。
    fn owns_select(&self, select: &Entity<SelectState<Vec<&'static str>>>) -> bool {
        self.method == *select || self.body_kind == *select
    }
}
