//! Cross-platform application menu for Windows and Linux.
//!
//! Windows / Linux 原生菜单栏骨架，与 macOS 菜单共享同一套 action id 与事件协议。

#[cfg(not(target_os = "macos"))]
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Manager, Wry,
};

#[cfg(not(target_os = "macos"))]
pub fn build_app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(
                app,
                "browse_connections",
                "Manage Connections",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "close_tab", "Close Tab", true, Some("CmdOrCtrl+W"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "open_settings", "Settings...", true, Some("CmdOrCtrl+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[&MenuItem::with_id(
            app,
            "find",
            "Find",
            true,
            Some("CmdOrCtrl+F"),
        )?],
    )?;

    Menu::with_items(app, &[&file_menu, &view_menu])
}

#[cfg(not(target_os = "macos"))]
pub fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().0.as_str() {
        "open_settings" => {
            let _ = crate::settings_window::open_settings_window(app.clone());
        }
        "browse_connections" => {
            let _ = crate::connections_window::open_connections_window(app.clone());
        }
        action => emit_menu_action_to_main(app, action),
    }
}

#[cfg(not(target_os = "macos"))]
fn emit_menu_action_to_main(app: &AppHandle, action: &str) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };

    let _ = main.show();
    let _ = main.set_focus();
    let _ = main.emit("thread:menu-action", action);
}
