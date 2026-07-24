//! External color theme storage under `~/.config/thread/themes`.
//!
//! 配色主题外置存储：启动时创建 `themes` 目录，将内置主题种子文件复制到用户
//! 配置目录（不覆盖已有文件），并支持列出与读取外部 CSS 主题供前端动态加载。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::config_dir;

pub const BUILTIN_COLOR_THEME_ID: &str = "default";

const SEED_THEMES: &[(&str, &str)] = &[
    ("zinc", include_str!("../resources/themes/zinc.css")),
    ("slate", include_str!("../resources/themes/slate.css")),
    ("stone", include_str!("../resources/themes/stone.css")),
    ("rose", include_str!("../resources/themes/rose.css")),
    ("blue", include_str!("../resources/themes/blue.css")),
    ("green", include_str!("../resources/themes/green.css")),
    ("violet", include_str!("../resources/themes/violet.css")),
    (
        "catppuccin",
        include_str!("../resources/themes/catppuccin.css"),
    ),
    ("nord", include_str!("../resources/themes/nord.css")),
];

/// Summary returned to the frontend theme picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorThemeInfo {
    pub id: String,
    pub source: String,
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

fn is_valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Ensures `~/.config/thread/themes` exists and seeds bundled themes when missing.
pub fn ensure_themes_initialized() {
    let dir = themes_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }

    for (id, css) in SEED_THEMES {
        let css_path = dir.join(format!("{id}.css"));
        if !css_path.exists() {
            let _ = fs::write(&css_path, css);
        }
    }

    let readme_path = dir.join("README.md");
    if !readme_path.exists() {
        let _ = fs::write(
            &readme_path,
            include_str!("../resources/themes/README.md"),
        );
    }
}

fn list_external_themes() -> Vec<ColorThemeInfo> {
    let dir = themes_dir();
    let mut themes = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return themes,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("css") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_valid_theme_id(stem) {
            continue;
        }

        themes.push(ColorThemeInfo {
            id: stem.to_string(),
            source: "external".to_string(),
        });
    }

    themes.sort_by(|a, b| a.id.cmp(&b.id));
    themes
}

pub fn list_color_themes() -> Vec<ColorThemeInfo> {
    ensure_themes_initialized();

    let mut themes = vec![ColorThemeInfo {
        id: BUILTIN_COLOR_THEME_ID.to_string(),
        source: "builtin".to_string(),
    }];

    themes.extend(list_external_themes());
    themes
}

pub fn read_color_theme_css(theme_id: &str) -> Result<String, String> {
    if theme_id == BUILTIN_COLOR_THEME_ID {
        return Err("built-in default theme is bundled in the frontend".to_string());
    }
    if !is_valid_theme_id(theme_id) {
        return Err(format!("invalid theme id: {theme_id}"));
    }

    ensure_themes_initialized();
    let path = themes_dir().join(format!("{theme_id}.css"));
    if !path.exists() {
        return Err(format!("theme not found: {theme_id}"));
    }
    fs::read_to_string(&path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_themes_dir() -> String {
    themes_dir().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn list_color_themes_command() -> Vec<ColorThemeInfo> {
    list_color_themes()
}

#[tauri::command]
pub fn read_color_theme_css_command(theme_id: String) -> Result<String, String> {
    read_color_theme_css(&theme_id)
}

/// No-op state holder so theme init can run from setup if needed later.
pub struct ThemesState;

impl ThemesState {
    pub fn new() -> Self {
        ensure_themes_initialized();
        Self
    }
}
