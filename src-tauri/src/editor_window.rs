use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

pub const EDITOR_WINDOW_PREFIX: &str = "editor-";

fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn editor_window_label(path: &str, source: &str, session_id: Option<&str>) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    source.hash(&mut hasher);
    if let Some(session_id) = session_id {
        session_id.hash(&mut hasher);
    }
    format!("{EDITOR_WINDOW_PREFIX}{:x}", hasher.finish())
}

fn file_name_from_path(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn build_editor_url(path: &str, source: &str, session_id: Option<&str>) -> String {
    let mut url = format!(
        "/?window=editor&path={}",
        percent_encode_component(path)
    );
    url.push_str(&format!("&source={}", percent_encode_component(source)));
    if let Some(session_id) = session_id {
        url.push_str(&format!(
            "&sessionId={}",
            percent_encode_component(session_id)
        ));
    }
    url
}

pub fn ensure_editor_window(
    app: &tauri::AppHandle,
    path: String,
    source: String,
    session_id: Option<String>,
) -> Result<(), String> {
    let label = editor_window_label(&path, &source, session_id.as_deref());
    let title = file_name_from_path(&path);

    if let Some(existing) = app.get_webview_window(&label) {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = build_editor_url(&path, &source, session_id.as_deref());
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title(&title)
        .inner_size(900.0, 640.0)
        .min_inner_size(480.0, 320.0)
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
pub fn open_editor_window(
    app: tauri::AppHandle,
    path: String,
    source: String,
    session_id: Option<String>,
) -> Result<(), String> {
    ensure_editor_window(&app, path, source, session_id)
}
