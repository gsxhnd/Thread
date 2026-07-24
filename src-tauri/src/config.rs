//! Unified on-disk configuration store backed by `config.toml`.
//!
//! 应用的统一配置中心。前端各类 UI 状态以 section 为单位保存在
//! `~/.config/thread/config.toml`。每个 section 以不透明的 `serde_json::Value`
//! 存储，因此无需在 Rust 侧重复定义前端的状态结构。这是一个可复用的通用
//! 配置骨架：新增业务 section 时，只需在 `ConfigFile` 与相关辅助函数中扩展即可。

use crate::atomic_file::{atomic_write, backup_corrupt_file};
use crate::sync_mutex::lock_or_recover;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_VERSION: u32 = 1;
const APP_DIR_NAME: &str = "thread";
const CONFIG_CHANGED_EVENT: &str = "thread:config-changed";

pub const SECTION_APP_SETTINGS: &str = "app_settings";
pub const SECTION_UI_LAYOUT: &str = "ui_layout";

const UI_SECTIONS: [&str; 2] = [SECTION_APP_SETTINGS, SECTION_UI_LAYOUT];

/// Typed representation of the whole `config.toml` file.
///
/// `config.toml` 文件的强类型映射。各 UI 区段以不透明的 `serde_json::Value`
/// 存储；缺省字段在序列化时被跳过，使配置文件保持精简。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_settings: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_layout: Option<Value>,
}

fn default_config_version() -> u32 {
    CONFIG_VERSION
}

/// Returns the platform config directory with a `thread` subfolder.
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
///
/// 配置文件的线程安全内存缓存。整个配置在内存中由 `Mutex` 保护，任何写操作
/// 都会立即回写磁盘（write-through）；作为 Tauri 的全局 state 在多个命令与
/// 窗口间共享。
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

    pub fn remove_section(&self, section: &str) -> Result<(), String> {
        let mut config = lock_or_recover(&self.config);
        set_section_value(&mut config, section, None)?;
        save_config(&self.path, &config)
    }

    pub fn load_ui_sections(&self) -> HashMap<String, String> {
        let config = lock_or_recover(&self.config);
        let mut sections = HashMap::new();
        for section in UI_SECTIONS {
            if let Some(value) = section_value(&config, section) {
                if let Ok(serialized) = serde_json::to_string(value) {
                    sections.insert(section.to_string(), serialized);
                }
            }
        }
        sections
    }
}

fn section_value<'a>(config: &'a ConfigFile, section: &str) -> Option<&'a Value> {
    match section {
        SECTION_APP_SETTINGS => config.app_settings.as_ref(),
        SECTION_UI_LAYOUT => config.ui_layout.as_ref(),
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

#[tauri::command]
pub fn get_config_load_warnings(store: State<'_, Arc<ThreadConfigStore>>) -> Vec<String> {
    store.take_load_warnings()
}

#[tauri::command]
pub fn get_config_dir() -> String {
    config_dir().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn get_config_file_path() -> String {
    config_file_path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn load_config_sections(store: State<'_, Arc<ThreadConfigStore>>) -> HashMap<String, String> {
    store.load_ui_sections()
}

#[tauri::command]
pub fn get_config_section(
    store: State<'_, Arc<ThreadConfigStore>>,
    section: String,
) -> Option<String> {
    store.get_section(&section)
}

#[tauri::command]
pub fn set_config_section(
    app: AppHandle,
    store: State<'_, Arc<ThreadConfigStore>>,
    section: String,
    value: String,
) -> Result<(), String> {
    store.set_section(&section, &value)?;
    if let Err(error) = app.emit(CONFIG_CHANGED_EVENT, section) {
        tracing::warn!(?error, "failed to emit {CONFIG_CHANGED_EVENT}");
    }
    Ok(())
}

#[tauri::command]
pub fn remove_config_section(
    app: AppHandle,
    store: State<'_, Arc<ThreadConfigStore>>,
    section: String,
) -> Result<(), String> {
    store.remove_section(&section)?;
    if let Err(error) = app.emit(CONFIG_CHANGED_EVENT, section) {
        tracing::warn!(?error, "failed to emit {CONFIG_CHANGED_EVENT}");
    }
    Ok(())
}
