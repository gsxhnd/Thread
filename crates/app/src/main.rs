//! Thread / 灰线 —— GPUI 应用入口。

#[macro_use]
mod i18n;
mod app;
mod format;
mod state;
mod theme;
mod views;
mod widgets;

use std::sync::Arc;

use gpui_kit::component::TitleBar;
use gpui_kit::{App, AppContext as _, KeyBinding, Menu, MenuItem, WindowBounds, px, size};
use thread_core::AppCore;

use app::AppView;

gpui_kit::actions!(thread_app, [Quit, OpenSettings, SaveRequest]);

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

pub(crate) fn install_menus(cx: &mut App) {
    cx.set_menus([Menu::new("Thread").items([
        MenuItem::action(tr!(cx, "保存", "Save"), SaveRequest),
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
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-s", SaveRequest, None),
                KeyBinding::new("ctrl-s", SaveRequest, None),
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
