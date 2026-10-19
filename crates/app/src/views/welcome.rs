//! 欢迎页：打开/新建工作区 + 最近列表。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};

use crate::app::AppView;

pub fn render(this: &mut AppView, _: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let session = this.core.config.session();

    let mut recents = Vec::new();
    for (index, recent) in session.recent_workspaces.iter().enumerate() {
        recents.push(
            h_flex()
                .id(("recent", index))
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .hover(|this| this.bg(theme.accent))
                .cursor_pointer()
                .child(Icon::new(IconName::Folder).small())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(recent.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .text_ellipsis_start()
                                .child(recent.path.clone()),
                        ),
                )
                .on_click({
                    let path = std::path::PathBuf::from(recent.path.clone());
                    cx.listener(move |this, _, window, cx| {
                        this.open_workspace_path(path.clone(), cx);
                        let _ = window;
                    })
                })
                .into_any_element(),
        );
    }

    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .bg(theme.background)
        .child(
            v_flex()
                .w(px(520.))
                .gap_8()
                .items_center()
                .child(
                    v_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .size_12()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_lg()
                                .bg(theme.primary)
                                .text_color(theme.primary_foreground)
                                .text_size(px(22.))
                                .font_weight(FontWeight::BOLD)
                                .child(tr!(cx, "灰", "T")),
                        )
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child(tr!(cx, "Thread / 灰线", "Thread")),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(tr!(
                                    cx,
                                    "本地优先的 API 调试工作台 · HTTP · gRPC · WebSocket · GraphQL",
                                    "A local-first API client · HTTP · gRPC · WebSocket · GraphQL"
                                )),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("open-workspace")
                                .primary()
                                .label(tr!(cx, "打开工作区", "Open workspace"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_open_workspace(window, cx);
                                })),
                        )
                        .child(
                            Button::new("create-workspace")
                                .outline()
                                .label(tr!(cx, "新建工作区", "New workspace"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_create_workspace(window, cx);
                                })),
                        ),
                )
                .when(!recents.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(tr!(cx, "最近打开", "Recent")),
                            )
                            .child(
                                v_flex()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.secondary)
                                    .overflow_hidden()
                                    .children(recents),
                            ),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr!(
                            cx,
                            "集合、请求与环境以 JSON 文件保存在工作区目录，可直接用 Git 管理；历史与代理数据仅存于本地。",
                            "Collections, requests, and environments are JSON files in the workspace, ready for Git. History and proxy data stay on this machine."
                        )),
                ),
        )
}
