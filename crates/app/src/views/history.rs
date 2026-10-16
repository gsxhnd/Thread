//! 历史视图：请求历史列表 + 详情（保存在本地 SQLite，不进工作区）。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;
use thread_core::services::history::{self, HistorySummary};

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

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = crate::format::SemanticColors::for_theme(cx.theme());

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

    let items: Vec<(usize, HistorySummary)> = this
        .history
        .items
        .iter()
        .enumerate()
        .map(|(ix, item)| (ix, item.clone()))
        .collect();

    v_flex()
        .flex_1()
        .min_h_0()
        .gap_3()
        .px_6()
        .py_4()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .child(
                    h_flex().gap_2().items_center().child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(tr!(
                                cx,
                                format!("共 {} 条", this.history.total),
                                format!("{} entries", this.history.total)
                            )),
                    ),
                )
                .child(
                    Button::new("clear-history")
                        .outline()
                        .small()
                        .label(tr!(cx, "清空历史", "Clear history"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.clear_history(window, cx);
                        })),
                ),
        )
        .child(
            h_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                // 列表
                .child(
                    v_flex()
                        .w(px(340.))
                        .flex_shrink_0()
                        .min_h_0()
                        .id("history-list")
                        .overflow_y_scroll()
                        .gap_1()
                        .children(items.into_iter().map(|(ix, item)| {
                            let method_color = method_color(item.method.as_deref(), &colors);
                            let status_color = status_color(item.status_code, &colors);
                            h_flex()
                                .id(("history-item", ix))
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|this| this.bg(theme.muted))
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
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .text_ellipsis()
                                                .child(item.url.clone().unwrap_or_default()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .flex_shrink_0()
                                        .child(format_relative_time(item.created_at, cx)),
                                )
                                .on_click(cx.listener(
                                    move |this, _: &gpui_kit::ClickEvent, window, cx| {
                                        this.open_history_detail(ix, window, cx);
                                    },
                                ))
                                .into_any_element()
                        })),
                )
                // 详情
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(detail_view(this, cx)),
                ),
        )
        .into_any_element()
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
        .child(
            div()
                .text_xs()
                .text_ellipsis_start()
                .child(detail.summary.url.clone().unwrap_or_default()),
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
                .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(tr!(
                    cx,
                    "响应体",
                    "Response body"
                )))
                .child(
                    div()
                        .rounded_md()
                        .bg(theme.muted)
                        .p_3()
                        .text_xs()
                        .font_family(mono)
                        .child(if body.is_empty() {
                            tr!(cx, "（空）", "(empty)").to_string()
                        } else {
                            body
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

impl AppView {
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
}
