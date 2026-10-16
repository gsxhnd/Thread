//! Workspace on-disk layout conventions.
//!
//! 约定工作区目录结构、文件名规则、文件名冲突处理与路径安全校验。
//! 这些规则是「文件即数据」（docs/00.overview.md §7.5）的落地基础。

use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const WORKSPACE_FILE: &str = "thread.workspace.json";
pub const ENVIRONMENTS_DIR: &str = "environments";
pub const COLLECTION_FILE: &str = "collection.json";
pub const GITIGNORE_FILE: &str = ".gitignore";
/// Prefix used for deterministic IDs derived from a path when a file has no `id` field.
pub const PATH_ID_PREFIX: &str = "path:";
pub const FILE_VERSION: u32 = 1;

const REQUEST_SUFFIXES: [(&str, &str); 5] = [
    (".http.json", "http"),
    (".grpc.json", "grpc"),
    (".ws.json", "websocket"),
    (".graphql.json", "graphql"),
    (".mqtt.json", "mqtt"),
];

const SKIPPED_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "dist",
    "dist-ssr",
    "build",
    "coverage",
    ".git",
];

/// Protocol for a request file name, e.g. `list-users.http.json` -> `http`.
pub fn protocol_from_file_name(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_lowercase();
    REQUEST_SUFFIXES
        .iter()
        .find(|(suffix, _)| lower.ends_with(suffix))
        .map(|(_, protocol)| *protocol)
}

pub fn file_suffix_for_protocol(protocol: &str) -> AppResult<&'static str> {
    REQUEST_SUFFIXES
        .iter()
        .find(|(_, name)| *name == protocol)
        .map(|(suffix, _)| *suffix)
        .ok_or_else(|| AppError::Validation(format!("unsupported protocol: {protocol}")))
}

pub fn is_environment_file(file_name: &str) -> bool {
    file_name.to_lowercase().ends_with(".env.json")
}

/// Directories that never take part in collection discovery.
pub fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_DIRS.contains(&name)
}

/// Converts a display name into a filesystem-safe, Git-friendly file name.
///
/// 中文等非 ASCII 字符保留原样（文件系统与 Git 均可正常处理），空格与
/// Windows 非法字符统一折叠为 `-`，避免跨平台文件名问题。
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for ch in name.trim().chars() {
        let invalid = matches!(
            ch,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'
        ) || ch.is_control();
        if ch.is_whitespace() || invalid {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
            continue;
        }
        out.push(ch);
        last_dash = ch == '-';
    }
    let trimmed = out.trim_matches('-').to_string();
    let truncated: String = trimmed.chars().take(60).collect();
    truncated.trim_end_matches(['.', '-']).to_string()
}

/// Picks a free path inside `dir` by appending `-2`, `-3`, ... on conflicts.
pub fn unique_path(dir: &Path, slug: &str, suffix: &str) -> PathBuf {
    let base = if slug.is_empty() { "untitled" } else { slug };
    let mut candidate = dir.join(format!("{base}{suffix}"));
    let mut index = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}-{index}{suffix}"));
        index += 1;
    }
    candidate
}

/// Normalizes a workspace-relative path received from the frontend.
///
/// 拒绝绝对路径、`..` 与空路径，避免越权读写工作区之外的文件。
pub fn normalize_relative(rel: &str) -> AppResult<PathBuf> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("path must not be empty".into()));
    }
    // Absolute paths (POSIX, Windows drive letters, UNC) are never workspace-relative.
    if trimmed.starts_with('/') || trimmed.starts_with('\\') || Path::new(trimmed).is_absolute() {
        return Err(AppError::Validation(format!(
            "path must be workspace-relative: {rel}"
        )));
    }
    let trimmed = trimmed.trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::Validation("path must not be empty".into()));
    }
    let candidate = PathBuf::from(trimmed);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => {
                return Err(AppError::Validation(format!(
                    "path must stay inside the workspace: {rel}"
                )));
            }
        }
    }
    Ok(candidate)
}

/// Resolves a workspace-relative path and guarantees it stays inside `root`.
pub fn resolve_inside(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let rel_path = normalize_relative(rel)?;
    let joined = root.join(&rel_path);
    let normalized = normalize_lexically(&joined);
    let root_normalized = normalize_lexically(root);
    if !normalized.starts_with(&root_normalized) {
        return Err(AppError::Validation(format!(
            "path escapes the workspace: {rel}"
        )));
    }
    Ok(normalized)
}

/// Lexical normalization (no filesystem access), used for containment checks.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Workspace-relative path with forward slashes (stable across platforms).
pub fn to_relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `.gitignore` template created with a new workspace.
pub fn gitignore_template() -> &'static str {
    "# Thread / 灰线 —— 工作区忽略规则\n\
     # 机密变量值保存在应用数据目录（不入库、不入 Git）；本地覆盖文件与运行产物在此排除。\n\
     .thread/\n\
     *.local.env.json\n\
     *.local.*\n\
     .DS_Store\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_protocol_from_file_name() {
        assert_eq!(
            protocol_from_file_name("list-users.http.json"),
            Some("http")
        );
        assert_eq!(protocol_from_file_name("hello.GRPC.json"), Some("grpc"));
        assert_eq!(protocol_from_file_name("chat.ws.json"), Some("websocket"));
        assert_eq!(protocol_from_file_name("collection.json"), None);
        assert_eq!(protocol_from_file_name("thread.workspace.json"), None);
    }

    #[test]
    fn slugify_keeps_cjk_and_folds_separators() {
        assert_eq!(slugify("Create User"), "Create-User");
        assert_eq!(slugify("用户 登录"), "用户-登录");
        assert_eq!(slugify("a/b:c*d?"), "a-b-c-d");
        assert_eq!(slugify("  "), "");
    }

    #[test]
    fn unique_path_appends_counter_on_conflict() {
        let dir = std::env::temp_dir().join(format!("thread-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_path(&dir, "login", ".http.json");
        assert!(first.ends_with("login.http.json"));
        std::fs::write(&first, "{}").unwrap();
        let second = unique_path(&dir, "login", ".http.json");
        assert!(second.ends_with("login-2.http.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(normalize_relative("../etc/passwd").is_err());
        assert!(normalize_relative("/etc/passwd").is_err());
        assert!(normalize_relative("").is_err());
        assert!(normalize_relative("my-api/list.http.json").is_ok());

        let root = Path::new("/tmp/workspace");
        assert!(resolve_inside(root, "a/b.json").is_ok());
        assert!(resolve_inside(root, "../../etc/passwd").is_err());
    }

    #[test]
    fn skipped_dirs_cover_vcs_and_build_outputs() {
        assert!(is_skipped_dir(".git"));
        assert!(is_skipped_dir(".thread"));
        assert!(is_skipped_dir("node_modules"));
        assert!(!is_skipped_dir("my-api"));
    }
}
