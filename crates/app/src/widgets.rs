//! 跨视图复用的小组件：空状态、徽标、区块卡片、键值表、页面标题。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::label::Label;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, h_flex, h_resizable, resizable_panel, v_flex, v_resizable,
};
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window, div, px,
};

use crate::format::{SemanticColors, method_color, status_color};

/// 居中空状态：图标 + 标题 + 说明 + 可选动作。
pub fn empty_state(
    cx: &App,
    icon: IconName,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let description: SharedString = description.into();
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .py_20()
        .child(
            div()
                .size_12()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme.muted)
                .child(Icon::new(icon).xsmall()),
        )
        .child(
            Label::new(title)
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .max_w(px(460.))
                .text_center()
                .child(description),
        )
}

/// 空状态 + 一个主按钮（例如“新建集合”）。
pub fn empty_state_with_action(
    cx: &App,
    icon: IconName,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    action: Button,
) -> impl IntoElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_4()
        .py_20()
        .child(empty_state(cx, icon, title, description))
        .child(action)
}

/// 带标题的卡片区块，用于设置/详情页。
pub fn section(
    cx: &App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    body: impl IntoElement,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let description: SharedString = description.into();
    v_flex()
        .gap_3()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .p_4()
        .child(
            v_flex()
                .gap_1()
                .child(
                    Label::new(title)
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description),
                ),
        )
        .child(body)
}

/// 页面标题行：大标题 + 说明 + 右侧动作区。
pub fn page_header(
    cx: &App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    trailing: Vec<impl IntoElement>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let description: SharedString = description.into();
    h_flex()
        .items_start()
        .justify_between()
        .gap_4()
        .pb_4()
        .child(
            v_flex()
                .gap_1()
                .child(
                    Label::new(title)
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description),
                ),
        )
        .child(h_flex().gap_2().children(trailing))
}

/// 单行键值说明。
pub fn field_row(
    cx: &App,
    key: impl Into<SharedString>,
    value: impl Into<SharedString>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let key: SharedString = key.into();
    let value: SharedString = value.into();
    h_flex()
        .items_center()
        .gap_2()
        .py_1()
        .text_xs()
        .child(
            div()
                .w(px(140.))
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .child(key),
        )
        .child(div().flex_1().child(value))
}

/// HTTP 方法徽标：方法名着语义色。
#[derive(IntoElement)]
pub struct MethodTag {
    method: String,
    color: Hsla,
}

impl MethodTag {
    pub fn new(method: &str, cx: &App) -> Self {
        let colors = SemanticColors::for_theme(cx.theme());
        Self {
            method: method.to_ascii_uppercase(),
            color: method_color(Some(method), &colors),
        }
    }
}

impl RenderOnce for MethodTag {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color;
        div()
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .bg(color.opacity(0.12))
            .child(self.method)
    }
}

/// 状态码徽标：2xx 绿、3xx 蓝、4xx 橙、5xx 红。
#[derive(IntoElement)]
pub struct StatusTag {
    code: u16,
    text: Option<String>,
    color: Hsla,
}

impl StatusTag {
    pub fn new(code: u16, text: Option<&str>, cx: &App) -> Self {
        let colors = SemanticColors::for_theme(cx.theme());
        Self {
            code,
            text: text.map(str::to_string),
            color: status_color(Some(code as i64), &colors),
        }
    }
}

impl RenderOnce for StatusTag {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color;
        let label = match &self.text {
            Some(text) if !text.trim().is_empty() => format!("{} {}", self.code, text),
            _ => self.code.to_string(),
        };
        div()
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .bg(color.opacity(0.12))
            .child(label)
    }
}

/// 简单小标签。
pub fn mini_tag(cx: &App, text: impl Into<SharedString>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let text: SharedString = text.into();
    div()
        .px_1()
        .py_0p5()
        .text_size(px(10.))
        .rounded_sm()
        .text_color(theme.muted_foreground)
        .bg(theme.muted)
        .child(text)
}

/// 上下分栏。下栏缺省时隐藏；有内容时两栏平分高度，内容在各自的栏里滚动。
///
/// 下栏始终占一个面板位，只是在没有内容时隐藏，避免面板数量变化把分隔状态打乱。
pub fn vertical_split(
    gutter: Hsla,
    id: &'static str,
    top: AnyElement,
    bottom: Option<AnyElement>,
) -> AnyElement {
    let open = bottom.is_some();
    let bottom = bottom.unwrap_or_else(|| div().into_any_element());
    div()
        .flex_1()
        .w_full()
        .min_h_0()
        .child(
            v_resizable(id)
                .child(
                    resizable_panel()
                        .size_range(px(140.)..px(8000.))
                        .child(fill_pane(gutter, top)),
                )
                .child(
                    resizable_panel()
                        .visible(open)
                        .size_range(px(140.)..px(8000.))
                        .child(fill_pane(gutter, bottom)),
                ),
        )
        .into_any_element()
}

/// 请求和响应的分栏。上下两栏平分高度；左右时请求在左、响应在右。
pub fn editor_split(
    gutter: Hsla,
    stacked: bool,
    id: &'static str,
    primary: AnyElement,
    secondary: Option<AnyElement>,
) -> AnyElement {
    if stacked {
        return vertical_split(gutter, id, primary, secondary);
    }
    let open = secondary.is_some();
    let secondary = secondary.unwrap_or_else(|| div().into_any_element());
    div()
        .flex_1()
        .w_full()
        .min_h_0()
        .child(
            h_resizable(id)
                .child(
                    resizable_panel()
                        .size(px(520.))
                        .size_range(px(240.)..px(8000.))
                        .child(fill_pane(gutter, primary)),
                )
                .child(
                    resizable_panel()
                        .visible(open)
                        .size_range(px(240.)..px(8000.))
                        .child(fill_pane(gutter, secondary)),
                ),
        )
        .into_any_element()
}

fn fill_pane(gutter: Hsla, child: AnyElement) -> AnyElement {
    v_flex()
        .size_full()
        .min_h_0()
        .min_w_0()
        .p_2()
        .bg(gutter)
        .overflow_hidden()
        .child(child)
        .into_any_element()
}

/// 请求页里一整条控件：底边隔开，左右留出同样的内边距。
pub fn band(border: Hsla, background: Hsla, child: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(border)
        .bg(background)
        .child(child)
}

/// 编辑区外框，和响应卡片一样有边，放在分栏的底色上。
pub fn pane_frame(border: Hsla, background: Hsla, child: impl IntoElement) -> AnyElement {
    v_flex()
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(background)
        .p_3()
        .child(child)
        .into_any_element()
}

/// 左右分栏，左侧给出初始宽度，分隔条可拖。
pub fn horizontal_split(
    id: &'static str,
    leading_width: f32,
    leading: AnyElement,
    trailing: AnyElement,
) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .min_h_0()
        .child(
            h_resizable(id)
                .child(
                    resizable_panel()
                        .size(px(leading_width))
                        .size_range(px(220.)..px(640.))
                        .child(leading),
                )
                .child(resizable_panel().child(trailing)),
        )
        .into_any_element()
}

/// 页面内提示条。
pub fn notice_line(cx: &App, message: impl Into<SharedString>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let message: SharedString = message.into();
    h_flex()
        .gap_2()
        .items_center()
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_xs()
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .child(Icon::new(IconName::Info).xsmall())
        .child(message)
}
