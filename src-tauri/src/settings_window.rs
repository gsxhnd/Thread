use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW_LABEL: &str = "settings";

pub fn ensure_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("/?window=settings".into()),
    )
    .title("Settings")
    .inner_size(800.0, 640.0)
    .min_inner_size(560.0, 480.0)
    .center()
    .decorations(true)
    .title_bar_style(TitleBarStyle::Overlay)
    .hidden_title(true)
    .transparent(true)
    .shadow(true)
    .resizable(true)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    crate::apply_macos_window_effects(&window);

    Ok(())
}

#[tauri::command]
pub fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    ensure_settings_window(&app)
}
