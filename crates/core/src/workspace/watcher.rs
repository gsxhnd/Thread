//! External-change detection for the open workspace.
//!
//! 阶段 0 要求「应用外修改 / 切换 Git 分支后可刷新请求列表」。这里用轻量的
//! 文件系统指纹轮询（不引入 `notify` 依赖）：
//! 每 2 秒比对一次「路径 + mtime + 大小」指纹，指纹变化才重新扫描；
//! 只有索引摘要真正变化时才 emit `workspace://changed`，避免自写触发的抖动。

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::WorkspaceState;
use super::index::WorkspaceIndex;
use super::layout;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Reason reported when an external change is picked up.
pub const REASON_EXTERNAL_CHANGE: &str = "external-change";

/// Cheap fingerprint over the workspace tree (path, mtime, size).
pub fn fs_signature(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    walk(root, &mut hasher, 0);
    hasher.finish()
}

fn walk(dir: &Path, hasher: &mut DefaultHasher, depth: usize) {
    if depth > 24 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if path.is_dir() {
            if layout::is_skipped_dir(&name) {
                continue;
            }
            hasher.write(name.as_bytes());
            walk(&path, hasher, depth + 1);
            continue;
        }
        let Ok(metadata) = path.metadata() else {
            continue;
        };
        hasher.write(name.as_bytes());
        hasher.write_u64(metadata.len());
        if let Ok(modified) = metadata.modified()
            && let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            hasher.write_u128(elapsed.as_nanos());
        }
    }
}

/// Spawns the background poller on the given Tokio runtime.
///
/// `on_changed` receives the freshly scanned index whenever an external
/// modification actually altered the index digest. The callback is invoked from
/// the polling task, so implementations must hand the value back to their own
/// event loop (GPUI's `cx.spawn`, a channel, …) rather than touching UI state.
pub fn spawn<F>(state: Arc<WorkspaceState>, on_changed: F)
where
    F: Fn(&WorkspaceIndex) + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            let Some(root) = state.root_if_open() else {
                continue;
            };
            let signature = fs_signature(&root);
            if !state.signature_changed(signature) {
                continue;
            }

            let before = state.index_digest();
            match state.refresh() {
                Ok(index) => {
                    if Some(index.digest.clone()) != before {
                        on_changed(&index);
                    }
                }
                Err(error) => {
                    tracing::warn!(?error, "workspace refresh failed");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn signature_changes_when_files_change() {
        let root = std::env::temp_dir().join(format!("thread-watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("api")).unwrap();
        fs::write(root.join("api/a.http.json"), "{}").unwrap();
        let first = fs_signature(&root);

        // Same content, same path -> fingerprint only depends on mtime/size.
        let signature_ignores_git = {
            fs::create_dir_all(root.join(".git")).unwrap();
            fs::write(root.join(".git/HEAD"), "refs/heads/main").unwrap();
            fs_signature(&root)
        };
        assert_eq!(first, signature_ignores_git);

        std::thread::sleep(Duration::from_millis(10));
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(root.join("api/a.http.json"))
            .unwrap();
        use std::io::Write;
        file.write_all(b" ").unwrap();
        file.flush().unwrap();
        let _ = file.sync_all();
        let _ = SystemTime::now();
        let second = fs_signature(&root);
        assert_ne!(first, second);

        let _ = fs::remove_dir_all(&root);
    }
}
