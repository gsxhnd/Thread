//! 左侧活动栏：图标切换侧边栏内容，主区域保持当前工作面。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Placement, Selectable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    div, px,
};

use crate::app::AppView;
use crate::state::MainView;

pub fn render(this: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let active = this.view;

    v_flex()
        .id("activity-rail")
        .w(px(48.))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .py_2()
        .gap_1()
        .bg(theme.sidebar)
        .border_r_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .gap_1()
                .child(rail_button(cx, active, MainView::Request))
                .child(rail_button(cx, active, MainView::Environments))
                .child(rail_button(cx, active, MainView::History))
                .child(div().w(px(16.)).h(px(1.)).my_2().bg(theme.border))
                .child(rail_button(cx, active, MainView::Proxy))
                .child(rail_button(cx, active, MainView::Certificates)),
        )
        .child(
            Button::new("rail-settings")
                .ghost()
                .compact()
                .icon(IconName::Settings)
                .tooltip(tr!(cx, "设置", "Settings"))
                .tooltip_placement(Placement::Right)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_settings(window, cx);
                })),
        )
}

fn rail_button(cx: &mut Context<AppView>, active: MainView, view: MainView) -> AnyElement {
    let theme = cx.theme().clone();
    let selected = active == view;
    div()
        .w_full()
        .h(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .when(selected, |well| well.bg(theme.accent))
                .child(
                    Button::new(rail_id(view))
                        .ghost()
                        .compact()
                        .icon(icon_for(view))
                        .selected(selected)
                        .tooltip(view.label(cx))
                        .tooltip_placement(Placement::Right)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.switch_view(view, window, cx);
                        })),
                ),
        )
        .into_any_element()
}

fn rail_id(view: MainView) -> &'static str {
    match view {
        MainView::Request => "rail-collections",
        MainView::Environments => "rail-environments",
        MainView::History => "rail-history",
        MainView::Proxy => "rail-proxy",
        MainView::Certificates => "rail-certificates",
    }
}

fn icon_for(view: MainView) -> IconName {
    match view {
        MainView::Request => IconName::Folder,
        MainView::Environments => IconName::Braces,
        MainView::History => IconName::Clock,
        MainView::Proxy => IconName::Network,
        MainView::Certificates => IconName::ShieldCheck,
    }
}
