//! GraphQL、WebSocket、gRPC 的编辑与发送。MQTT 只展示文件内容。

use std::collections::BTreeMap;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement as _, PathPromptOptions,
    SharedString, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;
use thread_core::services::graphql::{
    GraphqlRequest, INTROSPECTION_QUERY, complete_fields, graphql_operation,
    schema_from_introspection,
};
use thread_core::services::grpc::{GrpcInvoke, ServiceInfo};
use thread_core::services::http::{HttpResponse, Timing};
use thread_core::services::websocket::{WsConnect, WsOutgoing, WsSink};
use thread_core::workspace::formats::AuthSpec;

use crate::app::AppView;
use crate::views::request::{self, EditorPane, KvKind, Protocol};
use crate::views::response;
use crate::widgets;

pub fn body(
    this: &mut AppView,
    active: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    match this.tabs[active].protocol {
        Protocol::Graphql => graphql_body(this, active, window, cx),
        Protocol::Websocket => websocket_body(this, active, cx),
        Protocol::Grpc => grpc_body(this, active, window, cx),
        Protocol::Mqtt => mqtt_body(this, active, cx),
        Protocol::Http => div().into_any_element(),
    }
}

/// 请求视图的「发送」在非 HTTP 标签上转到这里。
pub fn send(this: &mut AppView, tab_ix: usize, cx: &mut Context<AppView>) {
    match this.tabs.get(tab_ix).map(|tab| tab.protocol) {
        Some(Protocol::Graphql) => send_graphql(this, tab_ix, false, cx),
        Some(Protocol::Websocket) => toggle_websocket(this, tab_ix, cx),
        Some(Protocol::Grpc) => call_grpc(this, tab_ix, cx),
        _ => {}
    }
}

fn graphql_body(
    this: &mut AppView,
    active: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let (url, body, variables, pane, connected, error, has_path) = {
        let tab = &this.tabs[active];
        (
            tab.url.clone(),
            tab.body.clone(),
            tab.variables.clone(),
            tab.pane,
            tab.connection_id.is_some(),
            tab.error.clone(),
            tab.path.is_some(),
        )
    };
    let suggestions = this.tabs[active].suggestions.clone();

    v_flex()
        .flex_1()
        .min_h_0()
        .child(widgets::band(
            theme.border,
            theme.background,
            toolbar(
                this,
                active,
                &url,
                if connected {
                    LiveAction::Stop
                } else {
                    LiveAction::Graphql
                },
                if connected {
                    tr!(cx, "停止订阅", "Stop subscription")
                } else {
                    tr!(cx, "发送", "Send")
                },
                has_path,
                cx,
            ),
        ))
        .child(widgets::band(
            theme.border,
            theme.background,
            h_flex().gap_2().child(
                Button::new("gql-schema")
                    .outline()
                    .small()
                    .icon(IconName::Search)
                    .label(tr!(cx, "获取 Schema 并补全", "Fetch schema"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        send_graphql(this, active, true, cx);
                    })),
            ),
        ))
        .when_some(error, |this, error| {
            this.child(widgets::band(
                theme.border,
                theme.background,
                crate::widgets::notice_line(cx, error),
            ))
        })
        .child(pane_bar(
            this,
            active,
            pane,
            &[
                (EditorPane::Primary, tr!(cx, "查询", "Query")),
                (EditorPane::Secondary, tr!(cx, "变量", "Variables")),
                (EditorPane::Headers, tr!(cx, "请求头", "Headers")),
            ],
            cx,
        ))
        .when(
            !suggestions.is_empty() && pane == EditorPane::Primary,
            |parent| parent.child(suggestion_row(this, active, &suggestions, window, cx)),
        )
        .child(widgets::editor_split(
            theme.muted,
            this.editor_layout == crate::theme::EditorLayout::Stacked,
            if this.editor_layout == crate::theme::EditorLayout::Stacked {
                "gql-editor-split"
            } else {
                "gql-editor-split-side"
            },
            filling_editor(match pane {
                EditorPane::Primary => editor_box(&body, &theme),
                EditorPane::Secondary => editor_box(&variables, &theme),
                EditorPane::Headers => widgets::pane_frame(
                    theme.border,
                    theme.background,
                    request::kv_editor(this, active, KvKind::Headers, cx),
                ),
            }),
            activity_pane(this, active, window, cx),
        ))
        .into_any_element()
}

fn websocket_body(this: &mut AppView, active: usize, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let (url, message, pane, connected, reconnect, binary, error, has_path) = {
        let tab = &this.tabs[active];
        (
            tab.url.clone(),
            tab.message.clone(),
            tab.pane,
            tab.connection_id.is_some(),
            tab.auto_reconnect,
            tab.binary_message,
            tab.error.clone(),
            tab.path.is_some(),
        )
    };

    v_flex()
        .flex_1()
        .min_h_0()
        .child(widgets::band(
            theme.border,
            theme.background,
            toolbar(
                this,
                active,
                &url,
                LiveAction::Websocket,
                if connected {
                    tr!(cx, "断开", "Disconnect")
                } else {
                    tr!(cx, "连接", "Connect")
                },
                has_path,
                cx,
            ),
        ))
        .child(widgets::band(
            theme.border,
            theme.background,
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    Switch::new("ws-reconnect")
                        .checked(reconnect)
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            if let Some(tab) = this.tabs.get_mut(active) {
                                tab.auto_reconnect = *checked;
                                tab.dirty = true;
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(cx, "断开后自动重连", "Reconnect after disconnect")),
                )
                .child(
                    Switch::new("ws-binary")
                        .checked(binary)
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            if let Some(tab) = this.tabs.get_mut(active) {
                                tab.binary_message = *checked;
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(cx, "按二进制发送（Base64）", "Send as binary (Base64)")),
                ),
        ))
        .when_some(error, |this, error| {
            this.child(widgets::band(
                theme.border,
                theme.background,
                crate::widgets::notice_line(cx, error),
            ))
        })
        .child(pane_bar(
            this,
            active,
            pane,
            &[
                (EditorPane::Primary, tr!(cx, "消息", "Message")),
                (EditorPane::Headers, tr!(cx, "请求头", "Headers")),
            ],
            cx,
        ))
        .child(widgets::editor_split(
            theme.muted,
            this.editor_layout == crate::theme::EditorLayout::Stacked,
            if this.editor_layout == crate::theme::EditorLayout::Stacked {
                "ws-editor-split"
            } else {
                "ws-editor-split-side"
            },
            match pane {
                EditorPane::Headers => filling_editor(widgets::pane_frame(
                    theme.border,
                    theme.background,
                    request::kv_editor(this, active, KvKind::Headers, cx),
                )),
                _ => h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&message).small()))
                    .child(
                        Button::new("ws-send")
                            .outline()
                            .small()
                            .label(tr!(cx, "发送", "Send"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                send_ws_message(this, active, window, cx);
                            })),
                    )
                    .into_any_element(),
            },
            Some(session_log(this, active, true, cx)),
        ))
        .into_any_element()
}

fn grpc_body(
    this: &mut AppView,
    active: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let (url, service, method, body, pane, tls, connected, error, has_path, streaming) = {
        let tab = &this.tabs[active];
        let service_name = tab.service.read(cx).value().to_string();
        let method_name = tab.rpc_method.read(cx).value().to_string();
        let streaming = method_streams(&tab.services, &service_name, &method_name);
        (
            tab.url.clone(),
            tab.service.clone(),
            tab.rpc_method.clone(),
            tab.body.clone(),
            tab.pane,
            tab.tls,
            tab.connection_id.is_some(),
            tab.error.clone(),
            tab.path.is_some(),
            streaming,
        )
    };
    let discovered = this.tabs[active].services.clone();

    v_flex()
        .flex_1()
        .min_h_0()
        .child(widgets::band(
            theme.border,
            theme.background,
            toolbar(
                this,
                active,
                &url,
                if connected {
                    LiveAction::Stop
                } else {
                    LiveAction::Grpc
                },
                if connected {
                    tr!(cx, "取消流", "Cancel stream")
                } else if streaming {
                    tr!(cx, "打开流", "Open stream")
                } else {
                    tr!(cx, "调用", "Call")
                },
                has_path,
                cx,
            ),
        ))
        .child(widgets::band(
            theme.border,
            theme.background,
            h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .child(Switch::new("grpc-tls").checked(tls).on_click(cx.listener(
                    move |this, checked: &bool, _, cx| {
                        if let Some(tab) = this.tabs.get_mut(active) {
                            tab.tls = *checked;
                            tab.dirty = true;
                            cx.notify();
                        }
                    },
                )))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("TLS"),
                )
                .child(
                    Button::new("grpc-proto")
                        .outline()
                        .small()
                        .label(tr!(cx, "加载 .proto", "Load .proto"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            load_proto(this, active, window, cx);
                        })),
                )
                .child(
                    Button::new("grpc-reflect")
                        .outline()
                        .small()
                        .label(tr!(cx, "服务反射", "Server reflection"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            reflect_grpc(this, active, window, cx);
                        })),
                )
                .child(div().w(px(180.)).child(Input::new(&service).small()))
                .child(div().w(px(180.)).child(Input::new(&method).small())),
        ))
        .when(streaming && connected, |parent| {
            parent.child(widgets::band(
                theme.border,
                theme.background,
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("grpc-more")
                            .outline()
                            .small()
                            .label(tr!(cx, "再发送一条", "Send another"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                send_grpc_more(this, active, window, cx);
                            })),
                    )
                    .child(
                        Button::new("grpc-half-close")
                            .outline()
                            .small()
                            .label(tr!(cx, "结束发送", "Finish sending"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                close_grpc_send(this, active, window, cx);
                            })),
                    ),
            ))
        })
        .when_some(error, |this, error| {
            this.child(widgets::band(
                theme.border,
                theme.background,
                crate::widgets::notice_line(cx, error),
            ))
        })
        .when(!discovered.is_empty(), |parent| {
            parent.child(widgets::band(
                theme.border,
                theme.background,
                method_list(this, active, &discovered, window, cx),
            ))
        })
        .child(pane_bar(
            this,
            active,
            pane,
            &[
                (EditorPane::Primary, tr!(cx, "载荷", "Payload")),
                (EditorPane::Headers, "Metadata"),
            ],
            cx,
        ))
        .child(widgets::editor_split(
            theme.muted,
            this.editor_layout == crate::theme::EditorLayout::Stacked,
            if this.editor_layout == crate::theme::EditorLayout::Stacked {
                "grpc-editor-split"
            } else {
                "grpc-editor-split-side"
            },
            filling_editor(match pane {
                EditorPane::Headers => widgets::pane_frame(
                    theme.border,
                    theme.background,
                    request::kv_editor(this, active, KvKind::Headers, cx),
                ),
                _ => editor_box(&body, &theme),
            }),
            activity_pane(this, active, window, cx),
        ))
        .into_any_element()
}

fn mqtt_body(this: &mut AppView, active: usize, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let broker = this.tabs[active].url.read(cx).value().to_string();
    v_flex()
        .flex_1()
        .gap_2()
        .child(crate::widgets::notice_line(
            cx,
            tr!(
                cx,
                "MQTT 不在当前版本里。文件会保留，但不会连接 Broker。",
                "MQTT is not in this version. The file is kept, but Thread will not connect to a broker."
            ),
        ))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(format!("Broker：{broker}")),
        )
        .into_any_element()
}

#[derive(Clone, Copy)]
enum LiveAction {
    Graphql,
    Websocket,
    Grpc,
    Stop,
}

fn toolbar(
    this: &mut AppView,
    active: usize,
    url: &Entity<InputState>,
    action: LiveAction,
    label: &'static str,
    has_path: bool,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .items_start()
        .child(crate::views::url_vars::field(this, active, url, cx))
        .child(
            Button::new("proto-action")
                .primary()
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| match action {
                    LiveAction::Graphql => send_graphql(this, active, false, cx),
                    LiveAction::Websocket => toggle_websocket(this, active, cx),
                    LiveAction::Grpc => call_grpc(this, active, cx),
                    LiveAction::Stop => {
                        this.stop_live(active);
                        cx.notify();
                    }
                })),
        )
        .child(save_button(active, has_path, cx))
}

fn save_button(active: usize, has_path: bool, cx: &mut Context<AppView>) -> impl IntoElement {
    if has_path {
        Button::new("proto-save")
            .outline()
            .label(tr!(cx, "保存", "Save"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.save_request(active, window, cx);
            }))
    } else {
        Button::new("proto-save-as")
            .outline()
            .label(tr!(cx, "保存到工作区", "Save to workspace"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.save_request_as(active, window, cx);
            }))
    }
}

fn pane_bar(
    _this: &mut AppView,
    active: usize,
    selected: EditorPane,
    panes: &[(EditorPane, &'static str)],
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    h_flex()
        .flex_shrink_0()
        .px_3()
        .gap_1()
        .border_b_1()
        .border_color(theme.border)
        .children(panes.iter().map(|(pane, label)| {
            let pane = *pane;
            let on = selected == pane;
            div()
                .id(SharedString::from(format!("pane-{label}")))
                .px_2()
                .py_1p5()
                .text_xs()
                .border_b_2()
                .cursor_pointer()
                .when(on, |this| {
                    this.border_color(theme.primary)
                        .text_color(theme.foreground)
                })
                .when(!on, |this| {
                    this.border_color(gpui_kit::transparent_black())
                        .text_color(theme.muted_foreground)
                        .hover(|this| this.text_color(theme.foreground))
                })
                .child(*label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tabs.get_mut(active) {
                        tab.pane = pane;
                        cx.notify();
                    }
                }))
        }))
}

fn editor_box(editor: &Entity<EditorState>, theme: &gpui_kit::component::Theme) -> AnyElement {
    div()
        .size_full()
        .min_h(px(120.))
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(Editor::new(editor).h_full())
        .into_any_element()
}

fn filling_editor(editor: AnyElement) -> AnyElement {
    div().size_full().min_h_0().child(editor).into_any_element()
}

fn activity_pane(
    this: &mut AppView,
    active: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Option<AnyElement> {
    let has_log = !this.tabs[active].log.is_empty();
    let log = if has_log {
        Some(session_log(this, active, false, cx))
    } else {
        None
    };
    let response = response::render(this, active, window, cx);
    Some(
        v_flex()
            .size_full()
            .min_h_0()
            .gap_2()
            .when_some(log, |parent, log| parent.child(log))
            .child(response)
            .into_any_element(),
    )
}

fn suggestion_row(
    _this: &mut AppView,
    active: usize,
    suggestions: &[thread_core::services::graphql::FieldCompletion],
    _window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    h_flex()
        .gap_1()
        .flex_wrap()
        .children(suggestions.iter().enumerate().map(|(ix, item)| {
            let insert = item.insert.clone();
            let label = format!("{} {}", item.label, item.detail);
            div()
                .id(("suggest", ix))
                .px_2()
                .py_0p5()
                .text_xs()
                .rounded_sm()
                .bg(theme.muted)
                .cursor_pointer()
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    insert_completion(this, active, &insert, window, cx);
                }))
        }))
}

fn method_list(
    _this: &mut AppView,
    active: usize,
    services: &[ServiceInfo],
    _window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let mut rows = Vec::new();
    for service in services {
        for method in &service.methods {
            let service_name = service.name.clone();
            let method_name = method.name.clone();
            let template = method.request_template.clone();
            let kind = stream_label(cx, method.client_streaming, method.server_streaming);
            rows.push(
                div()
                    .id(SharedString::from(format!(
                        "rpc-{service_name}-{method_name}"
                    )))
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|this| this.bg(theme.muted))
                    .child(format!("{service_name}/{method_name}  {kind}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let Some(tab) = this.tabs.get(active) else {
                            return;
                        };
                        let service = tab.service.clone();
                        let method = tab.rpc_method.clone();
                        let body = tab.body.clone();
                        let service_name = service_name.clone();
                        let method_name = method_name.clone();
                        let template = template.clone();
                        service.update(cx, |state, cx| {
                            state.set_value(service_name, window, cx);
                        });
                        method.update(cx, |state, cx| {
                            state.set_value(method_name, window, cx);
                        });
                        if body.read(cx).value().trim().is_empty() && !template.is_empty() {
                            body.update(cx, |state, cx| {
                                state.set_value(template, window, cx);
                            });
                        }
                        if let Some(tab) = this.tabs.get_mut(active) {
                            tab.dirty = true;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
    }
    v_flex()
        .id("grpc-methods")
        .max_h(px(140.))
        .overflow_y_scroll()
        .gap_0p5()
        .children(rows)
}

fn session_log(
    this: &mut AppView,
    active: usize,
    placeholder: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let log = this.tabs[active].log.clone();
    if log.is_empty() {
        if !placeholder {
            return div().into_any_element();
        }
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(tr!(
                cx,
                "连接之后，收发的消息会显示在这里。",
                "Messages show up here after you connect."
            ))
            .into_any_element();
    }
    v_flex()
        .id("session-log")
        .flex_1()
        .min_h(px(80.))
        .size_full()
        .overflow_y_scroll()
        .gap_0p5()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .p_2()
        .children(log.iter().map(|line| {
            div()
                .text_xs()
                .child(format!("{}  {}", line.kind, line.text))
        }))
        .into_any_element()
}

fn send_graphql(this: &mut AppView, tab_ix: usize, introspect: bool, cx: &mut Context<AppView>) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let endpoint = tab.url.read(cx).value().to_string();
    let query = if introspect {
        INTROSPECTION_QUERY.to_string()
    } else {
        tab.body.read(cx).value().to_string()
    };
    let variables = tab.variables.read(cx).value().to_string();
    let operation_name = {
        let name = tab.operation_name.read(cx).value().trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    };
    let headers = tab
        .headers
        .iter()
        .filter(|row| row.enabled)
        .map(|row| row.to_kv(cx))
        .collect();
    let operation = graphql_operation(&query);
    if !introspect && operation == "subscription" {
        subscribe_graphql(
            this,
            tab_ix,
            endpoint,
            query,
            variables,
            operation_name,
            headers,
            cx,
        );
        return;
    }

    if let Some(tab) = this.tabs.get_mut(tab_ix) {
        tab.sending = true;
        tab.error = None;
        cx.notify();
    }

    let core = this.core.clone();
    let task_core = this.core.clone();
    let receiver = core.spawn(async move {
        let variables_map = load_variables(&task_core);
        let timeout = Duration::from_millis(task_core.config.app_settings().default_timeout_ms);
        thread_core::services::graphql::send(GraphqlRequest {
            endpoint: endpoint.clone(),
            query,
            variables,
            operation_name,
            headers,
            auth: AuthSpec::default(),
            timeout,
            verify_ssl: true,
            variables_map,
        })
        .await
        .map(|response| (endpoint, response))
    });

    cx.spawn(async move |view, cx| {
        let result = receiver
            .await
            .map_err(|error| AppError::Internal(error.to_string()))
            .and_then(|outcome| outcome);
        let _ = view.update_in(cx, |view, window, cx| {
            let Some(tab) = view.tabs.get_mut(tab_ix) else {
                return;
            };
            tab.sending = false;
            match result {
                Ok((url, response)) => {
                    if introspect {
                        match schema_from_introspection(&response.body) {
                            Ok(schema) => {
                                let query = tab.body.read(cx).value().to_string();
                                tab.suggestions = complete_fields(&schema, &query);
                                tab.schema = Some(schema);
                                tab.error = None;
                                window.push_notification(
                                    (
                                        NotificationType::Success,
                                        tr!(
                                            cx,
                                            "Schema 已更新，可点选字段补全。",
                                            "Schema updated. Pick a field to insert it."
                                        ),
                                    ),
                                    cx,
                                );
                            }
                            Err(error) => {
                                tab.error = Some(error.to_string());
                                window.push_notification(
                                    (NotificationType::Error, error.to_string()),
                                    cx,
                                );
                            }
                        }
                    }
                    crate::views::history::push_local_run(
                        tab,
                        response.status_code,
                        response.duration_ms.round() as i64,
                        &url,
                    );
                    tab.response = Some(graphql_as_http(&url, &response));
                    response::auto_select_section(tab);
                }
                Err(error) => {
                    tab.error = Some(error.to_string());
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn subscribe_graphql(
    this: &mut AppView,
    tab_ix: usize,
    endpoint: String,
    query: String,
    variables: String,
    operation_name: Option<String>,
    headers: Vec<thread_core::workspace::formats::KeyValue>,
    cx: &mut Context<AppView>,
) {
    let sender = this.core.live_sender();
    let sink = std::sync::Arc::new(move |event| {
        let _ = sender.send(thread_core::LiveEvent::Graphql(event));
    });
    let core = this.core.clone();
    let registry = this.core.graphql.clone();
    let result = {
        let variables_map = load_variables(&core);
        let timeout = Duration::from_millis(core.config.app_settings().default_timeout_ms);
        registry.subscribe(
            GraphqlRequest {
                endpoint,
                query,
                variables,
                operation_name,
                headers,
                auth: AuthSpec::default(),
                timeout,
                verify_ssl: true,
                variables_map,
            },
            sink,
        )
    };
    match result {
        Ok(id) => {
            if let Some(tab) = this.tabs.get_mut(tab_ix) {
                tab.connection_id = Some(id);
                tab.error = None;
                tab.log.clear();
            }
        }
        Err(error) => {
            if let Some(tab) = this.tabs.get_mut(tab_ix) {
                tab.error = Some(error.to_string());
            }
        }
    }
    cx.notify();
}

fn toggle_websocket(this: &mut AppView, tab_ix: usize, cx: &mut Context<AppView>) {
    if this
        .tabs
        .get(tab_ix)
        .is_some_and(|tab| tab.connection_id.is_some())
    {
        this.stop_live(tab_ix);
        cx.notify();
        return;
    }
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let url = tab.url.read(cx).value().to_string();
    let headers = tab
        .headers
        .iter()
        .filter(|row| row.enabled && !row.is_empty(cx))
        .map(|row| {
            (
                row.key.read(cx).value().to_string(),
                row.value.read(cx).value().to_string(),
            )
        })
        .collect();
    let auto_reconnect = tab.auto_reconnect;
    let protocols = tab.ws_protocols.clone();
    let sender = this.core.live_sender();
    let sender_status = sender.clone();
    let sink = WsSink::new(
        move |event| {
            let _ = sender.send(thread_core::LiveEvent::WsMessage(event));
        },
        move |event| {
            let _ = sender_status.send(thread_core::LiveEvent::WsStatus(event));
        },
    );
    match this.core.websockets.connect(
        WsConnect {
            url,
            headers,
            protocols,
            auto_reconnect,
        },
        sink,
    ) {
        Ok(id) => {
            if let Some(tab) = this.tabs.get_mut(tab_ix) {
                tab.connection_id = Some(id);
                tab.error = None;
                tab.log.clear();
                tab.sending = true;
            }
        }
        Err(error) => {
            if let Some(tab) = this.tabs.get_mut(tab_ix) {
                tab.error = Some(error.to_string());
            }
        }
    }
    cx.notify();
}

fn send_ws_message(
    this: &mut AppView,
    tab_ix: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let Some(id) = tab.connection_id.clone() else {
        return;
    };
    let text = tab.message.read(cx).value().to_string();
    let binary = tab.binary_message;
    let message = if binary {
        match base64_decode(cx, &text) {
            Ok(bytes) => WsOutgoing::Binary(bytes),
            Err(error) => {
                window.push_notification((NotificationType::Error, error), cx);
                return;
            }
        }
    } else {
        WsOutgoing::Text(text)
    };
    if let Err(error) = this.core.websockets.send(&id, message) {
        window.push_notification((NotificationType::Error, error.to_string()), cx);
    }
}

fn load_proto(this: &mut AppView, tab_ix: usize, _window: &mut Window, cx: &mut Context<AppView>) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(tr!(cx, "选择 .proto 文件", "Choose a .proto file").into()),
    });
    let protos = this.core.protos.clone();
    let core = this.core.clone();
    cx.spawn(async move |view, cx| {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        let load_path = path.clone();
        let loaded = core.spawn(async move { protos.load_file(&load_path) });
        let result = loaded
            .await
            .map_err(|error| AppError::Internal(error.to_string()))
            .and_then(|outcome| outcome);
        let path_text = path.display().to_string();
        let _ = view.update_in(cx, |view, window, cx| {
            apply_proto_definition(view, tab_ix, result, Some(path_text), window, cx);
        });
    })
    .detach();
}

fn reflect_grpc(
    this: &mut AppView,
    tab_ix: usize,
    _window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let host = tab.url.read(cx).value().to_string();
    let tls = tab.tls;
    let metadata = metadata_pairs(tab, cx);
    let protos = this.core.protos.clone();
    let core = this.core.clone();
    let receiver = core.spawn(async move { protos.reflect(&host, tls, &metadata).await });
    cx.spawn(async move |view, cx| {
        let result = receiver
            .await
            .map_err(|error| AppError::Internal(error.to_string()))
            .and_then(|outcome| outcome);
        let _ = view.update_in(cx, |view, window, cx| {
            apply_proto_definition(view, tab_ix, result, None, window, cx);
        });
    })
    .detach();
}

fn apply_proto_definition(
    view: &mut AppView,
    tab_ix: usize,
    result: Result<thread_core::services::grpc::ProtoDefinition, AppError>,
    proto_path: Option<String>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = view.tabs.get_mut(tab_ix) else {
        return;
    };
    match result {
        Ok(definition) => {
            tab.pool_id = Some(definition.pool_id);
            tab.services = definition.services;
            tab.error = None;
            if proto_path.is_some() {
                tab.proto_source = "file".into();
                tab.proto_file_path = proto_path;
            } else {
                tab.proto_source = "reflection".into();
            }
            window.push_notification(
                (
                    NotificationType::Success,
                    tr!(cx, "服务描述已加载。", "Service description loaded."),
                ),
                cx,
            );
        }
        Err(error) => {
            tab.error = Some(error.to_string());
            window.push_notification((NotificationType::Error, error.to_string()), cx);
        }
    }
    cx.notify();
}

fn call_grpc(this: &mut AppView, tab_ix: usize, cx: &mut Context<AppView>) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let Some(pool_id) = tab.pool_id.clone() else {
        if let Some(tab) = this.tabs.get_mut(tab_ix) {
            tab.error = Some(
                tr!(
                    cx,
                    "先加载 .proto，或对服务器做反射。",
                    "Load a .proto file, or use server reflection."
                )
                .into(),
            );
            cx.notify();
        }
        return;
    };
    let host = tab.url.read(cx).value().to_string();
    let service = tab.service.read(cx).value().to_string();
    let method = tab.rpc_method.read(cx).value().to_string();
    let payload = tab.body.read(cx).value().to_string();
    let tls = tab.tls;
    let metadata = metadata_pairs(tab, cx);
    let timeout_ms = tab.timeout_ms.max(1);
    let streaming = method_streams(&tab.services, &service, &method);
    let invoke = GrpcInvoke {
        host: host.clone(),
        tls,
        metadata,
        pool_id,
        service,
        method,
        payload,
        timeout: Duration::from_millis(timeout_ms),
    };
    if streaming {
        let sender = this.core.live_sender();
        let sink = std::sync::Arc::new(move |event| {
            let _ = sender.send(thread_core::LiveEvent::Grpc(event));
        });
        match this.core.protos.open_stream(invoke, sink) {
            Ok(id) => {
                if let Some(tab) = this.tabs.get_mut(tab_ix) {
                    tab.connection_id = Some(id);
                    tab.error = None;
                    tab.log.clear();
                }
            }
            Err(error) => {
                if let Some(tab) = this.tabs.get_mut(tab_ix) {
                    tab.error = Some(error.to_string());
                }
            }
        }
        cx.notify();
        return;
    }

    let protos = this.core.protos.clone();
    let core = this.core.clone();
    if let Some(tab) = this.tabs.get_mut(tab_ix) {
        tab.sending = true;
        tab.error = None;
    }
    let receiver = core.spawn(async move { protos.unary(&invoke).await });
    cx.spawn(async move |view, cx| {
        let result = receiver
            .await
            .map_err(|error| AppError::Internal(error.to_string()))
            .and_then(|outcome| outcome);
        let _ = view.update_in(cx, |view, window, cx| {
            let Some(tab) = view.tabs.get_mut(tab_ix) else {
                return;
            };
            tab.sending = false;
            match result {
                Ok(grpc) => {
                    let body = serde_json::to_string_pretty(&grpc.response)
                        .unwrap_or_else(|_| grpc.response.to_string());
                    crate::views::history::push_local_run(
                        tab,
                        200,
                        grpc.duration_ms.round() as i64,
                        &host,
                    );
                    tab.response = Some(text_response(&host, &body, grpc.duration_ms));
                    response::auto_select_section(tab);
                }
                Err(error) => {
                    tab.error = Some(error.to_string());
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn send_grpc_more(
    this: &mut AppView,
    tab_ix: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let Some(id) = tab.connection_id.clone() else {
        return;
    };
    let payload = tab.body.read(cx).value().to_string();
    if let Err(error) = this.core.protos.send_stream(&id, &payload) {
        window.push_notification((NotificationType::Error, error.to_string()), cx);
    }
}

fn close_grpc_send(
    this: &mut AppView,
    tab_ix: usize,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(id) = this
        .tabs
        .get(tab_ix)
        .and_then(|tab| tab.connection_id.clone())
    else {
        return;
    };
    if let Err(error) = this.core.protos.close_stream(&id) {
        window.push_notification((NotificationType::Error, error.to_string()), cx);
    }
}

fn insert_completion(
    this: &mut AppView,
    tab_ix: usize,
    insert: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(tab) = this.tabs.get(tab_ix) else {
        return;
    };
    let current = tab.body.read(cx).value().to_string();
    let next = apply_completion(&current, insert);
    let body = tab.body.clone();
    let schema = tab.schema.clone();
    body.update(cx, |state, cx| state.set_value(next.clone(), window, cx));
    if let Some(tab) = this.tabs.get_mut(tab_ix) {
        if let Some(schema) = schema {
            tab.suggestions = complete_fields(&schema, &next);
        }
        tab.dirty = true;
    }
    cx.notify();
}

fn apply_completion(query: &str, insert: &str) -> String {
    let trimmed = query.trim_end();
    let ident_at = trimmed
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_')
        .last()
        .map(|(index, _)| index);
    if let Some(index) = ident_at {
        let prefix = &trimmed[index..];
        if !prefix.is_empty() && insert.starts_with(prefix) {
            return format!("{}{insert}", &trimmed[..index]);
        }
    }
    if trimmed.is_empty() {
        insert.to_string()
    } else {
        format!("{trimmed} {insert}")
    }
}

fn metadata_pairs(tab: &crate::app::RequestTab, cx: &gpui_kit::App) -> Vec<(String, String)> {
    tab.headers
        .iter()
        .filter(|row| row.enabled && !row.is_empty(cx))
        .map(|row| {
            (
                row.key.read(cx).value().to_string(),
                row.value.read(cx).value().to_string(),
            )
        })
        .collect()
}

fn method_streams(services: &[ServiceInfo], service: &str, method: &str) -> bool {
    services.iter().any(|item| {
        item.name == service
            && item.methods.iter().any(|candidate| {
                candidate.name == method
                    && (candidate.client_streaming || candidate.server_streaming)
            })
    })
}

fn stream_label(cx: &App, client: bool, server: bool) -> &'static str {
    match (client, server) {
        (true, true) => tr!(cx, "双向流", "Bidi stream"),
        (true, false) => tr!(cx, "客户端流", "Client stream"),
        (false, true) => tr!(cx, "服务端流", "Server stream"),
        (false, false) => "Unary",
    }
}

fn load_variables(core: &thread_core::AppCore) -> BTreeMap<String, String> {
    let Ok(root) = core.workspace.root() else {
        return BTreeMap::new();
    };
    let active = core
        .workspace
        .config()
        .ok()
        .and_then(|config| config.active_environment);
    let rel = active
        .as_deref()
        .and_then(|value| thread_core::workspace::ops::resolve_environment_rel(&root, value));
    thread_core::workspace::ops::resolve_variables(&root, rel.as_deref()).unwrap_or_default()
}

fn graphql_as_http(
    url: &str,
    response: &thread_core::services::graphql::GraphqlResponse,
) -> HttpResponse {
    let mut http = text_response(url, &response.body, response.duration_ms);
    http.status_code = response.status_code;
    http.status_text = response.status_text.clone();
    http.headers = response.headers.clone();
    http.unresolved_variables = response.unresolved_variables.clone();
    http
}

fn text_response(url: &str, body: &str, duration_ms: f64) -> HttpResponse {
    HttpResponse {
        task_id: String::new(),
        url: url.to_string(),
        status_code: 200,
        status_text: "OK".into(),
        headers: BTreeMap::new(),
        body: Some(body.to_string()),
        body_file_path: None,
        body_size: body.len() as u64,
        content_type: Some("application/json".into()),
        timing: Timing {
            total_ms: duration_ms,
            ..Timing::default()
        },
        redirect_count: 0,
        unresolved_variables: Vec::new(),
    }
}

fn base64_decode(cx: &App, text: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|error| {
            tr!(
                cx,
                format!("二进制消息需要合法的 Base64：{error}"),
                format!("Binary messages need valid Base64: {error}")
            )
        })
}

impl AppView {
    pub fn stop_live(&mut self, tab_ix: usize) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let protocol = tab.protocol;
        let id = tab.connection_id.clone();
        if let Some(id) = id.as_deref() {
            match protocol {
                Protocol::Websocket => {
                    self.core.websockets.disconnect(id);
                }
                Protocol::Graphql => {
                    self.core.graphql.unsubscribe(id);
                }
                Protocol::Grpc => {
                    self.core.protos.cancel_stream(id);
                }
                Protocol::Http | Protocol::Mqtt => {}
            }
        }
        if let Some(tab) = self.tabs.get_mut(tab_ix) {
            tab.connection_id = None;
            tab.sending = false;
        }
    }
}
