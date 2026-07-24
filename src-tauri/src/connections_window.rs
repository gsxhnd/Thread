use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

pub const CONNECTIONS_WINDOW_LABEL: &str = "connections";

pub fn ensure_connections_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(CONNECTIONS_WINDOW_LABEL) {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        CONNECTIONS_WINDOW_LABEL,
        WebviewUrl::App("/?window=connections".into()),
    )
    .title("Remote Management")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 400.0)
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
pub fn open_connections_window(app: tauri::AppHandle) -> Result<(), String> {
    ensure_connections_window(&app)
}
