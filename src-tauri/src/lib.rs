//! Tauri application entry point and command registry.
//!
//! Thread 桌面端的 Rust 入口骨架。聚合可复用的基础模块（配置存储、主题、
//! 窗口管理、菜单），初始化共享的全局 state，应用 macOS 窗口效果，并把
//! 全部 `#[tauri::command]` 注册到调用处理器中供前端 invoke 调用。
//!
//! 业务逻辑（终端、SSH、SFTP 等）已被移除，这里作为空项目模板保留骨架。

mod atomic_file;
mod config;
mod connections_window;
mod desktop_menu;
mod editor_window;
mod error;
#[cfg(target_os = "macos")]
mod macos_menu;
mod runtime;
mod settings_window;
mod sync_mutex;
mod themes;

use std::sync::Arc;

use config::ThreadConfigStore;

/// Applies the translucent vibrancy material to a macOS window.
///
/// 为 macOS 窗口应用半透明的"窗口背景"材质（vibrancy），实现毛玻璃效果；
/// 在非 macOS 平台上为一个空实现。
#[cfg(target_os = "macos")]
pub(crate) fn apply_macos_window_effects(window: &tauri::WebviewWindow) {
    use tauri::window::{Effect, EffectState, EffectsBuilder};

    let _ = window.set_effects(Some(
        EffectsBuilder::new()
            .effect(Effect::WindowBackground)
            .state(EffectState::Active)
            .radius(10.0)
            .build(),
    ));
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_macos_window_effects(_window: &tauri::WebviewWindow) {}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("thread=info,warn")),
        )
        .try_init();
}

#[cfg(target_os = "macos")]
fn configure_macos_window(app: &tauri::App) {
    use tauri::Manager;

    if let Some(window) = app.get_webview_window("main") {
        apply_macos_window_effects(&window);
    }
}

#[tauri::command]
fn apply_macos_window_chrome(app: tauri::AppHandle, label: String) -> Result<(), String> {
    use tauri::Manager;

    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("window not found: {label}"))?;
    apply_macos_window_effects(&window);
    Ok(())
}

/// Builds and runs the Tauri application until the last window closes.
///
/// 构建并启动 Tauri 应用：创建共享配置存储并注册为全局 state，安装
/// 对话框/opener 插件，完成 macOS 窗口装饰，注册所有命令后进入事件循环，
/// 直到应用退出。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let config_store = Arc::new(ThreadConfigStore::new());
    let _themes_state = themes::ThemesState::new();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(config_store)
        .setup(|app| {
            #[cfg(target_os = "macos")]
            configure_macos_window(app);

            Ok(())
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .enable_macos_default_menu(false)
            .menu(|app| macos_menu::build_app_menu(app))
            .on_menu_event(|app, event| {
                macos_menu::handle_menu_event(app, event);
            });
    }

    #[cfg(not(target_os = "macos"))]
    {
        builder = builder
            .menu(|app| desktop_menu::build_app_menu(app))
            .on_menu_event(|app, event| {
                desktop_menu::handle_menu_event(app, event);
            });
    }

    builder
        .invoke_handler(tauri::generate_handler![
            config::get_config_dir,
            config::get_config_file_path,
            config::get_config_load_warnings,
            config::load_config_sections,
            config::get_config_section,
            config::set_config_section,
            config::remove_config_section,
            themes::get_themes_dir,
            themes::list_color_themes_command,
            themes::read_color_theme_css_command,
            apply_macos_window_chrome,
            settings_window::open_settings_window,
            connections_window::open_connections_window,
            editor_window::open_editor_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
