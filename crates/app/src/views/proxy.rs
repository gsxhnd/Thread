//! 代理视图：启停控制 + 抓包设置 + 流量列表 + 条目详情。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::input::Input;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;
use thread_core::services::proxy::CaptureSettings;

use crate::app::AppView;
use crate::format::{format_bytes, format_duration, format_relative_time, status_color};
use crate::widgets;

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = crate::format::SemanticColors::for_theme(cx.theme());
    let running = this.proxy.status.running;
    let paused = this.proxy.status.paused;
    let port_entity = this.proxy.port.clone();
    let status_text = if running {
        let port = this.proxy.status.port.unwrap_or(0);
        let count = this.proxy.entries.len();
        tr!(
            cx,
            format!("监听中 127.0.0.1:{port} · {count} 条流量"),
            format!("Listening on 127.0.0.1:{port} · {count} flows")
        )
    } else {
        tr!(cx, "未运行", "Stopped").to_string()
    };
    let persist = this.proxy.settings.persist_sessions;
    let mask = this.proxy.settings.mask_sensitive;
    let decrypt = this.proxy.settings.decrypt_https;
    let filter_host = this.proxy.filter_host.clone();
    let filter_method = this.proxy.filter_method.clone();
    let filter_status = this.proxy.filter_status.clone();
    let max_entries = this.proxy.max_entries.clone();
    let max_body_kb = this.proxy.max_body_kb.clone();
    let host_filter = filter_host.read(cx).value().to_string();
    let method_filter = filter_method.read(cx).value().to_string();
    let status_filter = filter_status.read(cx).value().to_string();
    let entries: Vec<_> = this
        .proxy
        .entries
        .iter()
        .filter(|entry| {
            thread_core::services::proxy::matches_traffic_filter(
                entry,
                &host_filter,
                &method_filter,
                &status_filter,
            )
        })
        .cloned()
        .collect();
    let entry_count = entries.len();
    let total_count = this.proxy.entries.len();
    let selected_id = this
        .proxy
        .selected
        .as_ref()
        .map(|detail| detail.summary.id.clone());

    // 列表展示为倒序（最新在上）。
    let rows: Vec<AnyElement> = entries
        .iter()
        .rev()
        .enumerate()
        .map(|(ix, entry)| {
            let status_color = status_color(entry.status_code.map(|code| code as i64), &colors);
            let label = format!("{}{}", entry.host, entry.path);
            let entry_id = entry.id.clone();
            let selected = selected_id.as_deref() == Some(entry.id.as_str());
            h_flex()
                .id(("proxy-entry", ix))
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .when(selected, |this| this.bg(theme.accent))
                .when(!selected, |this| this.hover(|this| this.bg(theme.muted)))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .w(px(44.))
                        .flex_shrink_0()
                        .child(entry.method.clone()),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().text_xs().text_ellipsis().child(label))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .text_ellipsis()
                                .child(if entry.decrypted {
                                    tr!(cx, "HTTPS 已解密", "HTTPS decrypted").to_string()
                                } else {
                                    match entry.mode.as_str() {
                                        "tunnel" => tr!(
                                            cx,
                                            "CONNECT 隧道（未解密）",
                                            "CONNECT tunnel (not decrypted)"
                                        )
                                        .to_string(),
                                        _ => tr!(cx, "HTTP 明文", "Plain HTTP").to_string(),
                                    }
                                }),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .w(px(36.))
                        .flex_shrink_0()
                        .text_color(status_color)
                        .child(if let Some(code) = entry.status_code {
                            code.to_string()
                        } else if entry.error.is_some() {
                            "ERR".to_string()
                        } else {
                            "…".to_string()
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .flex_shrink_0()
                        .child(format_duration(entry.duration_ms as f64)),
                )
                .on_click(
                    cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                        this.open_proxy_detail(entry_id.clone(), window, cx);
                    }),
                )
                .into_any_element()
        })
        .collect();

    v_flex()
        .flex_1()
        .min_h_0()
        .gap_3()
        .px_4()
        .py_3()
        .child(
            // 控制行
            h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .gap_y_1()
                .child(if running {
                    Button::new("proxy-stop")
                        .outline()
                        .icon(IconName::Square)
                        .label(tr!(cx, "停止代理", "Stop proxy"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.stop_proxy(window, cx);
                        }))
                } else {
                    Button::new("proxy-start")
                        .primary()
                        .icon(IconName::Play)
                        .label(tr!(cx, "启动代理", "Start proxy"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_proxy(window, cx);
                        }))
                })
                .when(running, |this| {
                    this.child(if paused {
                        Button::new("proxy-resume")
                            .outline()
                            .small()
                            .label(tr!(cx, "继续记录", "Resume"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_proxy_paused(false, window, cx);
                            }))
                    } else {
                        Button::new("proxy-pause")
                            .outline()
                            .small()
                            .label(tr!(cx, "暂停记录", "Pause"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_proxy_paused(true, window, cx);
                            }))
                    })
                })
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(tr!(cx, "端口", "Port")),
                        )
                        .child(
                            div()
                                .w(px(80.))
                                .child(Input::new(&port_entity).small().disabled(running)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if running {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(status_text),
                ),
        )
        .child(
            // 抓包设置
            widgets::section(
                cx,
                tr!(cx, "抓包偏好", "Capture"),
                tr!(
                    cx,
                    "保存在 config.toml（应用级）。HTTPS 解密需要先在系统里信任 Thread CA。",
                    "Stored in config.toml. Decrypting HTTPS requires trusting the Thread CA on this system."
                ),
                v_flex()
                    .gap_2()
                    .child(setting_switch(
                        "persist-sessions",
                        tr!(cx, "持久化会话", "Persist sessions"),
                        tr!(
                            cx,
                            "把会话与条目写入 SQLite；关闭时流量只保留在内存中。",
                            "Write sessions and entries to SQLite. When off, traffic stays in memory."
                        ),
                        persist,
                        cx.listener(|this, checked: &bool, window, cx| {
                            this.proxy.settings.persist_sessions = *checked;
                            this.save_proxy_settings(window, cx);
                        }),
                    ))
                    .child(setting_switch(
                        "mask-sensitive",
                        tr!(cx, "隐藏敏感字段", "Mask sensitive fields"),
                        tr!(
                            cx,
                            "遮盖 Authorization / Cookie 等请求头与敏感查询参数。",
                            "Hide Authorization, Cookie, and sensitive query parameters."
                        ),
                        mask,
                        cx.listener(|this, checked: &bool, window, cx| {
                            this.proxy.settings.mask_sensitive = *checked;
                            this.save_proxy_settings(window, cx);
                        }),
                    ))
                    .child(setting_switch(
                        "decrypt-https",
                        tr!(cx, "解密 HTTPS（MITM）", "Decrypt HTTPS (MITM)"),
                        tr!(
                            cx,
                            "要求已在操作系统或浏览器中信任 Thread CA，且在证书页确认信任。未信任时 CONNECT 只做隧道转发，并标明内容不可查看。",
                            "Requires the Thread CA to be trusted, and trust confirmed on the Certificates page. Otherwise CONNECT stays a tunnel and the body cannot be viewed."
                        ),
                        decrypt,
                        cx.listener(|this, checked: &bool, window, cx| {
                            this.proxy.settings.decrypt_https = *checked;
                            this.save_proxy_settings(window, cx);
                        }),
                    ))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(limit_input(tr!(cx, "最大条数", "Max entries"), &max_entries))
                            .child(limit_input(
                                tr!(cx, "单条 Body 上限 KB", "Max body KB"),
                                &max_body_kb,
                            ))
                            .child(
                                Button::new("apply-limits")
                                    .outline()
                                    .small()
                                    .label(tr!(cx, "应用限额", "Apply limits"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_capture_limits(cx);
                                        this.save_proxy_settings(window, cx);
                                    })),
                            ),
                    ),
            ),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(filter_input(&filter_host))
                .child(filter_input(&filter_method))
                .child(filter_input(&filter_status)),
        )
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(
                            cx,
                            format!(
                                "显示 {entry_count} / {total_count} 条（列表最多保留 2000 条）"
                            ),
                            format!("Showing {entry_count} / {total_count} (list keeps 2000)")
                        )),
                )
                .child(
                    Button::new("proxy-clear")
                        .outline()
                        .small()
                        .label(tr!(cx, "清空列表", "Clear list"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_proxy_entries(cx);
                        })),
                ),
        )
        .child(
            widgets::horizontal_split(
                "proxy-split",
                460.,
                v_flex()
                    .size_full()
                    .min_h_0()
                    .id("proxy-entries")
                    .overflow_y_scroll()
                    .gap_0p5()
                    .pr_2()
                    .when(rows.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .py_4()
                                .child(tr!(
                                    cx,
                                    "还没有流量。把 HTTP 客户端指向本机代理即可抓包。",
                                    "No traffic yet. Point an HTTP client at this proxy to capture it."
                                )),
                        )
                    })
                    .children(rows)
                    .into_any_element(),
                div()
                    .size_full()
                    .min_h_0()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .p_3()
                    .child(detail_view(this, cx))
                    .into_any_element(),
            ),
        )
        .into_any_element()
}

fn detail_view(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(detail) = &this.proxy.selected else {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(
                        cx,
                        "从左侧选择一条流量查看请求与响应。",
                        "Select a flow to see the request and response."
                    )),
            )
            .into_any_element();
    };

    let request_body = body_text(cx, &detail.request_body);
    let response_body = body_text(cx, &detail.response_body);
    // JSON 响应体与响应面板保持一致：默认美化展示，复制按钮复制展示文本。
    let response_display = pretty_if_json(&response_body, detail.summary.content_type.as_deref());

    v_flex()
        .id("proxy-detail")
        .overflow_y_scroll()
        .size_full()
        .gap_3()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(widgets::StatusTag::new(
                    detail.summary.status_code.unwrap_or(0),
                    detail.summary.status_text.as_deref(),
                    cx,
                ))
                .child(widgets::mini_tag(
                    cx,
                    format!("↑ {}", format_bytes(detail.summary.request_body_size)),
                ))
                .child(widgets::mini_tag(
                    cx,
                    format!("↓ {}", format_bytes(detail.summary.response_body_size)),
                ))
                .child(widgets::mini_tag(
                    cx,
                    format_duration(detail.summary.duration_ms as f64),
                ))
                .child(widgets::mini_tag(
                    cx,
                    format_relative_time(detail.summary.captured_at, cx),
                )),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_ellipsis_start()
                        .child(detail.summary.url.clone()),
                )
                .child(
                    Clipboard::new("copy-proxy-url")
                        .small()
                        .value(detail.summary.url.clone())
                        .tooltip(tr!(cx, "复制 URL", "Copy URL")),
                ),
        )
        .when(
            !detail.summary.decrypted && detail.summary.mode == "tunnel",
            |parent| {
                let note = detail.summary.note.clone().unwrap_or_else(|| {
                    tr!(
                        cx,
                        "未解密的 CONNECT 隧道，请求与响应内容不可查看。",
                        "Undecrypted CONNECT tunnel. The request and response cannot be viewed."
                    )
                    .into()
                });
                parent.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            },
        )
        .child(header_table(
            cx,
            tr!(cx, "请求头", "Request headers"),
            &detail.request_headers,
            &theme,
        ))
        .child(header_table(
            cx,
            tr!(cx, "响应头", "Response headers"),
            &detail.response_headers,
            &theme,
        ))
        .when(!request_body.is_empty(), |this| {
            this.child(body_block(
                cx,
                tr!(cx, "请求体", "Request body"),
                &request_body,
                &theme,
            ))
        })
        .when(!response_body.is_empty(), |this| {
            this.child(body_block(
                cx,
                tr!(cx, "响应体", "Response body"),
                &response_display,
                &theme,
            ))
        })
        .into_any_element()
}

fn header_table(
    cx: &App,
    title: &str,
    pairs: &[thread_core::services::proxy::capture::HeaderPair],
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(if pairs.is_empty() {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(cx, "（无）", "(none)"))
                .into_any_element()
        } else {
            v_flex()
                .gap_0p5()
                .children(pairs.iter().map(|pair| {
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .child(
                            div()
                                .w(px(200.))
                                .flex_shrink_0()
                                .text_color(theme.muted_foreground)
                                .text_ellipsis()
                                .child(pair.key.clone()),
                        )
                        .child(div().flex_1().text_ellipsis().child(pair.value.clone()))
                }))
                .into_any_element()
        })
        .into_any_element()
}

fn body_block(cx: &App, title: &str, text: &str, theme: &gpui_kit::component::Theme) -> AnyElement {
    let mono = theme.mono_font_family.clone();
    let text = text.to_string();
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .when(!text.is_empty(), |this| {
                    this.child(
                        Clipboard::new(SharedString::from(format!("copy-proxy-body-{title}")))
                            .small()
                            .value(text.clone())
                            .tooltip(tr!(cx, "复制", "Copy")),
                    )
                }),
        )
        .child(
            div()
                .id(SharedString::from(format!("proxy-body-{title}")))
                .max_h(px(200.))
                .overflow_y_scroll()
                .rounded_md()
                .bg(theme.muted)
                .p_2()
                .text_xs()
                .font_family(mono)
                .child(text),
        )
        .into_any_element()
}

/// Content-Type 是 JSON 时美化展示；解析失败原样返回。
fn pretty_if_json(text: &str, content_type: Option<&str>) -> String {
    let looks_json = text.starts_with('{') || text.starts_with('[');
    if !looks_json || !content_type.map(|ct| ct.contains("json")).unwrap_or(false) {
        return text.to_string();
    }
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_string())
}

fn filter_input(
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
) -> impl IntoElement {
    div().w(px(140.)).child(Input::new(input).small())
}

fn limit_input(
    label: &'static str,
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
) -> impl IntoElement {
    h_flex()
        .gap_1()
        .items_center()
        .child(div().text_xs().child(label))
        .child(div().w(px(88.)).child(Input::new(input).small()))
}

fn setting_switch(
    id: &'static str,
    label: &str,
    description: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    setting_switch_inner(id, label, description, checked, on_click)
}

fn setting_switch_inner(
    id: &'static str,
    label: &str,
    description: &str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    // 说明与标签在闭包外解析不了主题色，这里只搭结构。
    h_flex()
        .id(SharedString::from(format!("setting-{id}")))
        .items_center()
        .justify_between()
        .gap_3()
        .py_1()
        .child(
            v_flex()
                .gap_0p5()
                .child(div().text_sm().child(label.to_string()))
                .child(div().text_xs().child(description.to_string())),
        )
        .child(Switch::new(id).checked(checked).on_click(on_click))
}

impl AppView {
    pub fn refresh_proxy_status(&mut self, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let task_core = self.core.clone();
        let receiver = core.spawn(async move { task_core.proxy.status().await });
        cx.spawn(async move |this, cx| {
            if let Ok(status) = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
            {
                let _ = this.update(cx, |view, cx| {
                    view.proxy.status = status;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn start_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_capture_limits(cx);
        self.save_proxy_settings(window, cx);
        let port: u16 = self
            .proxy
            .port
            .read(cx)
            .value()
            .trim()
            .parse()
            .unwrap_or(9090);
        let workspace_path = self
            .workspace
            .read(cx)
            .root()
            .map(|root| root.to_string_lossy().into_owned());
        let settings = self.proxy.settings.clone();
        let core = self.core.clone();
        let task_core2 = self.core.clone();
        tracing::info!(port, "starting proxy");
        let receiver =
            core.spawn(async move { task_core2.proxy.start(port, workspace_path, settings).await });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| {
                match result {
                    Ok(status) => {
                        let bound = status.port;
                        tracing::info!(port = bound.unwrap_or(port), "proxy started");
                        view.proxy.status = status;
                        view.proxy.entries.clear();
                        view.proxy.selected = None;
                        window.push_notification(
                            (
                                NotificationType::Success,
                                tr!(
                                    cx,
                                    format!("代理已启动：127.0.0.1:{}", bound.unwrap_or(port)),
                                    format!(
                                        "Proxy listening on 127.0.0.1:{}",
                                        bound.unwrap_or(port)
                                    )
                                ),
                            ),
                            cx,
                        );
                    }
                    Err(error) => {
                        tracing::warn!(%error, "proxy failed to start");
                        window.push_notification((NotificationType::Error, error.to_string()), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn stop_proxy(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("stopping proxy");
        let core = self.core.clone();
        let task_core = self.core.clone();
        let receiver = core.spawn(async move { task_core.proxy.stop().await });
        cx.spawn(async move |this, cx| {
            if let Ok(status) = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
            {
                let _ = this.update(cx, |view, cx| {
                    view.proxy.status = status;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn set_proxy_paused(&mut self, paused: bool, _: &mut Window, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let task_core = self.core.clone();
        let receiver = core.spawn(async move { task_core.proxy.set_paused(paused).await });
        cx.spawn(async move |this, cx| {
            if let Ok(status) = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome)
            {
                let _ = this.update(cx, |view, cx| {
                    view.proxy.status = status;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn save_proxy_settings(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let settings: CaptureSettings = self.proxy.settings.clone();
        if let Err(error) = self.core.config.set_proxy_capture_settings(&settings) {
            tracing::warn!(?error, "failed to save capture settings");
        }
        cx.notify();
    }

    pub fn apply_capture_limits(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.proxy.settings.clone();
        if let Ok(value) = self
            .proxy
            .max_entries
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
        {
            settings.max_entries = value;
        }
        if let Ok(kilobytes) = self
            .proxy
            .max_body_kb
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
        {
            settings.max_body_bytes = kilobytes.saturating_mul(1024);
        }
        self.proxy.settings = settings.normalized();
    }

    pub fn clear_proxy_entries(&mut self, cx: &mut Context<Self>) {
        let session = self
            .proxy
            .status
            .session_id
            .clone()
            .or_else(|| self.proxy.last_session_id.clone());
        self.proxy.entries.clear();
        self.proxy.selected = None;
        cx.notify();
        let Some(session) = session else {
            return;
        };
        let proxy = self.core.proxy.clone();
        let _receiver = self.core.spawn(async move { proxy.clear(&session) });
    }

    pub fn open_proxy_detail(&mut self, id: String, _: &mut Window, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let task_core = self.core.clone();
        let receiver = core.spawn(async move { task_core.proxy.entry(&id) });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update(cx, |view, cx| {
                view.proxy.selected = result.ok().flatten();
                cx.notify();
            });
        })
        .detach();
    }
}

/// 把代理 Body 视图转成可展示文本。
fn body_text(cx: &App, body: &thread_core::services::proxy::capture::BodyView) -> String {
    if body.content.is_empty() {
        return String::new();
    }
    if body.encoding == "text" {
        body.content.clone()
    } else {
        let bytes = body.total_bytes;
        let encoded = &body.content;
        tr!(
            cx,
            format!("（二进制，base64，{bytes} 字节）\n{encoded}"),
            format!("(binary, base64, {bytes} bytes)\n{encoded}")
        )
    }
}
