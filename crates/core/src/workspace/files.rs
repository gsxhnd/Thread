//! File read/write helpers for workspace documents.
//!
//! 所有工作区写入都走 `atomic_file::atomic_write`（同目录临时文件 + rename），
//! 避免应用崩溃或断电留下半个 JSON 文件；删除操作一律「移入回收站」而非 unlink，
//! 以符合「不直接删除用户数据」的约定。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::atomic_file::atomic_write;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};

use super::layout;

pub fn read_text(path: &Path) -> AppResult<String> {
    fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(path.display().to_string())
        } else {
            AppError::Io(format!("{}: {error}", path.display()))
        }
    })
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<T> {
    let text = read_text(path)?;
    serde_json::from_str::<T>(&text)
        .map_err(|error| AppError::Validation(format!("{}: invalid JSON: {error}", path.display())))
}

pub fn read_json_value(path: &Path) -> AppResult<serde_json::Value> {
    read_json(path)
}

/// Pretty JSON with a trailing newline: diff-friendly and Git-friendly.
pub fn to_json_string<T: Serialize>(value: &T) -> AppResult<String> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let text = to_json_string(value)?;
    write_text(path, &text)
}

pub fn write_text(path: &Path, text: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, text).map_err(AppError::Io)
}

/// Writes the file only when the path does not exist yet.
pub fn write_text_if_absent(path: &Path, text: &str) -> AppResult<bool> {
    if path.exists() {
        return Ok(false);
    }
    write_text(path, text)?;
    Ok(true)
}

pub fn ensure_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

/// Moves a file or directory into the local trash instead of deleting it.
pub fn move_to_trash(
    workspace_root: &Path,
    target: &Path,
    trash_root: &Path,
) -> AppResult<PathBuf> {
    if !target.exists() {
        return Err(AppError::NotFound(target.display().to_string()));
    }
    let relative = layout::to_relative_string(workspace_root, target);
    let destination = trash_root
        .join(now_ms().to_string())
        .join(if relative.is_empty() {
            target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "item".to_string())
        } else {
            relative
        });

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::rename(target, &destination) {
        Ok(()) => Ok(destination),
        Err(_) => {
            // Cross-device (or跨卷) fallback: copy then remove.
            copy_recursive(target, &destination)?;
            if target.is_dir() {
                fs::remove_dir_all(target)?;
            } else {
                fs::remove_file(target)?;
            }
            Ok(destination)
        }
    }
}

fn copy_recursive(source: &Path, destination: &Path) -> AppResult<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

/// Reads and parses a workspace-relative JSON document.
pub fn read_relative_json<T: DeserializeOwned>(root: &Path, relative: &str) -> AppResult<T> {
    let path = layout::resolve_inside(root, relative)?;
    read_json(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thread-files-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn json_writes_are_pretty_and_end_with_newline() {
        let dir = temp_dir("json");
        let path = dir.join("collection.json");
        let meta = super::super::formats::CollectionMeta::new("My API");
        write_json(&path, &meta).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("\n"));
        assert!(text.contains("\n  \"name\": \"My API\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_to_trash_preserves_relative_structure() {
        let root = temp_dir("trash-root");
        let trash = temp_dir("trash-store");
        let target = root.join("my-api").join("list.http.json");
        ensure_dir(target.parent().unwrap()).unwrap();
        fs::write(&target, "{}").unwrap();

        let moved = move_to_trash(&root, &target, &trash).unwrap();
        assert!(!target.exists());
        assert!(moved.exists());
        assert!(moved.ends_with("my-api/list.http.json"));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn write_text_if_absent_never_overwrites() {
        let dir = temp_dir("absent");
        let path = dir.join(".gitignore");
        assert!(write_text_if_absent(&path, "first").unwrap());
        assert!(!write_text_if_absent(&path, "second").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_json_reports_missing_file_as_not_found() {
        let dir = temp_dir("missing");
        let error = read_json::<serde_json::Value>(&dir.join("nope.json")).unwrap_err();
        assert_eq!(error.code(), "NOT_FOUND");
        let _ = fs::remove_dir_all(&dir);
    }
}
