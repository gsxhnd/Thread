//! 外观偏好：light / dark 两档，跟随系统由启动时决定。
//!
//! 选择写入 `config.toml` 的 `ui_layout` section（沿用核心提供的
//! `ThreadConfigStore::set_section`），下次启动时恢复。

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, WindowAppearance};
use thread_core::config::{SECTION_UI_LAYOUT, ThreadConfigStore};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn label(self, cx: &App) -> &'static str {
        match self {
            Self::System => crate::tr!(cx, "跟随系统", "System"),
            Self::Light => crate::tr!(cx, "浅色", "Light"),
            Self::Dark => crate::tr!(cx, "深色", "Dark"),
        }
    }

    pub const ALL: [ThemePreference; 3] = [
        ThemePreference::System,
        ThemePreference::Light,
        ThemePreference::Dark,
    ];

    fn resolve(self, system_dark: bool) -> ThemeMode {
        match self {
            Self::Dark => ThemeMode::Dark,
            Self::Light => ThemeMode::Light,
            Self::System => {
                if system_dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                }
            }
        }
    }
}

fn system_dark(cx: &App) -> bool {
    matches!(
        cx.window_appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// `ui_layout` section. Theme and locale share one JSON object so saving one
/// does not erase the other.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct UiLayout {
    pub theme: Option<String>,
    pub locale: Option<String>,
    pub editor_layout: Option<String>,
    pub sidebar_collapsed: Option<bool>,
}

pub(crate) fn load_layout(config: &ThreadConfigStore) -> UiLayout {
    config
        .get_section(SECTION_UI_LAYOUT)
        .and_then(|json| serde_json::from_str::<UiLayout>(&json).ok())
        .unwrap_or_default()
}

pub(crate) fn save_layout(config: &ThreadConfigStore, layout: &UiLayout) {
    let Ok(json) = serde_json::to_string(layout) else {
        return;
    };
    if let Err(error) = config.set_section(SECTION_UI_LAYOUT, &json) {
        tracing::warn!(%error, "failed to persist ui layout");
    }
}

/// Reads the stored preference and applies the theme before the first window opens.
pub fn init(config: &ThreadConfigStore, cx: &mut App) {
    let preference = stored(config);
    let dark = system_dark(cx);
    Theme::change(preference.resolve(dark), None, cx);
}

/// Current persisted preference (falls back to System).
pub fn stored(config: &ThreadConfigStore) -> ThemePreference {
    load_layout(config)
        .theme
        .as_deref()
        .and_then(|name| match name {
            "light" => Some(ThemePreference::Light),
            "dark" => Some(ThemePreference::Dark),
            _ => None,
        })
        .unwrap_or_default()
}

/// Switches the theme and persists the choice.
pub fn set(config: &ThreadConfigStore, preference: ThemePreference, cx: &mut App) {
    let dark = system_dark(cx);
    Theme::change(preference.resolve(dark), None, cx);

    let mut layout = load_layout(config);
    layout.theme = Some(
        match preference {
            ThemePreference::System => "system",
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        }
        .to_string(),
    );
    save_layout(config, &layout);
    tracing::info!(
        theme = layout.theme.as_deref().unwrap_or("system"),
        "theme changed"
    );
}

/// 请求编辑区和响应区的摆放。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorLayout {
    /// 请求在上，响应在下。
    #[default]
    Stacked,
    /// 请求在左，响应在右。
    Columns,
}

pub fn editor_layout(config: &ThreadConfigStore) -> EditorLayout {
    match load_layout(config).editor_layout.as_deref() {
        Some("columns") => EditorLayout::Columns,
        _ => EditorLayout::Stacked,
    }
}

pub fn store_editor_layout(config: &ThreadConfigStore, layout: EditorLayout) {
    let mut stored = load_layout(config);
    stored.editor_layout = Some(
        match layout {
            EditorLayout::Stacked => "stacked",
            EditorLayout::Columns => "columns",
        }
        .to_string(),
    );
    save_layout(config, &stored);
}

pub fn sidebar_collapsed(config: &ThreadConfigStore) -> bool {
    load_layout(config).sidebar_collapsed.unwrap_or(false)
}

pub fn store_sidebar_collapsed(config: &ThreadConfigStore, collapsed: bool) {
    let mut stored = load_layout(config);
    stored.sidebar_collapsed = Some(collapsed);
    save_layout(config, &stored);
}
