//! 快捷键页：只列出应用自己绑定、并且当前窗口能解析到的按键。

use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{Action, App, IntoElement, ParentElement as _, Styled as _, Window, div};

use crate::widgets;
use crate::{
    CloseTab, CloseWorkspace, NewRequest, NextTab, OpenSettings, PreviousTab, Quit,
    RenameTreeSelection, SaveRequest, SendRequest, ToggleLayout, ToggleSidebar,
};

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
                "这里只列出已经生效的绑定。在输入框里，发送和切换标签仍然可用。",
                "Only bindings that are active. Send and tab switching still work while typing."
            ),
            Vec::<gpui_kit::AnyElement>::new(),
        ))
        .child(widgets::section(
            cx,
            tr!(cx, "请求", "Request"),
            tr!(
                cx,
                "针对当前打开的请求标签。",
                "These act on the request tab in front."
            ),
            v_flex()
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "发送请求", "Send request"),
                    tr!(
                        cx,
                        "发送当前请求。在地址栏或正文里按也一样。",
                        "Sends the current request, including from the URL or body."
                    ),
                    &SendRequest,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "保存请求", "Save request"),
                    tr!(
                        cx,
                        "把当前请求写回工作区。临时请求会存成新文件。",
                        "Writes the current request back to the workspace."
                    ),
                    &SaveRequest,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "新建请求", "New request"),
                    tr!(cx, "打开一个临时 HTTP 请求。", "Opens an untitled HTTP request."),
                    &NewRequest,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "关闭标签", "Close tab"),
                    tr!(
                        cx,
                        "关掉当前标签。有未保存的修改时会先询问。",
                        "Closes the current tab, asking first when it has unsaved edits."
                    ),
                    &CloseTab,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "下一个标签", "Next tab"),
                    tr!(cx, "切到右边的请求标签。", "Moves to the next request tab."),
                    &NextTab,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "上一个标签", "Previous tab"),
                    tr!(cx, "切到左边的请求标签。", "Moves to the previous request tab."),
                    &PreviousTab,
                )),
        ))
        .child(widgets::section(
            cx,
            tr!(cx, "导航", "Navigation"),
            tr!(
                cx,
                "侧边栏、布局和工作区。集合树获得焦点时，回车会重命名选中的请求。",
                "Sidebar, layout, and the workspace. Enter renames the selected request when the collection tree is focused."
            ),
            v_flex()
                .child(shortcut_row(
                    window,
                    cx,
                    Some("Tree"),
                    tr!(cx, "重命名请求", "Rename request"),
                    tr!(
                        cx,
                        "在集合树里选中一个请求后，在原位置改名。集合则展开或收起。",
                        "With a request selected in the tree, edits its name in place. A collection expands or collapses."
                    ),
                    &RenameTreeSelection,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "切换侧边栏", "Toggle sidebar"),
                    tr!(cx, "展开或收起左侧集合树。", "Shows or hides the collection tree."),
                    &ToggleSidebar,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "切换布局", "Toggle layout"),
                    tr!(
                        cx,
                        "请求和响应在上下、左右之间切换。",
                        "Switches the request and response between stacked and side by side."
                    ),
                    &ToggleLayout,
                ))
                .child(shortcut_row(
                    window,
                    cx,
                    None,
                    tr!(cx, "关闭工作区", "Close workspace"),
                    tr!(
                        cx,
                        "关掉当前工作区，回到欢迎页。工作区菜单里也有这一项。",
                        "Closes the current workspace and returns to the welcome screen. The same command is in the workspace menu."
                    ),
                    &CloseWorkspace,
                )),
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
                    None,
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
                    None,
                    tr!(cx, "退出 Thread", "Quit Thread"),
                    tr!(cx, "退出应用。", "Quits the app."),
                    &Quit,
                )),
        ))
}

fn shortcut_row(
    window: &Window,
    cx: &App,
    context: Option<&str>,
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
        .child(match Kbd::binding_for_action(action, context, window) {
            Some(kbd) => kbd.into_any_element(),
            None => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(tr!(cx, "未绑定", "Not bound"))
                .into_any_element(),
        })
}
