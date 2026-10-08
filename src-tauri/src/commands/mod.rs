//! Tauri command handlers, grouped by feature area.
//!
//! 约定（见 docs/04.ipc-api.md §1）：每个 command 接收单个 `payload` 对象，
//! 字段使用小驼峰；command 只做校验与编排，实际逻辑落在 `workspace::ops`
//! 与 `services::*`。

pub mod collection;
pub mod environment;
pub mod history;
pub mod http;
pub mod request;
pub mod workspace;

use serde::Serialize;

/// Local paths surfaced in the settings page (data vs. workspace separation).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub config_dir: String,
    pub config_file: String,
    pub data_dir: String,
    pub db_file: String,
}

#[tauri::command]
pub fn app_paths() -> AppPaths {
    crate::config::app_data_paths()
}
