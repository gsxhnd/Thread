//! Tauri application entry point and command registry.

mod atomic_file;
mod commands;
mod config;
mod connections_window;
mod db;
mod desktop_menu;
mod editor_window;
mod error;
#[cfg(target_os = "macos")]
mod macos_menu;
mod models;
mod runtime;
mod services;
mod settings_window;
mod sync_mutex;
mod themes;

use std::sync::Arc;

use config::ThreadConfigStore;
use db::{db_path, DbState};
use services::AiState;

/// Applies the translucent vibrancy material to a macOS window.
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let config_store = Arc::new(ThreadConfigStore::new());
    let db = Arc::new(
        DbState::open(db_path()).unwrap_or_else(|e| {
            panic!("failed to open database at {:?}: {e}", db_path());
        }),
    );
    let ai_state = Arc::new(AiState::new());

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(config_store)
        .manage(db)
        .manage(ai_state)
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
            // Project
            commands::project_create,
            commands::project_update,
            commands::project_delete,
            commands::project_list,
            commands::project_get,
            // Volume / Chapter
            commands::volume_create,
            commands::volume_update,
            commands::volume_delete,
            commands::volume_reorder,
            commands::chapter_create,
            commands::chapter_get,
            commands::chapter_save,
            commands::chapter_update_status,
            commands::chapter_delete,
            commands::chapter_reorder,
            commands::chapter_tree,
            // Plotline
            commands::plotline_create,
            commands::plotline_update,
            commands::plotline_delete,
            commands::plotline_attach,
            commands::plotline_detach,
            commands::outline_list,
            // Character
            commands::character_create,
            commands::character_update,
            commands::character_delete,
            commands::character_list,
            commands::relation_create,
            commands::relation_delete,
            commands::relation_graph,
            // Worldbuilding
            commands::location_create,
            commands::location_update,
            commands::location_delete,
            commands::faction_create,
            commands::faction_update,
            commands::faction_delete,
            commands::item_create,
            commands::item_update,
            commands::item_delete,
            commands::worldbuilding_list,
            // Timeline
            commands::event_create,
            commands::event_update,
            commands::event_delete,
            commands::event_reorder,
            commands::event_link,
            commands::event_unlink,
            commands::timeline_list,
            // Foreshadowing
            commands::foreshadowing_create,
            commands::foreshadowing_update,
            commands::foreshadowing_delete,
            commands::foreshadowing_link,
            commands::foreshadowing_list,
            commands::foreshadowing_unresolved,
            // Stats
            commands::stats_overview,
            commands::stats_calendar,
            commands::stats_streak,
            commands::goal_set,
            commands::goal_get,
            // AI
            commands::ai_config_set,
            commands::ai_config_get,
            commands::ai_complete,
            commands::ai_cancel,
            // Export
            commands::export_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
