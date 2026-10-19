//! 内容区分发：按当前视图渲染对应页面。

use gpui_kit::InteractiveElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window};

use crate::app::AppView;
use crate::state::MainView;
use crate::views;

pub fn render(this: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let view = this.view;
    v_flex()
        .flex_1()
        .h_full()
        .min_w_0()
        .min_h_0()
        .id("content")
        .overflow_hidden()
        .child(match view {
            MainView::Request => {
                if this.open_collection.is_some() {
                    views::collection::render(this, window, cx)
                } else {
                    views::request::render(this, window, cx)
                }
            }
            MainView::History => views::history::render(this, window, cx),
            MainView::Proxy => views::proxy::render(this, window, cx),
            MainView::Certificates => views::certificates::render(this, window, cx),
            MainView::Environments => views::environments::render(this, window, cx),
        })
        .into_any_element()
}
