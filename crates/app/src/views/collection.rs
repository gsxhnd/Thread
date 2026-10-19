//! 集合页：点侧边栏文件夹后，在主区域打开这个集合。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::workspace::index::{CollectionNode, RequestNode};

use crate::app::AppView;
use crate::widgets;

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    let path = this.open_collection.clone().unwrap_or_default();
    let node = this
        .workspace
        .read(cx)
        .index()
        .and_then(|index| find_collection(&index.collections, &path))
        .cloned();
    let Some(node) = node else {
        return v_flex()
            .size_full()
            .child(widgets::band(
                theme.border,
                theme.background,
                back_button(cx),
            ))
            .child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(tr!(
                        cx,
                        "这个集合已经不在工作区里。",
                        "This collection is no longer in the workspace."
                    )),
            )
            .into_any_element();
    };

    let description = node.description.clone().unwrap_or_default();
    let children = node.children.clone();
    let requests = node.requests.clone();
    let name = node.name.clone();

    v_flex()
        .size_full()
        .min_h_0()
        .child(widgets::band(
            theme.border,
            theme.background,
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(back_button(cx))
                .child(Icon::new(IconName::Folder).small())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                ),
        ))
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .id("collection-page")
                .overflow_y_scroll()
                .p_4()
                .gap_4()
                .when(!description.is_empty(), |column| {
                    column.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(description),
                    )
                })
                .child(section_title(
                    cx,
                    tr!(cx, "子集合", "Collections"),
                    children.len(),
                ))
                .child(if children.is_empty() {
                    empty_line(cx, tr!(cx, "没有子集合。", "No nested collections."))
                } else {
                    v_flex()
                        .gap_1()
                        .children(
                            children
                                .into_iter()
                                .map(|child| collection_row(child, &theme, cx)),
                        )
                        .into_any_element()
                })
                .child(section_title(
                    cx,
                    tr!(cx, "请求", "Requests"),
                    requests.len(),
                ))
                .child(if requests.is_empty() {
                    empty_line(
                        cx,
                        tr!(
                            cx,
                            "这个集合里还没有请求。",
                            "No requests in this collection."
                        ),
                    )
                } else {
                    v_flex()
                        .gap_1()
                        .children(
                            requests
                                .into_iter()
                                .map(|request| request_row(request, &theme, cx)),
                        )
                        .into_any_element()
                }),
        )
        .into_any_element()
}

fn back_button(cx: &mut Context<AppView>) -> Button {
    Button::new("collection-back")
        .ghost()
        .small()
        .icon(IconName::ArrowLeft)
        .label(tr!(cx, "返回请求", "Back to requests"))
        .on_click(cx.listener(|this, _, _, cx| {
            this.open_collection = None;
            cx.notify();
        }))
}

fn section_title(cx: &gpui_kit::App, label: &'static str, count: usize) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(count.to_string()),
        )
        .into_any_element()
}

fn empty_line(cx: &gpui_kit::App, text: &'static str) -> AnyElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn collection_row(
    node: CollectionNode,
    theme: &gpui_kit::component::Theme,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let path = node.path.clone();
    let count = node.request_count();
    div()
        .id(SharedString::from(format!("collection-open-{}", node.path)))
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .hover(|this| this.bg(theme.muted))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::Folder).xsmall())
                .child(div().flex_1().text_sm().text_ellipsis().child(node.name))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(count.to_string()),
                ),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_collection(path.clone(), cx);
        }))
        .into_any_element()
}

fn request_row(
    node: RequestNode,
    theme: &gpui_kit::component::Theme,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let open = node.clone();
    let method = node.method.clone().unwrap_or_else(|| node.protocol.clone());
    div()
        .id(SharedString::from(format!(
            "collection-request-{}",
            node.path
        )))
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .hover(|this| this.bg(theme.muted))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .w(px(64.))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.primary)
                        .child(method),
                )
                .child(div().flex_1().text_sm().text_ellipsis().child(node.name))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .text_ellipsis()
                        .child(node.url.unwrap_or_default()),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_request(&open, window, cx);
        }))
        .into_any_element()
}

pub fn find_collection<'a>(nodes: &'a [CollectionNode], path: &str) -> Option<&'a CollectionNode> {
    for node in nodes {
        if node.path == path {
            return Some(node);
        }
        if let Some(found) = find_collection(&node.children, path) {
            return Some(found);
        }
    }
    None
}
