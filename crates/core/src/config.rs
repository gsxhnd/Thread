//! Unified on-disk configuration store backed by `config.toml`.
//!
//! 应用级配置（与工作区无关）：HTTP 默认设置、UI 布局、最近打开的工作区。
//! 集合/请求/环境属于工作区文件，不在这里（见 workspace 模块）。
//!
//! 配置文件位于 `~/.config/thread/config.toml`；每个 section 以不透明的
//! `serde_json::Value` 保存，新增 section 时只需扩展 `ConfigFile` 与映射函数。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::atomic_file::{atomic_write, backup_corrupt_file};
use crate::db::now_ms;
use crate::services::proxy::settings::CaptureSettings;
use crate::sync_mutex::lock_or_recover;

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_VERSION: u32 = 1;
const APP_DIR_NAME: &str = "thread";

pub const SECTION_APP_SETTINGS: &str = "app_settings";
pub const SECTION_UI_LAYOUT: &str = "ui_layout";
pub const SECTION_SESSION: &str = "session";
pub const SECTION_PROXY_SETTINGS: &str = "proxy_settings";

const UI_SECTIONS: [&str; 2] = [SECTION_APP_SETTINGS, SECTION_UI_LAYOUT];

/// HTTP defaults used when a request/collection does not override them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_timeout_ms")]
    pub default_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    /// Responses larger than this are streamed to a file instead of being inlined.
    #[serde(default = "default_max_inline_body_bytes")]
    pub max_inline_body_bytes: u64,
    /// 0 disables automatic history trimming.
    #[serde(default = "default_history_limit")]
    pub history_limit: i64,
    #[serde(default = "default_history_body_storage")]
    pub history_body_storage: String,
}

fn default_timeout_ms() -> u64 {
    30_000
}
fn default_true() -> bool {
    true
}
fn default_max_inline_body_bytes() -> u64 {
    1024 * 1024
}
fn default_history_limit() -> i64 {
    1000
}
fn default_history_body_storage() -> String {
    "inline".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_timeout_ms: default_timeout_ms(),
            follow_redirects: default_true(),
            verify_ssl: default_true(),
            max_inline_body_bytes: default_max_inline_body_bytes(),
            history_limit: default_history_limit(),
            history_body_storage: default_history_body_storage(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspace {
    pub path: String,
    pub name: String,
    pub last_opened_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionData {
    pub last_workspace_path: Option<String>,
    pub recent_workspaces: Vec<RecentWorkspace>,
}

/// Typed representation of the whole `config.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_settings: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_layout: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_settings: Option<Value>,
}

fn default_config_version() -> u32 {
    CONFIG_VERSION
}

pub fn config_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME);
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn config_file_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

/// Thread-safe, in-memory cache of the config file with write-through saves.
pub struct ThreadConfigStore {
    path: PathBuf,
    config: Mutex<ConfigFile>,
    load_warnings: Mutex<Vec<String>>,
}

impl ThreadConfigStore {
    pub fn new() -> Self {
        let path = config_file_path();
        let (config, warnings) = load_config(&path);
        Self {
            path,
            config: Mutex::new(config),
            load_warnings: Mutex::new(warnings),
        }
    }

    pub fn take_load_warnings(&self) -> Vec<String> {
        lock_or_recover(&self.load_warnings).drain(..).collect()
    }

    pub fn get_section(&self, section: &str) -> Option<String> {
        let config = lock_or_recover(&self.config);
        section_value(&config, section).and_then(|value| serde_json::to_string(value).ok())
    }

    pub fn set_section(&self, section: &str, json: &str) -> Result<(), String> {
        let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let mut config = lock_or_recover(&self.config);
        set_section_value(&mut config, section, Some(value))?;
        save_config(&self.path, &config)
    }

    pub fn load_ui_sections(&self) -> HashMap<String, String> {
        let config = lock_or_recover(&self.config);
        let mut sections = HashMap::new();
        for section in UI_SECTIONS {
            if let Some(value) = section_value(&config, section)
                && let Ok(serialized) = serde_json::to_string(value)
            {
                sections.insert(section.to_string(), serialized);
            }
        }
        sections
    }

    pub fn app_settings(&self) -> AppSettings {
        let config = lock_or_recover(&self.config);
        config
            .app_settings
            .as_ref()
            .and_then(|value| serde_json::from_value::<AppSettings>(value.clone()).ok())
            .unwrap_or_default()
    }

    pub fn proxy_capture_settings(&self) -> CaptureSettings {
        let config = lock_or_recover(&self.config);
        config
            .proxy_settings
            .as_ref()
            .and_then(|value| serde_json::from_value::<CaptureSettings>(value.clone()).ok())
            .unwrap_or_default()
    }

    pub fn set_proxy_capture_settings(&self, settings: &CaptureSettings) -> Result<(), String> {
        let value = serde_json::to_value(settings).map_err(|error| error.to_string())?;
        let mut config = lock_or_recover(&self.config);
        set_section_value(&mut config, SECTION_PROXY_SETTINGS, Some(value))?;
        save_config(&self.path, &config)
    }

    pub fn set_app_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let value = serde_json::to_value(settings).map_err(|error| error.to_string())?;
        let mut config = lock_or_recover(&self.config);
        set_section_value(&mut config, SECTION_APP_SETTINGS, Some(value))?;
        save_config(&self.path, &config)
    }

    pub fn session(&self) -> SessionData {
        let config = lock_or_recover(&self.config);
        config
            .session
            .as_ref()
            .and_then(|value| serde_json::from_value::<SessionData>(value.clone()).ok())
            .unwrap_or_default()
    }

    fn set_session(&self, session: &SessionData) -> Result<(), String> {
        let value = serde_json::to_value(session).map_err(|error| error.to_string())?;
        let mut config = lock_or_recover(&self.config);
        set_section_value(&mut config, SECTION_SESSION, Some(value))?;
        save_config(&self.path, &config)
    }

    /// Records a successful workspace open in the recent list (deduplicated, newest first).
    pub fn touch_recent_workspace(&self, path: &str, name: &str) -> Result<(), String> {
        let mut session = self.session();
        session.last_workspace_path = Some(path.to_string());
        session.recent_workspaces.retain(|item| item.path != path);
        session.recent_workspaces.insert(
            0,
            RecentWorkspace {
                path: path.to_string(),
                name: name.to_string(),
                last_opened_at: now_ms(),
            },
        );
        session.recent_workspaces.truncate(10);
        self.set_session(&session)
    }

    pub fn forget_recent_workspace(&self, path: &str) -> Result<(), String> {
        let mut session = self.session();
        session.recent_workspaces.retain(|item| item.path != path);
        if session.last_workspace_path.as_deref() == Some(path) {
            session.last_workspace_path = None;
        }
        self.set_session(&session)
    }
}

impl Default for ThreadConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

fn section_value<'a>(config: &'a ConfigFile, section: &str) -> Option<&'a Value> {
    match section {
        SECTION_APP_SETTINGS => config.app_settings.as_ref(),
        SECTION_UI_LAYOUT => config.ui_layout.as_ref(),
        SECTION_SESSION => config.session.as_ref(),
        SECTION_PROXY_SETTINGS => config.proxy_settings.as_ref(),
        _ => None,
    }
}

fn set_section_value(
    config: &mut ConfigFile,
    section: &str,
    value: Option<Value>,
) -> Result<(), String> {
    match section {
        SECTION_APP_SETTINGS => config.app_settings = value,
        SECTION_UI_LAYOUT => config.ui_layout = value,
        SECTION_SESSION => config.session = value,
        SECTION_PROXY_SETTINGS => config.proxy_settings = value,
        _ => {
            return Err(format!("unknown config section: {section}"));
        }
    }
    Ok(())
}

fn load_config(path: &Path) -> (ConfigFile, Vec<String>) {
    if !path.exists() {
        return (ConfigFile::default(), Vec::new());
    }

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            let message = format!(
                "Failed to read {}: {error}. Using default configuration.",
                path.display()
            );
            eprintln!("thread: {message}");
            return (ConfigFile::default(), vec![message]);
        }
    };

    if let Ok(config) = toml::from_str::<ConfigFile>(&content) {
        return (config, Vec::new());
    }

    let backup_path = backup_corrupt_file(path);
    let backup_note = backup_path
        .as_ref()
        .map(|backup| backup.display().to_string())
        .unwrap_or_else(|| "backup failed".to_string());
    let message = format!(
        "Configuration file {} is invalid and was reset to defaults. A backup was saved to {backup_note}.",
        path.display()
    );
    eprintln!("thread: {message}");
    (ConfigFile::default(), vec![message])
}

fn save_config(path: &Path, config: &ConfigFile) -> Result<(), String> {
    let content = toml::to_string_pretty(config).map_err(|error| error.to_string())?;
    atomic_write(path, &content)
}

/// Local paths surfaced in the settings page (data vs. workspace separation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub config_dir: String,
    pub config_file: String,
    pub data_dir: String,
    pub db_file: String,
    pub log_dir: String,
}

pub fn app_data_paths() -> AppPaths {
    AppPaths {
        config_dir: config_dir().to_string_lossy().into_owned(),
        config_file: config_file_path().to_string_lossy().into_owned(),
        data_dir: crate::db::data_dir().to_string_lossy().into_owned(),
        db_file: crate::db::db_path().to_string_lossy().into_owned(),
        log_dir: crate::db::log_dir().to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_settings_defaults_are_stable() {
        let settings = AppSettings::default();
        assert_eq!(settings.default_timeout_ms, 30_000);
        assert!(settings.follow_redirects);
        assert!(settings.verify_ssl);
        assert_eq!(settings.max_inline_body_bytes, 1_048_576);
    }

    #[test]
    fn partial_app_settings_json_uses_defaults() {
        let settings: AppSettings = serde_json::from_str(r#"{"defaultTimeoutMs": 5000}"#).unwrap();
        assert_eq!(settings.default_timeout_ms, 5000);
        assert!(settings.verify_ssl);
    }

    #[test]
    fn recent_workspaces_are_deduplicated_and_capped() {
        let store = ThreadConfigStore {
            path: std::env::temp_dir().join(format!("thread-cfg-{}.toml", std::process::id())),
            config: Mutex::new(ConfigFile::default()),
            load_warnings: Mutex::new(Vec::new()),
        };
        for index in 0..12 {
            store
                .touch_recent_workspace(&format!("/tmp/ws-{index}"), &format!("ws-{index}"))
                .unwrap();
        }
        store.touch_recent_workspace("/tmp/ws-5", "ws-5").unwrap();
        let session = store.session();
        assert_eq!(session.recent_workspaces.len(), 10);
        assert_eq!(session.recent_workspaces[0].path, "/tmp/ws-5");
        assert_eq!(
            session
                .recent_workspaces
                .iter()
                .filter(|item| item.path == "/tmp/ws-5")
                .count(),
            1
        );
    }
}
