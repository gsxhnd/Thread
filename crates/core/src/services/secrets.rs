//! Secret environment variable storage.
//!
//! 机密变量值**不写入工作区文件**（工作区文件会被 Git 跟踪、被 diff、被分享）。
//! 值保存在应用数据目录 `secrets/<workspace-key>.json`，工作区文件里只保留
//! `{ "key": ..., "value": "", "secret": true }` 占位。
//!
//! 说明：这是「本地忽略文件」方案，不是系统凭据库加密。阶段 5 评估脚本运行时
//! 时会一并决定是否接入 macOS Keychain / Windows Credential Manager。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::secrets_dir;
use crate::error::AppResult;
use crate::workspace::files;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SecretFile {
    workspace_path: String,
    values: HashMap<String, String>,
}

/// Stable, filename-safe key for a workspace path (FNV-1a).
pub fn workspace_key(root: &Path) -> String {
    let text = root.to_string_lossy();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn secret_path(root: &Path) -> std::path::PathBuf {
    secrets_dir().join(format!("{}.json", workspace_key(root)))
}

fn storage_key(environment_rel: &str, variable_key: &str) -> String {
    format!("{environment_rel}|{variable_key}")
}

pub fn load(root: &Path) -> HashMap<String, String> {
    let path = secret_path(root);
    if !path.exists() {
        return HashMap::new();
    }
    files::read_json::<SecretFile>(&path)
        .map(|file| file.values)
        .unwrap_or_default()
}

fn save(root: &Path, values: HashMap<String, String>) -> AppResult<()> {
    let file = SecretFile {
        workspace_path: root.to_string_lossy().into_owned(),
        values,
    };
    files::write_json(&secret_path(root), &file)
}

pub fn get(root: &Path, environment_rel: &str, variable_key: &str) -> Option<String> {
    load(root)
        .get(&storage_key(environment_rel, variable_key))
        .cloned()
}

pub fn set(root: &Path, environment_rel: &str, variable_key: &str, value: &str) -> AppResult<()> {
    let mut values = load(root);
    values.insert(
        storage_key(environment_rel, variable_key),
        value.to_string(),
    );
    save(root, values)
}

pub fn remove(root: &Path, environment_rel: &str, variable_key: &str) -> AppResult<()> {
    let mut values = load(root);
    values.remove(&storage_key(environment_rel, variable_key));
    save(root, values)
}

pub fn remove_environment(root: &Path, environment_rel: &str) -> AppResult<()> {
    let mut values = load(root);
    let prefix = format!("{environment_rel}|");
    values.retain(|key, _| !key.starts_with(&prefix));
    save(root, values)
}

/// Secret values that belong to the given environment, keyed by variable name.
pub fn for_environment(root: &Path, environment_rel: &str) -> HashMap<String, String> {
    let prefix = format!("{environment_rel}|");
    load(root)
        .into_iter()
        .filter_map(|(key, value)| {
            key.strip_prefix(&prefix)
                .map(|name| (name.to_string(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_is_stable_and_path_specific() {
        let a = workspace_key(Path::new("/tmp/workspace-a"));
        let b = workspace_key(Path::new("/tmp/workspace-b"));
        assert_eq!(a, workspace_key(Path::new("/tmp/workspace-a")));
        assert_ne!(a, b);
        assert!(!a.contains('/'));
    }
}
