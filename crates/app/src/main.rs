//! Thread / 灰线 —— GPUI 应用入口。

#[macro_use]
mod i18n;
mod app;
mod codegen;
mod curl;
mod format;
mod json_fold;
mod state;
mod theme;
mod views;
mod widgets;

use std::sync::Arc;

use gpui_kit::component::TitleBar;
use gpui_kit::{App, AppContext as _, KeyBinding, Menu, MenuItem, WindowBounds, px, size};
use thread_core::AppCore;

use app::AppView;

gpui_kit::actions!(
    thread_app,
    [
        Quit,
        OpenSettings,
        SaveRequest,
        ToggleSidebar,
        ToggleLayout,
        SendRequest,
        NewRequest,
        CloseTab,
        NextTab,
        PreviousTab,
        CloseWorkspace,
        RenameTreeSelection,
    ]
);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

fn open_settings(_: &OpenSettings, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| view.open_settings(window, cx));
}

fn save_request(_: &SaveRequest, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| view.save_active_tab(window, cx));
}

fn toggle_sidebar(_: &ToggleSidebar, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update(cx, |view, cx| view.toggle_sidebar(cx));
}

fn toggle_layout(_: &ToggleLayout, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update(cx, |view, cx| view.toggle_editor_layout(cx));
}

fn send_request(_: &SendRequest, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| {
        if view.view != crate::state::MainView::Request || view.open_collection.is_some() {
            return;
        }
        let Some(index) = view.active_tab else {
            return;
        };
        view.send_request(index, window, cx);
    });
}

fn new_request(_: &NewRequest, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| {
        if !view.workspace.read(cx).is_open() {
            return;
        }
        view.new_draft_tab(window, cx);
    });
}

fn close_tab(_: &CloseTab, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| {
        let Some(index) = view.active_tab else {
            return;
        };
        view.close_tab(index, window, cx);
    });
}

fn next_tab(_: &NextTab, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update(cx, |view, cx| view.cycle_tab(1, cx));
}

fn previous_tab(_: &PreviousTab, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update(cx, |view, cx| view.cycle_tab(-1, cx));
}

fn close_workspace(_: &CloseWorkspace, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| {
        if view.workspace.read(cx).is_open() {
            view.close_workspace(window, cx);
        }
    });
}

fn rename_tree_selection(_: &RenameTreeSelection, cx: &mut App) {
    let Some(main) = cx.try_global::<app::MainApp>() else {
        return;
    };
    let app = main.0.clone();
    let _ = app.update_in(cx, |view, window, cx| {
        view.rename_or_toggle_selected(window, cx);
    });
}

pub(crate) fn install_menus(cx: &mut App) {
    cx.set_menus([Menu::new("Thread").items([
        MenuItem::action(tr!(cx, "新建请求", "New Request"), NewRequest),
        MenuItem::action(tr!(cx, "关闭标签", "Close Tab"), CloseTab),
        MenuItem::action(tr!(cx, "下一个标签", "Next Tab"), NextTab),
        MenuItem::action(tr!(cx, "上一个标签", "Previous Tab"), PreviousTab),
        MenuItem::separator(),
        MenuItem::action(tr!(cx, "发送", "Send"), SendRequest),
        MenuItem::action(tr!(cx, "保存", "Save"), SaveRequest),
        MenuItem::action(tr!(cx, "重命名", "Rename"), RenameTreeSelection),
        MenuItem::separator(),
        MenuItem::action(tr!(cx, "切换侧边栏", "Toggle Sidebar"), ToggleSidebar),
        MenuItem::action(tr!(cx, "切换布局", "Toggle Layout"), ToggleLayout),
        MenuItem::action(tr!(cx, "关闭工作区", "Close Workspace"), CloseWorkspace),
        MenuItem::separator(),
        MenuItem::action(tr!(cx, "设置…", "Settings…"), OpenSettings),
        MenuItem::separator(),
        MenuItem::action(tr!(cx, "退出 Thread", "Quit Thread"), Quit),
    ])]);
}

fn main() {
    thread_core::init_tracing();
    tracing::info!("starting");
    let core = match AppCore::new() {
        Ok(core) => Arc::new(core),
        Err(error) => {
            tracing::error!(%error, "failed to initialize core");
            eprintln!("thread: failed to initialize core: {error}");
            std::process::exit(1);
        }
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            theme::init(&core.config, cx);
            i18n::init(&core.config, cx);
            cx.on_action(quit);
            cx.on_action(open_settings);
            cx.on_action(save_request);
            cx.on_action(toggle_sidebar);
            cx.on_action(toggle_layout);
            cx.on_action(send_request);
            cx.on_action(new_request);
            cx.on_action(close_tab);
            cx.on_action(next_tab);
            cx.on_action(previous_tab);
            cx.on_action(close_workspace);
            cx.on_action(rename_tree_selection);
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("ctrl-,", OpenSettings, None),
                KeyBinding::new("cmd-s", SaveRequest, None),
                KeyBinding::new("ctrl-s", SaveRequest, None),
                KeyBinding::new("cmd-b", ToggleSidebar, None),
                KeyBinding::new("ctrl-b", ToggleSidebar, None),
                KeyBinding::new("cmd-e", ToggleLayout, None),
                KeyBinding::new("ctrl-e", ToggleLayout, None),
                KeyBinding::new("cmd-enter", SendRequest, None),
                KeyBinding::new("ctrl-enter", SendRequest, None),
                KeyBinding::new("cmd-n", NewRequest, None),
                KeyBinding::new("ctrl-n", NewRequest, None),
                KeyBinding::new("cmd-w", CloseTab, None),
                KeyBinding::new("ctrl-w", CloseTab, None),
                KeyBinding::new("ctrl-tab", NextTab, None),
                KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
                KeyBinding::new("cmd-shift-]", NextTab, None),
                KeyBinding::new("cmd-shift-[", PreviousTab, None),
                KeyBinding::new("cmd-shift-w", CloseWorkspace, None),
                KeyBinding::new("ctrl-shift-w", CloseWorkspace, None),
                KeyBinding::new("enter", RenameTreeSelection, Some("Tree")),
            ]);
            install_menus(cx);

            let options = gpui_kit::WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
                window_min_size: Some(size(px(960.), px(600.))),
                app_id: Some("thread-gpui".into()),
                ..TitleBar::window_options()
            };
            let (_handle, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| AppView::new(core.clone(), window, cx))
            })
            .expect("failed to open window");
            cx.set_global(app::MainApp(view.downgrade()));
            tracing::info!("main window opened");
            cx.activate(true);
        });
}
