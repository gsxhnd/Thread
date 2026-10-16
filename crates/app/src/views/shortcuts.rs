//! 快捷键页：只列出应用自己绑定、并且当前窗口能解析到的按键。

use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{Action, App, IntoElement, ParentElement as _, Styled as _, Window, div};

use crate::widgets;
use crate::{OpenSettings, Quit};

pub fn render(window: &Window, cx: &App) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .gap_5()
        .px_6()
        .py_5()
        .child(widgets::page_header(
            cx,
            tr!(cx, "快捷键", "Shortcuts"),
            tr!(
                cx,
                "这里只列出已经生效的绑定。",
                "Only bindings that are active."
            ),
            Vec::<gpui_kit::AnyElement>::new(),
        ))
        .child(widgets::section(
            cx,
            tr!(cx, "应用", "Application"),
            tr!(
                cx,
                "主窗口和设置窗口都响应这些按键。",
                "These work in the main window and the settings window."
            ),
            v_flex()
                .child(shortcut_row(
                    window,
                    cx,
                    tr!(cx, "打开设置", "Open settings"),
                    tr!(
                        cx,
                        "打开设置窗口。已经打开时，只把那个窗口放到前面。",
                        "Opens settings, or brings the existing window forward."
                    ),
                    &OpenSettings,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    tr!(cx, "退出 Thread", "Quit Thread"),
                    tr!(cx, "退出应用。", "Quits the app."),
                    &Quit,
                )),
        ))
}

fn shortcut_row(
    window: &Window,
    cx: &App,
    label: &str,
    detail: &str,
    action: &dyn Action,
) -> impl IntoElement {
    let theme = cx.theme();
    gpui_kit::component::h_flex()
        .items_center()
        .justify_between()
        .gap_4()
        .py_3()
        .border_b_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .flex_1()
                .gap_0p5()
                .min_w_0()
                .child(div().text_sm().child(label.to_string()))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(detail.to_string()),
                ),
        )
        .child(match Kbd::binding_for_action(action, None, window) {
            Some(kbd) => kbd.into_any_element(),
            None => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(cx, "未绑定", "Not bound"))
                .into_any_element(),
        })
}
