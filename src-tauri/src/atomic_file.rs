//! Cross-platform atomic file writes and corrupt-file backup helpers.
//!
//! 跨平台原子写文件与损坏文件备份工具。先写入同目录临时文件再 rename，
//! 避免写入中断导致配置文件损坏。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Writes `content` to `path` atomically via a same-directory temp file + rename.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let temp_path = temp_file_path(path);
    {
        let mut file = fs::File::create(&temp_path).map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }

    if path.exists() {
        #[cfg(windows)]
        {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
    }

    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        error.to_string()
    })
}

/// Copies a corrupt file to `<name>.bak.<unix_ms>` in the same directory.
pub fn backup_corrupt_file(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    let file_name = path.file_name()?.to_string_lossy();
    let backup_name = format!("{file_name}.bak.{timestamp}");
    let backup_path = path.with_file_name(backup_name);

    match fs::copy(path, &backup_path) {
        Ok(_) => Some(backup_path),
        Err(error) => {
            eprintln!(
                "thread: failed to backup corrupt file {}: {error}",
                path.display()
            );
            None
        }
    }
}

fn temp_file_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    path.with_file_name(format!("{file_name}.{timestamp}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn atomic_write_replaces_existing_content() {
        let dir = env::temp_dir().join(format!("thread-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        atomic_write(&path, "version = 1\n").unwrap();
        atomic_write(&path, "version = 2\n").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "version = 2\n");

        let _ = fs::remove_dir_all(&dir);
    }
}
