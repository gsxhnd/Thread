//! 界面语言。目前只有中文和英文，选择写入 `config.toml` 的 `ui_layout.locale`。
//!
//! 未保存过选择时，按系统首选语言决定：中文用中文，其余用英文。

use gpui_kit::{App, Global};
use thread_core::config::ThreadConfigStore;

use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    Zh,
    En,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::Zh, Locale::En];

    /// 语言菜单上的名字，始终用该语言自己的写法。
    pub fn label(self) -> &'static str {
        match self {
            Self::Zh => "中文",
            Self::En => "English",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "zh" => Some(Self::Zh),
            "en" => Some(Self::En),
            _ => None,
        }
    }
}

struct AppLocale(Locale);

impl Global for AppLocale {}

pub fn current(cx: &App) -> Locale {
    cx.try_global::<AppLocale>()
        .map(|locale| locale.0)
        .unwrap_or(Locale::Zh)
}

pub fn is_en(cx: &App) -> bool {
    current(cx) == Locale::En
}

/// Picks an already-built string. Both sides are formatted by the caller.
pub fn owned(locale: Locale, zh: String, en: String) -> String {
    if locale == Locale::En { en } else { zh }
}

/// 读取已保存的语言；没有记录时用系统语言。
pub fn init(config: &ThreadConfigStore, cx: &mut App) {
    let locale = stored(config);
    cx.set_global(AppLocale(locale));
    tracing::info!(locale = locale.code(), "locale ready");
}

pub fn stored(config: &ThreadConfigStore) -> Locale {
    theme::load_layout(config)
        .locale
        .as_deref()
        .and_then(Locale::from_code)
        .unwrap_or_else(system_locale)
}

pub fn set(config: &ThreadConfigStore, locale: Locale, cx: &mut App) {
    cx.set_global(AppLocale(locale));
    let mut layout = theme::load_layout(config);
    layout.locale = Some(locale.code().to_string());
    theme::save_layout(config, &layout);
    tracing::info!(locale = locale.code(), "locale changed");
}

fn system_locale() -> Locale {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && !value.is_empty()
        {
            return locale_from_tag(&value);
        }
    }
    macos_preferred_language().unwrap_or(Locale::Zh)
}

#[cfg(target_os = "macos")]
fn macos_preferred_language() -> Option<Locale> {
    let output = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let tag = text.split('"').find(|part| {
        let part = part.trim();
        !part.is_empty() && part.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    })?;
    Some(locale_from_tag(tag))
}

#[cfg(not(target_os = "macos"))]
fn macos_preferred_language() -> Option<Locale> {
    None
}

fn locale_from_tag(tag: &str) -> Locale {
    let tag = tag.trim().trim_matches('"');
    let tag = tag.split(['.', '@']).next().unwrap_or(tag);
    let tag = tag.to_ascii_lowercase();
    if tag.starts_with("zh") || tag.is_empty() || tag == "c" || tag == "posix" {
        Locale::Zh
    } else {
        Locale::En
    }
}

#[macro_export]
macro_rules! tr {
    ($cx:expr, $zh:expr, $en:expr) => {{ if $crate::i18n::is_en($cx) { $en } else { $zh } }};
}

#[cfg(test)]
mod tests {
    use super::{Locale, locale_from_tag};

    #[test]
    fn chinese_tags_stay_chinese() {
        assert_eq!(locale_from_tag("zh_CN.UTF-8"), Locale::Zh);
        assert_eq!(locale_from_tag("zh-Hans-CN"), Locale::Zh);
        assert_eq!(locale_from_tag("C"), Locale::Zh);
    }

    #[test]
    fn other_tags_use_english() {
        assert_eq!(locale_from_tag("en_US.UTF-8"), Locale::En);
        assert_eq!(locale_from_tag("ja_JP"), Locale::En);
    }
}
