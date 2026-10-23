//! 主窗口底部状态栏：工作区、提示、代理、当前环境。

use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div, px};
use thread_core::services::proxy::ProxyStatus;

use crate::app::AppView;

pub fn render(this: &AppView, cx: &App) -> impl IntoElement {
    let model = this.workspace.read(cx);
    let root = model.root();
    let index = model.index();
    let workspace = match (index, root.as_ref()) {
        (Some(index), Some(root)) => format!("{} · {}", index.name, root.display()),
        (Some(index), None) => index.name.clone(),
        _ => tr!(cx, "未打开工作区", "No workspace").to_string(),
    };
    let environment_active = index
        .and_then(|index| index.active_environment.as_ref())
        .is_some();
    let environment = index
        .and_then(|index| index.active_environment.clone())
        .unwrap_or_else(|| tr!(cx, "无环境", "No environment").to_string());
    let notice = model.notice.clone();

    let mut bar = StatusBar::new()
        .flex_shrink_0()
        .left(div().max_w(px(480.)).text_ellipsis().child(workspace))
        .right(proxy_chip(cx, &this.proxy.status))
        .right(status_chip(
            environment,
            environment_active,
            cx.theme().primary,
        ));
    if let Some(notice) = notice {
        bar = bar.child(
            div()
                .min_w_0()
                .text_ellipsis()
                .text_color(cx.theme().foreground)
                .child(notice),
        );
    }
    bar
}

fn status_chip(label: String, marked: bool, color: gpui_kit::Hsla) -> impl IntoElement {
    h_flex()
        .gap_1()
        .items_center()
        .when(marked, |row| {
            row.child(div().size(px(6.)).flex_shrink_0().rounded_full().bg(color))
        })
        .child(label)
}

fn proxy_chip(cx: &App, status: &ProxyStatus) -> impl IntoElement {
    let colors = crate::format::SemanticColors::for_theme(cx.theme());
    let color = if !status.running {
        cx.theme().muted_foreground
    } else if status.paused {
        colors.post
    } else {
        colors.get
    };
    status_chip(proxy_label(cx, status), true, color)
}

fn proxy_label(cx: &App, status: &ProxyStatus) -> String {
    if !status.running {
        return tr!(cx, "代理未启动", "Proxy stopped").to_string();
    }
    let endpoint = match status.port {
        Some(port) => format!("127.0.0.1:{port}"),
        None => tr!(cx, "运行中", "Running").to_string(),
    };
    if status.paused {
        tr!(
            cx,
            format!("代理 {endpoint} · 已暂停"),
            format!("Proxy {endpoint} · paused")
        )
    } else {
        tr!(cx, format!("代理 {endpoint}"), format!("Proxy {endpoint}"))
    }
}
