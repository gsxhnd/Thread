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
