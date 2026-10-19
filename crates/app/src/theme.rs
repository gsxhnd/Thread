//! 外观偏好：跟随系统、浅色/深色，以及若干内置配色主题。
//!
//! 选择写入 `config.toml` 的 `ui_layout` section（沿用核心提供的
//! `ThreadConfigStore::set_section`），下次启动时恢复。

use std::rc::Rc;

use gpui_kit::component::theme::{ThemeConfig, ThemeRegistry};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, SharedString, WindowAppearance};
use thread_core::config::{SECTION_UI_LAYOUT, ThreadConfigStore};

use serde::{Deserialize, Serialize};

/// 设置页里可选的外观。`System` / `Light` / `Dark` 使用内置默认配色，其余是内置的命名主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
    Solarized,
    Nord,
    Dracula,
}

/// 一套完整配色。界面会读到的表面、侧边栏、列表选中和弹出层都在这里，
/// 避免只改几个底色时按钮和选中态还停在另一套默认色上。
struct Palette {
    name: &'static str,
    dark: bool,
    background: u32,
    foreground: u32,
    sidebar: u32,
    sidebar_foreground: u32,
    sidebar_border: u32,
    sidebar_accent: u32,
    border: u32,
    muted: u32,
    muted_foreground: u32,
    primary: u32,
    primary_foreground: u32,
    accent: u32,
    accent_foreground: u32,
    secondary: u32,
    input_border: u32,
    list_active: u32,
    popover: u32,
}

impl ThemePreference {
    pub const ALL: [ThemePreference; 6] = [
        ThemePreference::System,
        ThemePreference::Light,
        ThemePreference::Dark,
        ThemePreference::Solarized,
        ThemePreference::Nord,
        ThemePreference::Dracula,
    ];

    pub fn label(self, cx: &App) -> &'static str {
        match self {
            Self::System => crate::tr!(cx, "跟随系统", "System"),
            Self::Light => crate::tr!(cx, "浅色", "Light"),
            Self::Dark => crate::tr!(cx, "深色", "Dark"),
            Self::Solarized => crate::tr!(cx, "Solarized 浅色", "Solarized Light"),
            Self::Nord => crate::tr!(cx, "Nord 深色", "Nord"),
            Self::Dracula => crate::tr!(cx, "Dracula 深色", "Dracula"),
        }
    }

    /// 写入配置文件的稳定标识。
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Solarized => "solarized",
            Self::Nord => "nord",
            Self::Dracula => "dracula",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preference| preference.code() == code)
    }

    /// 设置页色块用的代表色：背景、文字、强调色。
    pub fn swatch(self) -> (u32, u32, u32) {
        match self.palette() {
            Some(palette) => (palette.background, palette.foreground, palette.primary),
            None => (0xf6f5f2, 0x1c1917, 0x0f766e),
        }
    }

    fn palette(self) -> Option<Palette> {
        Some(match self {
            // 浅色：暖纸底，侧边栏再沉一点，强调色用青绿，和中性灰默认主题拉开。
            Self::Light => Palette {
                name: "Thread Light",
                dark: false,
                background: 0xf6f5f2,
                foreground: 0x1c1917,
                sidebar: 0xeeece7,
                sidebar_foreground: 0x1c1917,
                sidebar_border: 0xe4e0d8,
                sidebar_accent: 0xe0dbd1,
                border: 0xe4e0d8,
                muted: 0xeeece7,
                muted_foreground: 0x78716c,
                primary: 0x0f766e,
                primary_foreground: 0xf6f5f2,
                accent: 0xe7e2d8,
                accent_foreground: 0x1c1917,
                secondary: 0xfbfaf7,
                input_border: 0xd6d1c7,
                list_active: 0xd5f5f0,
                popover: 0xfffdf8,
            },
            // 深色：分层表面，文字用暖灰而不是纯白，选中条用深青。
            Self::Dark => Palette {
                name: "Thread Dark",
                dark: true,
                background: 0x14161a,
                foreground: 0xe7e5e4,
                sidebar: 0x0f1114,
                sidebar_foreground: 0xe7e5e4,
                sidebar_border: 0x2a2d34,
                sidebar_accent: 0x23262e,
                border: 0x2a2d34,
                muted: 0x1c1f26,
                muted_foreground: 0xa8a29e,
                primary: 0x2dd4bf,
                primary_foreground: 0x042f2e,
                accent: 0x23262e,
                accent_foreground: 0xe7e5e4,
                secondary: 0x1c1f26,
                input_border: 0x3f4450,
                list_active: 0x134e4a,
                popover: 0x1c1f26,
            },
            Self::Solarized => Palette {
                name: "Thread Solarized Light",
                dark: false,
                background: 0xfdf6e3,
                foreground: 0x586e75,
                sidebar: 0xeee8d5,
                sidebar_foreground: 0x586e75,
                sidebar_border: 0xd9d1b8,
                sidebar_accent: 0xe6dfc8,
                border: 0xd9d1b8,
                muted: 0xeee8d5,
                muted_foreground: 0x839496,
                primary: 0x268bd2,
                primary_foreground: 0xfdf6e3,
                accent: 0xe6dfc8,
                accent_foreground: 0x586e75,
                secondary: 0xfdf6e3,
                input_border: 0xd9d1b8,
                list_active: 0xd6eee8,
                popover: 0xfdf6e3,
            },
            Self::Nord => Palette {
                name: "Thread Nord",
                dark: true,
                background: 0x2e3440,
                foreground: 0xd8dee9,
                sidebar: 0x3b4252,
                sidebar_foreground: 0xd8dee9,
                sidebar_border: 0x4c566a,
                sidebar_accent: 0x434c5e,
                border: 0x4c566a,
                muted: 0x3b4252,
                muted_foreground: 0x81a1c1,
                primary: 0x88c0d0,
                primary_foreground: 0x2e3440,
                accent: 0x434c5e,
                accent_foreground: 0xd8dee9,
                secondary: 0x3b4252,
                input_border: 0x4c566a,
                list_active: 0x4c566a,
                popover: 0x3b4252,
            },
            Self::Dracula => Palette {
                name: "Thread Dracula",
                dark: true,
                background: 0x282a36,
                foreground: 0xf8f8f2,
                sidebar: 0x21222c,
                sidebar_foreground: 0xf8f8f2,
                sidebar_border: 0x44475a,
                sidebar_accent: 0x44475a,
                border: 0x44475a,
                muted: 0x343746,
                muted_foreground: 0x6272a4,
                primary: 0xbd93f9,
                primary_foreground: 0x282a36,
                accent: 0x44475a,
                accent_foreground: 0xf8f8f2,
                secondary: 0x343746,
                input_border: 0x44475a,
                list_active: 0x44475a,
                popover: 0x343746,
            },
            Self::System => return None,
        })
    }

    /// 解析成要用的明暗模式。`System` 跟随系统外观。
    fn mode(self, system_dark: bool) -> ThemeMode {
        match self {
            Self::Dark | Self::Nord | Self::Dracula => ThemeMode::Dark,
            Self::Light | Self::Solarized => ThemeMode::Light,
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

/// 命名主题序列化成一个单条目的主题集 JSON，供注册表加载。
fn theme_set_json(palette: &Palette) -> String {
    let hex = |value: u32| format!("#{value:06x}");
    let config = serde_json::json!({
        "name": palette.name,
        "mode": if palette.dark { "dark" } else { "light" },
        "colors": {
            "background": hex(palette.background),
            "foreground": hex(palette.foreground),
            "caret": hex(palette.foreground),
            "sidebar.background": hex(palette.sidebar),
            "sidebar.foreground": hex(palette.sidebar_foreground),
            "sidebar.border": hex(palette.sidebar_border),
            "sidebar.accent.background": hex(palette.sidebar_accent),
            "sidebar.accent.foreground": hex(palette.sidebar_foreground),
            "border": hex(palette.border),
            "input.border": hex(palette.input_border),
            "ring": hex(palette.primary),
            "muted.background": hex(palette.muted),
            "muted.foreground": hex(palette.muted_foreground),
            "primary.background": hex(palette.primary),
            "primary.foreground": hex(palette.primary_foreground),
            "accent.background": hex(palette.accent),
            "accent.foreground": hex(palette.accent_foreground),
            "secondary.background": hex(palette.secondary),
            "secondary.foreground": hex(palette.foreground),
            "popover.background": hex(palette.popover),
            "popover.foreground": hex(palette.foreground),
            "list.background": hex(palette.background),
            "list.active.background": hex(palette.list_active),
            "list.active.border": hex(palette.primary),
            "list.hover.background": hex(palette.accent),
            "selection.background": hex(palette.list_active),
        }
    });
    serde_json::json!({ "name": palette.name, "themes": [config] }).to_string()
}

/// 把命名主题登记进主题注册表，返回可以直接套用的配置。
fn register(palette: &Palette, cx: &mut App) -> Option<Rc<ThemeConfig>> {
    let set = theme_set_json(palette);
    let registry = ThemeRegistry::global_mut(cx);
    if let Err(error) = registry.load_themes_from_str(&set) {
        tracing::warn!(%error, "failed to register theme");
    }
    ThemeRegistry::global(cx)
        .themes()
        .get(&SharedString::from(palette.name))
        .cloned()
}

/// 套用一个外观：先换上对应的浅色/深色配置，再切到目标模式。
fn apply(preference: ThemePreference, cx: &mut App) {
    let dark = system_dark(cx);
    let mode = preference.mode(dark);
    // 跟随系统时套用同一套 Thread 浅色/深色，而不是组件库的中性默认色。
    let concrete = match preference {
        ThemePreference::System if dark => ThemePreference::Dark,
        ThemePreference::System => ThemePreference::Light,
        other => other,
    };
    let (mut light, mut dark_config) = {
        let registry = ThemeRegistry::global(cx);
        (
            registry.default_light_theme().clone(),
            registry.default_dark_theme().clone(),
        )
    };
    if let Some(palette) = concrete.palette()
        && let Some(config) = register(&palette, cx)
    {
        if palette.dark {
            dark_config = config;
        } else {
            light = config;
        }
    }
    // 第一次切换会创建全局主题，之后再换配置并重新套用当前模式。
    Theme::change(mode, None, cx);
    Theme::update(cx, |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark_config;
    });
    Theme::change(mode, None, cx);
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
    apply(preference, cx);
}

/// Current persisted preference (falls back to System).
pub fn stored(config: &ThreadConfigStore) -> ThemePreference {
    load_layout(config)
        .theme
        .as_deref()
        .and_then(ThemePreference::from_code)
        .unwrap_or_default()
}

/// Switches the theme and persists the choice.
pub fn set(config: &ThreadConfigStore, preference: ThemePreference, cx: &mut App) {
    apply(preference, cx);

    let mut layout = load_layout(config);
    layout.theme = Some(preference.code().to_string());
    save_layout(config, &layout);
    tracing::info!(theme = preference.code(), "theme changed");
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::theme::ThemeSet;

    /// 命名主题的 JSON 必须能被 gpui-kit 的主题集 schema 原样解析，
    /// 否则设置页选了主题也只会静默失效（register 里只记 warn）。
    #[test]
    fn palette_json_matches_theme_schema() {
        for preference in ThemePreference::ALL {
            let Some(palette) = preference.palette() else {
                continue;
            };
            let set: ThemeSet =
                serde_json::from_str(&theme_set_json(&palette)).expect("valid theme set json");
            assert_eq!(set.themes.len(), 1);
            let theme = &set.themes[0];
            assert_eq!(theme.name, palette.name);
            assert_eq!(theme.mode.is_dark(), palette.dark);
            let hex = |value: u32| format!("#{value:06x}");
            assert_eq!(
                theme.colors.background.as_deref(),
                Some(hex(palette.background).as_str())
            );
            assert_eq!(
                theme.colors.primary.as_deref(),
                Some(hex(palette.primary).as_str())
            );
            assert_eq!(
                theme.colors.muted_foreground.as_deref(),
                Some(hex(palette.muted_foreground).as_str())
            );
        }
    }

    /// 配置文件里的旧值（light/dark/system）和新值都要能读回来。
    #[test]
    fn preference_codes_round_trip() {
        for preference in ThemePreference::ALL {
            assert_eq!(
                ThemePreference::from_code(preference.code()),
                Some(preference)
            );
        }
        assert_eq!(ThemePreference::from_code("nonsense"), None);
        assert_eq!(ThemePreference::from_code(""), None);
    }
}
