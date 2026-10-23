//! 历史视图：请求历史列表 + 详情（保存在本地 SQLite，不进工作区）。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, FontWeight, IntoElement, ParentElement as _,
    PathPromptOptions, SharedString, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;
use thread_core::services::history::{self, HistoryDetail, HistorySummary};
use thread_core::services::http::HttpRequestConfig;
use thread_core::workspace::formats::{HttpBody, KeyValue};

use crate::app::AppView;
use crate::format::{format_duration, format_relative_time, method_color, status_color};
use crate::widgets;

/// 未命名请求，以及旧版本写死的「临时请求」，都按当前语言显示。
fn history_title(cx: &gpui_kit::App, name: Option<&str>) -> String {
    match name.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("临时请求") => tr!(cx, "临时请求", "Untitled request").to_string(),
        Some(name) => name.to_string(),
    }
}

/// 侧边栏里的历史列表。详情留在主区域。
pub fn sidebar(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = crate::format::SemanticColors::for_theme(cx.theme());
    if this.history.items.is_empty() {
        return div()
            .flex_1()
            .pt_8()
            .child(widgets::compact_empty(
                cx,
                IconName::Clock,
                tr!(cx, "还没有历史记录。", "No history yet."),
            ))
            .into_any_element();
    }

    let items: Vec<(usize, HistorySummary)> = this
        .history
        .items
        .iter()
        .enumerate()
        .map(|(ix, item)| (ix, item.clone()))
        .collect();
    let selected_id = this
        .history
        .selected
        .as_ref()
        .map(|detail| detail.summary.id.clone());

    div()
        .flex_1()
        .min_h_0()
        .id("history-list")
        .overflow_y_scroll()
        .px_1()
        .py_1()
        .child(
            v_flex()
                .gap_0p5()
                .children(items.into_iter().map(|(ix, item)| {
                    let method_color = method_color(item.method.as_deref(), &colors);
                    let status_color = status_color(item.status_code, &colors);
                    let selected = selected_id.as_deref() == Some(item.id.as_str());
                    h_flex()
                        .id(("history-item", ix))
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .when(selected, |this| this.bg(theme.accent))
                        .when(!selected, |this| this.hover(|this| this.bg(theme.muted)))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .w(px(48.))
                                .flex_shrink_0()
                                .text_color(method_color)
                                .child(item.method.clone().unwrap_or_else(|| "-".into())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .w(px(36.))
                                .flex_shrink_0()
                                .text_color(status_color)
                                .child(
                                    item.status_code
                                        .map(|code| code.to_string())
                                        .unwrap_or_else(|| "—".into()),
                                ),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_ellipsis()
                                        .child(history_title(cx, item.name.as_deref())),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .text_ellipsis()
                                                .child(item.url.clone().unwrap_or_default()),
                                        )
                                        .when_some(item.duration_ms, |row, ms| {
                                            row.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .flex_shrink_0()
                                                    .child(format_duration(ms as f64)),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .flex_shrink_0()
                                                .child(format_relative_time(item.created_at, cx)),
                                        ),
                                ),
                        )
                        .on_click(
                            cx.listener(move |this, _: &gpui_kit::ClickEvent, window, cx| {
                                this.open_history_detail(ix, window, cx);
                            }),
                        )
                        .into_any_element()
                })),
        )
        .into_any_element()
}

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();

    if this.history.items.is_empty() {
        return widgets::empty_state(
            cx,
            IconName::Clock,
            tr!(cx, "还没有历史记录", "No history yet"),
            tr!(
                cx,
                "每次发送的请求都会记录在这里（保存在本地数据目录，不进入工作区与 Git）。",
                "Each sent request is recorded here, in the local data folder, outside the workspace and Git."
            ),
        )
        .into_any_element();
    }

    let export = this.history.selected.is_some().then(|| {
        Button::new("export-selected-har")
            .outline()
            .small()
            .label(tr!(cx, "导出所选 HAR…", "Export selected HAR…"))
            .tooltip(tr!(
                cx,
                "HAR 含请求/响应正文、Headers 与 Cookie；分享前请检查。",
                "HAR may contain request/response bodies, headers and cookies; review before sharing."
            ))
            .on_click(cx.listener(|this, _, window, cx| {
                this.prompt_har_export(window, cx);
            }))
            .into_any_element()
    });

    v_flex()
        .flex_1()
        .min_h_0()
        .gap_3()
        .px_4()
        .py_3()
        .when_some(export, |column, export| {
            column.child(h_flex().justify_end().child(export))
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .p_3()
                .child(detail_view(this, cx)),
        )
        .into_any_element()
}

fn pretty_body(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| body.to_string())
}

fn detail_view(this: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(detail) = &this.history.selected else {
        return div()
            .size_full()
            .items_center()
            .justify_center()
            .flex()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(
                        cx,
                        "从左侧选择一条历史记录查看详情。",
                        "Select a history entry to see the details."
                    )),
            )
            .into_any_element();
    };

    let body = this.history.body.clone().unwrap_or_default();
    let mono = theme.mono_font_family.clone();

    v_flex()
        .id("history-detail")
        .overflow_y_scroll()
        .size_full()
        .gap_3()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(widgets::MethodTag::new(
                    detail.summary.method.as_deref().unwrap_or("GET"),
                    cx,
                ))
                .child(widgets::StatusTag::new(
                    detail.summary.status_code.unwrap_or(0) as u16,
                    None,
                    cx,
                ))
                .child(widgets::mini_tag(
                    cx,
                    format_duration(detail.summary.duration_ms.unwrap_or(0) as f64),
                ))
                .child(widgets::mini_tag(
                    cx,
                    format_relative_time(detail.summary.created_at, cx),
                )),
        )
        .when_some(
            detail.summary.url.clone().filter(|url| !url.is_empty()),
            |this, url| {
                this.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .text_ellipsis_start()
                                .child(url.clone()),
                        )
                        .child(
                            Clipboard::new("copy-history-url")
                                .small()
                                .value(url)
                                .tooltip(tr!(cx, "复制 URL", "Copy URL")),
                        ),
                )
            },
        )
        .when(
            detail.summary.has_body_file && this.history.body.is_none(),
            |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(
                            cx,
                            "响应体过大，保存在本地文件中（详情见数据目录）。",
                            "The response body is large and was stored in a local file."
                        )),
                )
            },
        )
        .child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(tr!(
                            cx,
                            "响应体",
                            "Response body"
                        )))
                        .when(!body.is_empty(), |this| {
                            this.child(
                                Clipboard::new("copy-history-body")
                                    .small()
                                    .value(pretty_body(&body))
                                    .tooltip(tr!(cx, "复制响应体", "Copy response body")),
                            )
                        }),
                )
                .child(
                    div()
                        .rounded_md()
                        .bg(theme.muted)
                        .p_3()
                        .text_xs()
                        .font_family(mono)
                        .whitespace_normal()
                        .child(if body.is_empty() {
                            tr!(cx, "（空）", "(empty)").to_string()
                        } else {
                            pretty_body(&body)
                        }),
                ),
        )
        .child(
            Button::new("history-delete")
                .outline()
                .small()
                .icon(IconName::Delete)
                .label(tr!(cx, "删除这条记录", "Delete this entry"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.delete_history_entry(window, cx);
                })),
        )
        .into_any_element()
}

/// 记一次还没进历史库的发送，挂在当前标签的时间线上。
pub fn push_local_run(tab: &mut crate::app::RequestTab, status: u16, duration_ms: i64, url: &str) {
    tab.local_runs.insert(
        0,
        crate::app::LocalRun {
            id: uuid::Uuid::new_v4().to_string(),
            at: thread_core::db::now_ms(),
            status,
            duration_ms,
            url: url.to_string(),
            request: None,
            response: None,
        },
    );
}

/// 当前请求自己的时间线：已保存的 HTTP 请求读历史库，其余用本次打开后的发送记录。
pub fn request_timeline(
    this: &mut AppView,
    tab_ix: usize,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let Some(tab) = this.tabs.get(tab_ix) else {
        return div().into_any_element();
    };
    let path = tab.path.clone();
    let local = tab.local_runs.clone();
    let use_db = path.is_some() && tab.protocol == crate::views::request::Protocol::Http;
    if use_db && this.timeline_for.as_deref() != path.as_deref() {
        this.reload_request_timeline(tab_ix, cx);
    }
    let db_items = if use_db {
        this.timeline.clone()
    } else {
        Vec::new()
    };

    let selected = this
        .tabs
        .get(tab_ix)
        .and_then(|tab| tab.timeline_selected.clone());
    let (request, wire) = this
        .tabs
        .get(tab_ix)
        .map(|tab| (tab.timeline_request.clone(), tab.timeline_wire))
        .unwrap_or((None, false));
    let rows = if use_db {
        db_items
            .into_iter()
            .map(|item| {
                let chosen = selected.as_deref() == Some(item.id.as_str());
                timeline_db_row(tab_ix, item, chosen, cx)
            })
            .collect::<Vec<_>>()
    } else {
        local
            .into_iter()
            .map(|run| {
                let chosen = selected.as_deref() == Some(run.id.as_str());
                timeline_local_row(tab_ix, run, chosen, cx)
            })
            .collect::<Vec<_>>()
    };

    v_flex()
        .size_full()
        .min_h_0()
        .gap_2()
        .id("request-timeline")
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr!(cx, "时间线", "Timeline")),
                )
                .child(
                    Button::new("timeline-clear")
                        .outline()
                        .small()
                        .label(tr!(cx, "清空", "Clear"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.clear_request_timeline(tab_ix, window, cx);
                        })),
                ),
        )
        .when(!use_db, |column| {
            column.child(widgets::notice_line(
                cx,
                tr!(
                    cx,
                    "临时请求的时间线只留在这次打开里。保存到工作区之后，记录会跟这个请求走。",
                    "An untitled request keeps its timeline for this session. After you save it, the record stays with the request."
                ),
            ))
        })
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .id("timeline-rows")
                .overflow_y_scroll()
                .gap_1()
                .when(rows.is_empty(), |column| {
                    column.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(tr!(
                                cx,
                                "还没有发送记录。发送一次之后会出现在这里，点一行可以查看那次的请求和响应，也可以单独保存、删除或清空。",
                                "No sends yet. After you send, each run shows up here. Click a row to view that request and response, or save, delete, and clear them one by one."
                            )),
                    )
                })
                .when(!rows.is_empty(), |column| column.child(timeline_header(cx)))
                .children(rows),
        )
        .when_some(request, |column, config| {
            column.child(timeline_request_panel(&config, wire, cx))
        })
        .into_any_element()
}

fn timeline_header(cx: &gpui_kit::App) -> AnyElement {
    let theme = cx.theme().clone();
    h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(div().w(px(72.)).child(tr!(cx, "时间", "Time")))
        .child(div().w(px(56.)).child(tr!(cx, "方法", "Method")))
        .child(div().w(px(44.)).child(tr!(cx, "状态", "Status")))
        .child(div().w(px(64.)).child(tr!(cx, "耗时", "Duration")))
        .child(div().flex_1().child("URL"))
        .into_any_element()
}

fn timeline_db_row(
    tab_ix: usize,
    item: HistorySummary,
    selected: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = crate::format::SemanticColors::for_theme(&theme);
    let id = item.id.clone();
    let select_id = id.clone();
    let save_id = id.clone();
    let delete_id = id.clone();
    let when = format_relative_time(item.created_at, cx);
    let status = item
        .status_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "—".to_string());
    let duration = item
        .duration_ms
        .map(|ms| format_duration(ms as f64))
        .unwrap_or_default();
    h_flex()
        .id(SharedString::from(format!("timeline-{id}")))
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.accent))
        .when(!selected, |row| row.hover(|row| row.bg(theme.muted)))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_timeline_entry(tab_ix, &select_id, cx);
        }))
        .child(
            div()
                .w(px(72.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(when),
        )
        .child(
            div()
                .w(px(56.))
                .flex_shrink_0()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(method_color(item.method.as_deref(), &colors))
                .child(item.method.clone().unwrap_or_default()),
        )
        .child(
            div()
                .w(px(44.))
                .flex_shrink_0()
                .text_xs()
                .text_color(status_color(item.status_code, &colors))
                .child(status),
        )
        .child(
            div()
                .w(px(64.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(duration),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_ellipsis()
                .child(item.url.clone().unwrap_or_default()),
        )
        .child(
            Button::new(SharedString::from(format!("timeline-save-{save_id}")))
                .ghost()
                .xsmall()
                .label(tr!(cx, "保存", "Save"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.save_timeline_entry(&save_id, tab_ix, window, cx);
                })),
        )
        .child(
            Button::new(SharedString::from(format!("timeline-delete-{delete_id}")))
                .ghost()
                .xsmall()
                .label(tr!(cx, "删除", "Delete"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.delete_timeline_entry(&delete_id, tab_ix, window, cx);
                })),
        )
        .into_any_element()
}

fn timeline_local_row(
    tab_ix: usize,
    run: crate::app::LocalRun,
    selected: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = crate::format::SemanticColors::for_theme(&theme);
    let id = run.id.clone();
    let select_id = id.clone();
    let save_id = id.clone();
    let delete_id = id.clone();
    let method = run
        .request
        .as_ref()
        .map(|config| config.method.clone())
        .unwrap_or_default();
    h_flex()
        .id(SharedString::from(format!("timeline-local-{id}")))
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.accent))
        .when(!selected, |row| row.hover(|row| row.bg(theme.muted)))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_timeline_entry(tab_ix, &select_id, cx);
        }))
        .child(
            div()
                .w(px(72.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format_relative_time(run.at, cx)),
        )
        .child(
            div()
                .w(px(56.))
                .flex_shrink_0()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(method_color(
                    (!method.is_empty()).then_some(method.as_str()),
                    &colors,
                ))
                .child(method),
        )
        .child(
            div()
                .w(px(44.))
                .flex_shrink_0()
                .text_xs()
                .text_color(status_color(Some(run.status as i64), &colors))
                .child(run.status.to_string()),
        )
        .child(
            div()
                .w(px(64.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format_duration(run.duration_ms as f64)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_ellipsis()
                .child(run.url),
        )
        .child(
            Button::new(SharedString::from(format!("timeline-local-save-{save_id}")))
                .ghost()
                .xsmall()
                .label(tr!(cx, "保存", "Save"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this
                        .tabs
                        .get(tab_ix)
                        .and_then(|tab| tab.path.as_ref())
                        .is_some()
                    {
                        this.save_request(tab_ix, window, cx);
                    } else {
                        this.save_request_as(tab_ix, window, cx);
                    }
                })),
        )
        .child(
            Button::new(SharedString::from(format!(
                "timeline-local-delete-{delete_id}"
            )))
            .ghost()
            .xsmall()
            .label(tr!(cx, "删除", "Delete"))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(tab) = this.tabs.get_mut(tab_ix) {
                    tab.local_runs.retain(|run| run.id != delete_id);
                    if tab.timeline_selected.as_deref() == Some(delete_id.as_str()) {
                        tab.timeline_selected = None;
                        tab.timeline_request = None;
                        tab.timeline_wire = false;
                    }
                }
                cx.notify();
            })),
        )
        .into_any_element()
}

fn timeline_request_panel(
    config: &HttpRequestConfig,
    wire: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let mono = theme.mono_font_family.clone();
    let query = enabled_pairs(&config.query_params);
    let headers = enabled_pairs(&config.headers);
    let body = body_text(&config.body);
    let auth = auth_summary(&config.auth, cx);
    let url = config.url.clone();

    v_flex()
        .flex_1()
        .min_h_0()
        .id("timeline-request")
        .overflow_y_scroll()
        .gap_2()
        .pt_2()
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr!(cx, "请求", "Request")),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(widgets::MethodTag::new(&config.method, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_ellipsis_start()
                        .child(url.clone()),
                )
                .when(!url.is_empty(), |row| {
                    row.child(
                        Clipboard::new("copy-timeline-request-url")
                            .small()
                            .value(url)
                            .tooltip(tr!(cx, "复制 URL", "Copy URL")),
                    )
                }),
        )
        .when(!query.is_empty(), |panel| {
            panel.child(kv_block(cx, tr!(cx, "查询参数", "Query"), &query))
        })
        .when(!headers.is_empty(), |panel| {
            panel.child(kv_block(cx, tr!(cx, "请求头", "Headers"), &headers))
        })
        .when(!wire, |panel| {
            panel.child(
                v_flex()
                    .gap_1()
                    .child(section_label(cx, tr!(cx, "认证", "Auth")))
                    .child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(theme.foreground)
                            .child(auth),
                    ),
            )
        })
        .when_some(body, |panel, body| {
            panel.child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(section_label(cx, tr!(cx, "请求体", "Body")))
                            .child(
                                Clipboard::new("copy-timeline-request-body")
                                    .small()
                                    .value(body.clone())
                                    .tooltip(tr!(cx, "复制请求体", "Copy body")),
                            ),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .bg(theme.muted)
                            .p_3()
                            .text_xs()
                            .font_family(mono)
                            .whitespace_normal()
                            .child(body),
                    ),
            )
        })
        .into_any_element()
}

fn section_label(cx: &gpui_kit::App, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn kv_block(
    cx: &gpui_kit::App,
    title: impl Into<SharedString>,
    pairs: &[(String, String)],
) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex()
        .gap_1()
        .child(section_label(cx, title))
        .children(pairs.iter().map(|(key, value)| {
            h_flex()
                .gap_2()
                .items_start()
                .text_xs()
                .child(
                    div()
                        .w(px(140.))
                        .flex_shrink_0()
                        .text_color(theme.muted_foreground)
                        .child(key.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .child(value.clone()),
                )
        }))
        .into_any_element()
}

fn enabled_pairs(rows: &[KeyValue]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|row| row.enabled && !row.key.is_empty())
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect()
}

fn body_text(body: &HttpBody) -> Option<String> {
    match body.kind.as_str() {
        "none" | "" => None,
        "form" | "form-data" => {
            let lines: Vec<String> = body
                .fields
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
                .map(|field| {
                    let value = if field.kind == "file" {
                        field.file_path.clone().unwrap_or_default()
                    } else {
                        field.value.clone()
                    };
                    format!("{}: {value}", field.key)
                })
                .collect();
            if !lines.is_empty() {
                Some(lines.join("\n"))
            } else if body.content.is_empty() {
                None
            } else {
                Some(body.content.clone())
            }
        }
        "json" if !body.content.is_empty() => Some(pretty_body(&body.content)),
        _ if body.content.is_empty() => None,
        _ => Some(body.content.clone()),
    }
}

fn auth_summary(auth: &thread_core::workspace::formats::AuthSpec, cx: &gpui_kit::App) -> String {
    match auth.kind.as_str() {
        "none" => tr!(cx, "无", "None").to_string(),
        "bearer" => format!("Bearer {}", auth.token.as_deref().unwrap_or("")),
        "basic" => format!(
            "Basic {}:{}",
            auth.username.as_deref().unwrap_or(""),
            auth.password.as_deref().unwrap_or("")
        ),
        "apiKey" | "apikey" => format!(
            "{} {} = {}",
            auth.location.as_deref().unwrap_or("header"),
            auth.key.as_deref().unwrap_or(""),
            auth.value.as_deref().unwrap_or("")
        ),
        _ => tr!(cx, "继承集合", "Inherit collection").to_string(),
    }
}

/// 历史快照里的请求。优先用当时真正发出去的内容（变量已换成具体值）。
/// 第二个值表示这是线上请求，认证已经写进请求头。
fn request_config_from_detail(detail: &HistoryDetail) -> Option<(HttpRequestConfig, bool)> {
    if let Some(config) = resolved_display(&detail.request_snapshot) {
        return Some((config, true));
    }
    for key in ["sent", "document"] {
        if let Some(config) = detail
            .request_snapshot
            .get(key)
            .and_then(|value| serde_json::from_value::<HttpRequestConfig>(value.clone()).ok())
            .filter(|config| {
                !config.url.is_empty()
                    || !config.query_params.is_empty()
                    || !config.headers.is_empty()
                    || (config.body.kind != "none" && !config.body.kind.is_empty())
                    || !config.body.content.is_empty()
            })
        {
            return Some((config, false));
        }
    }
    None
}

fn resolved_display(snapshot: &serde_json::Value) -> Option<HttpRequestConfig> {
    let resolved = snapshot.get("resolved")?;
    let method = resolved
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let url = resolved
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if method.is_empty() && url.is_empty() {
        return None;
    }
    let headers = resolved
        .get("headers")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let kind = resolved
        .get("bodyKind")
        .and_then(|value| value.as_str())
        .unwrap_or("none")
        .to_string();
    let content = resolved
        .get("bodyPreview")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    Some(HttpRequestConfig {
        method: if method.is_empty() {
            "GET".to_string()
        } else {
            method
        },
        url,
        headers,
        body: HttpBody {
            kind,
            content,
            ..HttpBody::default()
        },
        ..HttpRequestConfig::default()
    })
}

impl AppView {
    /// Export the selected HTTP history detail as a HAR 1.2 file.
    pub fn prompt_har_export(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.history.selected.clone() else {
            return;
        };
        let name = detail
            .summary
            .name
            .clone()
            .unwrap_or_else(|| "history".into());
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr!(cx, "选择 HAR 导出文件夹", "Choose a HAR export folder").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(directory) = paths.into_iter().next() else {
                return;
            };
            let exported = cx
                .background_spawn(async move {
                    let text = thread_core::workspace::har::export_history(&[detail])?;
                    let path = thread_core::workspace::layout::unique_path(
                        &directory,
                        &thread_core::workspace::layout::slugify(&name),
                        ".har",
                    );
                    thread_core::workspace::files::write_text(&path, &text)?;
                    Ok::<_, AppError>(path)
                })
                .await;
            let _ = this.update_in(cx, |_, window, cx| match exported {
                Ok(path) => window.push_notification(
                    (
                        NotificationType::Success,
                        tr!(
                            cx,
                            format!("HAR 已导出到 {}", path.display()),
                            format!("HAR exported to {}", path.display())
                        ),
                    ),
                    cx,
                ),
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx)
                }
            });
        })
        .detach();
    }

    pub fn reload_history(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            self.history.items.clear();
            self.history.total = 0;
            self.history.selected = None;
            self.history.body = None;
            return;
        };
        let root = root.to_string_lossy().into_owned();
        let db = self.core.db.clone();
        let receiver = self
            .core
            .spawn(async move { history::list(&db, &root, None, 200, 0) });
        self.history.loading = true;
        cx.spawn(async move |this, cx| {
            let page = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update(cx, |view, cx| {
                view.history.loading = false;
                match page {
                    Ok(page) => {
                        view.history.items = page.items;
                        view.history.total = page.total;
                    }
                    Err(error) => {
                        view.history.items.clear();
                        view.history.total = 0;
                        tracing::warn!(?error, "failed to load history");
                    }
                }
                view.history.selected = None;
                view.history.body = None;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open_history_detail(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.history.items.get(ix) else {
            return;
        };
        let id = item.id.clone();
        let db = self.core.db.clone();
        let receiver = self.core.spawn(async move { history::get(&db, &id) });
        let db2 = self.core.db.clone();
        cx.spawn(async move |this, cx| {
            let detail = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            // 大响应体从磁盘读出来。
            let body = match &detail {
                Ok(detail) => match (&detail.response_body, &detail.response_body_file_path) {
                    (Some(inline), _) => Some(inline.clone()),
                    (None, Some(path)) => std::fs::read_to_string(path).ok(),
                    _ => None,
                },
                Err(_) => None,
            };
            let _ = db2;
            let _ = this.update(cx, |view, cx| {
                match detail {
                    Ok(detail) => {
                        view.history.selected = Some(detail);
                        view.history.body = body;
                    }
                    Err(error) => {
                        view.history.selected = None;
                        view.history.body = None;
                        tracing::warn!(?error, "failed to load history detail");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_history_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = &self.history.selected else {
            return;
        };
        let id = selected.summary.id.clone();
        let db = self.core.db.clone();
        let receiver = self.core.spawn(async move { history::delete(&db, &id) });
        cx.spawn(async move |this, cx| {
            let _ = receiver.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.history.selected = None;
                view.history.body = None;
                view.reload_history(cx);
                window.push_notification(
                    tr!(cx, "已删除该条历史记录。", "History entry deleted."),
                    cx,
                );
            });
        })
        .detach();
    }

    pub fn clear_history(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.read(cx).root() else {
            return;
        };
        let root = root.to_string_lossy().into_owned();
        let db = self.core.db.clone();
        let receiver = self
            .core
            .spawn(async move { history::clear(&db, &root, None) });
        cx.spawn(async move |this, cx| {
            let removed = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update_in(cx, |view, window, cx| {
                view.history.selected = None;
                view.history.body = None;
                view.reload_history(cx);
                match removed {
                    Ok(count) => window.push_notification(
                        (
                            NotificationType::Success,
                            tr!(
                                cx,
                                format!("已清空 {count} 条历史记录。"),
                                format!("Cleared {count} history entries.")
                            ),
                        ),
                        cx,
                    ),
                    Err(error) => {
                        window.push_notification((NotificationType::Error, error.to_string()), cx)
                    }
                }
            });
        })
        .detach();
    }

    /// 点时间线的一行：选中它，展示那次的请求，并把响应放到响应区。正在编辑的表单保持不动。
    pub fn select_timeline_entry(&mut self, tab_ix: usize, id: &str, cx: &mut Context<Self>) {
        let values = self.current_variables(cx);
        let Some(tab) = self.tabs.get_mut(tab_ix) else {
            return;
        };
        tab.timeline_selected = Some(id.to_string());
        let local = tab
            .local_runs
            .iter()
            .find(|run| run.id == id)
            .map(|run| (run.request.clone(), run.response.clone()));
        if let Some((request, response)) = local {
            let shown = request
                .as_ref()
                .map(|config| crate::views::url_vars::concrete_config(config, &values));
            tab.timeline_request = shown.clone();
            tab.timeline_wire = false;
            if let Some(shown) = shown
                && let Some(run) = tab.local_runs.iter_mut().find(|run| run.id == id)
            {
                run.request = Some(shown);
            }
            if let Some(response) = response {
                tab.response = Some(response);
            }
            cx.notify();
            return;
        }
        tab.timeline_request = None;
        tab.timeline_wire = false;
        let id = id.to_string();
        let expected = id.clone();
        let db = self.core.db.clone();
        let receiver = self.core.spawn(async move { history::get(&db, &id) });
        cx.spawn(async move |this, cx| {
            let detail = receiver.await.ok().and_then(|result| result.ok());
            let _ = this.update(cx, |view, cx| {
                let current = view
                    .tabs
                    .get(tab_ix)
                    .and_then(|tab| tab.timeline_selected.clone());
                if current.as_deref() != Some(expected.as_str()) {
                    return;
                }
                let prepared = detail.as_ref().and_then(request_config_from_detail);
                let values = if matches!(prepared, Some((_, false))) {
                    view.current_variables(cx)
                } else {
                    std::collections::BTreeMap::new()
                };
                let Some(tab) = view.tabs.get_mut(tab_ix) else {
                    return;
                };
                match prepared {
                    Some((config, true)) => {
                        tab.timeline_request = Some(config);
                        tab.timeline_wire = true;
                    }
                    Some((config, false)) => {
                        tab.timeline_request =
                            Some(crate::views::url_vars::concrete_config(&config, &values));
                        tab.timeline_wire = false;
                    }
                    None => {
                        tab.timeline_request = None;
                        tab.timeline_wire = false;
                    }
                }
                if let Some(detail) = detail.as_ref()
                    && let Some(response) = crate::views::request::response_from_history(detail)
                {
                    tab.response = Some(response);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 读取当前已保存 HTTP 请求在历史库里的记录。
    pub fn reload_request_timeline(&mut self, tab_ix: usize, cx: &mut Context<Self>) {
        let Some(path) = self.tabs.get(tab_ix).and_then(|tab| tab.path.clone()) else {
            return;
        };
        let Some(root) = self
            .workspace
            .read(cx)
            .root()
            .map(|root| root.to_string_lossy().into_owned())
        else {
            return;
        };
        if self.timeline_for.as_deref() != Some(path.as_str()) {
            self.timeline.clear();
        }
        self.timeline_for = Some(path.clone());
        let db = self.core.db.clone();
        let expected = path.clone();
        let receiver = self
            .core
            .spawn(async move { history::list_for_request(&db, &root, &path, 100) });
        cx.spawn(async move |this, cx| {
            let items = receiver.await.ok().and_then(|result| result.ok());
            let _ = this.update(cx, |view, cx| {
                if view.timeline_for.as_deref() != Some(expected.as_str()) {
                    return;
                }
                if view.tabs.get(tab_ix).and_then(|tab| tab.path.as_deref())
                    != Some(expected.as_str())
                {
                    return;
                }
                view.timeline = items.unwrap_or_default();
                cx.notify();
            });
        })
        .detach();
    }

    /// 把一条历史记录里的请求文档写回当前文件。
    pub fn save_timeline_entry(
        &mut self,
        id: &str,
        tab_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rel) = self.tabs.get(tab_ix).and_then(|tab| tab.path.clone()) else {
            self.save_request_as(tab_ix, window, cx);
            return;
        };
        let Some(root) = self
            .workspace
            .read(cx)
            .root()
            .map(|path| path.to_path_buf())
        else {
            return;
        };
        let id = id.to_string();
        let db = self.core.db.clone();
        let receiver = self.core.spawn(async move {
            let detail = history::get(&db, &id)?;
            let document = detail
                .request_snapshot
                .get("document")
                .cloned()
                .ok_or_else(|| {
                    AppError::Validation("history entry has no request document".into())
                })?;
            thread_core::workspace::ops::update_request_document(&root, &rel, document)
        });
        cx.spawn(async move |this, cx| {
            let saved = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update_in(cx, |view, window, cx| {
                match saved {
                    Ok(file) => {
                        if let Some(old) = view.tabs.get(tab_ix) {
                            let tab_id = old.id.clone();
                            let runs = old.local_runs.clone();
                            if let Some(path) = old.path.clone() {
                                let mut fresh =
                                    crate::app::RequestTab::from_file(path, Ok(file), window, cx);
                                fresh.id = tab_id;
                                fresh.local_runs = runs;
                                fresh.section = crate::views::request::RequestSection::Timeline;
                                view.tabs[tab_ix] = fresh;
                                if let Some(url) = view.tabs.get(tab_ix).map(|tab| tab.url.clone())
                                {
                                    crate::views::url_vars::watch_request_url(
                                        view, url, window, cx,
                                    );
                                }
                                crate::views::request::watch_tab_edits(view, tab_ix, window, cx);
                            }
                        }
                        view.workspace.update(cx, |model, cx| model.refresh(cx));
                        view.reload_request_timeline(tab_ix, cx);
                        window.push_notification(
                            (
                                NotificationType::Success,
                                tr!(
                                    cx,
                                    "已把这条记录写回请求。",
                                    "Wrote this run back to the request."
                                )
                                .to_string(),
                            ),
                            cx,
                        );
                    }
                    Err(error) => {
                        window.push_notification((NotificationType::Error, error.to_string()), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_timeline_entry(
        &mut self,
        id: &str,
        tab_ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = id.to_string();
        let removed_id = id.clone();
        let db = self.core.db.clone();
        let receiver = self.core.spawn(async move { history::delete(&db, &id) });
        cx.spawn(async move |this, cx| {
            let removed = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update_in(cx, |view, window, cx| match removed {
                Ok(_) => {
                    if let Some(tab) = view.tabs.get_mut(tab_ix)
                        && tab.timeline_selected.as_deref() == Some(removed_id.as_str())
                    {
                        tab.timeline_selected = None;
                        tab.timeline_request = None;
                        tab.timeline_wire = false;
                    }
                    view.reload_request_timeline(tab_ix, cx);
                    view.reload_history(cx);
                    window.push_notification(tr!(cx, "已删除这条记录。", "Deleted this run."), cx);
                }
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    pub fn clear_request_timeline(
        &mut self,
        tab_ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(tab_ix) else {
            return;
        };
        let http_file = tab.protocol == crate::views::request::Protocol::Http && tab.path.is_some();
        if !http_file {
            if let Some(tab) = self.tabs.get_mut(tab_ix) {
                tab.local_runs.clear();
                tab.timeline_selected = None;
                tab.timeline_request = None;
                tab.timeline_wire = false;
            }
            cx.notify();
            return;
        }
        let Some(path) = tab.path.clone() else {
            return;
        };
        let Some(root) = self
            .workspace
            .read(cx)
            .root()
            .map(|root| root.to_string_lossy().into_owned())
        else {
            return;
        };
        let db = self.core.db.clone();
        let receiver = self
            .core
            .spawn(async move { history::clear_for_request(&db, &root, &path) });
        cx.spawn(async move |this, cx| {
            let removed = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|result| result);
            let _ = this.update_in(cx, |view, window, cx| match removed {
                Ok(count) => {
                    view.timeline.clear();
                    if let Some(tab) = view.tabs.get_mut(tab_ix) {
                        tab.timeline_selected = None;
                        tab.timeline_request = None;
                        tab.timeline_wire = false;
                    }
                    view.reload_history(cx);
                    window.push_notification(
                        (
                            NotificationType::Success,
                            tr!(
                                cx,
                                format!("已清空这个请求的 {count} 条记录。"),
                                format!("Cleared {count} runs for this request.")
                            ),
                        ),
                        cx,
                    );
                    cx.notify();
                }
                Err(error) => {
                    window.push_notification((NotificationType::Error, error.to_string()), cx);
                }
            });
        })
        .detach();
    }
}
