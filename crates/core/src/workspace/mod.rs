//! Workspace: the Git-friendly file store that is the source of truth.
//!
//! 目录约定（见 docs/03.database.md §2.1）：
//!
//! ```text
//! my-workspace/
//! ├── thread.workspace.json     工作区配置
//! ├── .gitignore                机密/本地产物忽略模板
//! ├── environments/             *.env.json
//! └── my-api/                   Collection 文件夹（可嵌套）
//!     ├── collection.json
//!     └── users/list-users.http.json
//! ```

pub mod backup;
pub mod files;
pub mod formats;
pub mod har;
pub mod index;
pub mod layout;
pub mod openapi;
pub mod ops;
pub mod postman;
pub mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use serde_json::Value;

use crate::db::new_id;
use crate::error::{AppError, AppResult};

pub use formats::{AuthSpec, EnvironmentFile, HttpRequestDoc, KeyValue, WorkspaceConfig};
pub use index::{EnvironmentSummary, SearchHit, WorkspaceIndex};

/// Workspace snapshot returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub index: WorkspaceIndex,
    pub config: WorkspaceConfig,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetail {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub path: String,
    pub collection_path: String,
    pub collection_name: String,
    /// Collection-level headers that this request inherits (display only).
    pub inherited_headers: Vec<KeyValue>,
    pub inherited_auth: AuthSpec,
    /// Full stored document (all protocols) for the raw JSON view.
    pub raw: Value,
    /// Present for HTTP requests; other protocols are read-only in Phase 1.
    pub http: Option<HttpRequestDoc>,
    /// Kept for damaged files so the UI can explain why nothing loads.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDetail {
    pub id: String,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub has_meta: bool,
    pub auth: AuthSpec,
    pub headers: Vec<KeyValue>,
    pub pre_script: String,
    pub post_script: String,
    pub parent_path: Option<String>,
    pub request_count: usize,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentDetail {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_global: bool,
    pub is_active: bool,
    pub variables: Vec<EnvVariableView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVariableView {
    pub key: String,
    /// Always empty for `secret` variables; use `env_variable_reveal` to read them.
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
    pub description: Option<String>,
    pub has_stored_value: bool,
}

struct OpenWorkspace {
    root: PathBuf,
    config: WorkspaceConfig,
    index: WorkspaceIndex,
}

/// Global state of the currently open workspace (one at a time in Phase 1).
pub struct WorkspaceState {
    inner: RwLock<Option<OpenWorkspace>>,
    signature: Mutex<u64>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(None),
            signature: Mutex::new(0),
        }
    }

    pub fn root_if_open(&self) -> Option<PathBuf> {
        self.inner.read().as_ref().map(|open| open.root.clone())
    }

    pub fn is_open(&self) -> bool {
        self.inner.read().is_some()
    }

    pub fn root(&self) -> AppResult<PathBuf> {
        self.inner
            .read()
            .as_ref()
            .map(|open| open.root.clone())
            .ok_or_else(|| AppError::Validation("no workspace is open".into()))
    }

    pub fn index(&self) -> AppResult<WorkspaceIndex> {
        self.inner
            .read()
            .as_ref()
            .map(|open| open.index.clone())
            .ok_or_else(|| AppError::Validation("no workspace is open".into()))
    }

    pub fn index_digest(&self) -> Option<String> {
        self.inner
            .read()
            .as_ref()
            .map(|open| open.index.digest.clone())
    }

    pub fn config(&self) -> AppResult<WorkspaceConfig> {
        self.inner
            .read()
            .as_ref()
            .map(|open| open.config.clone())
            .ok_or_else(|| AppError::Validation("no workspace is open".into()))
    }

    pub fn snapshot(&self) -> AppResult<WorkspaceSnapshot> {
        let guard = self.inner.read();
        let open = guard
            .as_ref()
            .ok_or_else(|| AppError::Validation("no workspace is open".into()))?;
        Ok(WorkspaceSnapshot {
            index: open.index.clone(),
            config: open.config.clone(),
            warnings: open.index.warnings.clone(),
        })
    }

    pub fn open(&self, root: &Path) -> AppResult<WorkspaceSnapshot> {
        let open = Self::load(root)?;
        *self.signature.lock() = watcher::fs_signature(&open.root);
        let snapshot = WorkspaceSnapshot {
            index: open.index.clone(),
            config: open.config.clone(),
            warnings: open.index.warnings.clone(),
        };
        *self.inner.write() = Some(open);
        Ok(snapshot)
    }

    fn load(root: &Path) -> AppResult<OpenWorkspace> {
        if !root.exists() {
            return Err(AppError::NotFound(format!(
                "workspace directory does not exist: {}",
                root.display()
            )));
        }
        if !root.is_dir() {
            return Err(AppError::Validation(format!(
                "workspace path is not a directory: {}",
                root.display()
            )));
        }
        let outcome = index::scan(root, None)?;
        Ok(OpenWorkspace {
            root: root.to_path_buf(),
            config: outcome.config,
            index: outcome.index,
        })
    }

    pub fn close(&self) {
        *self.inner.write() = None;
    }

    /// Re-scans the workspace from disk (external edits, Git branch switch, `git pull`).
    pub fn refresh(&self) -> AppResult<WorkspaceIndex> {
        let (root, config) = {
            let guard = self.inner.read();
            let open = guard
                .as_ref()
                .ok_or_else(|| AppError::Validation("no workspace is open".into()))?;
            (open.root.clone(), open.config.clone())
        };

        // Re-read the workspace file so hand edits to it are picked up too.
        let config_path = root.join(layout::WORKSPACE_FILE);
        let config = if config_path.exists() {
            files::read_json::<WorkspaceConfig>(&config_path).unwrap_or(config)
        } else {
            config
        };

        let outcome = index::scan(&root, Some(config))?;
        {
            let mut guard = self.inner.write();
            if let Some(open) = guard.as_mut() {
                open.config = outcome.config.clone();
                open.index = outcome.index.clone();
            }
        }
        *self.signature.lock() = watcher::fs_signature(&root);
        Ok(outcome.index)
    }

    /// Persists the workspace config file and refreshes the index.
    pub fn update_config<F>(&self, mutate: F) -> AppResult<WorkspaceSnapshot>
    where
        F: FnOnce(&mut WorkspaceConfig),
    {
        let (root, mut config) = {
            let guard = self.inner.read();
            let open = guard
                .as_ref()
                .ok_or_else(|| AppError::Validation("no workspace is open".into()))?;
            (open.root.clone(), open.config.clone())
        };
        mutate(&mut config);
        let path = root.join(layout::WORKSPACE_FILE);
        files::write_json(&path, &config)?;
        self.open(&root)
    }

    /// True when the filesystem fingerprint differs from the last known one.
    pub fn signature_changed(&self, signature: u64) -> bool {
        let mut guard = self.signature.lock();
        if *guard == signature {
            return false;
        }
        *guard = signature;
        true
    }
}

pub type SharedWorkspace = Arc<WorkspaceState>;

/// Initializes a folder as a Thread workspace (idempotent for existing files).
///
/// 只补齐缺失的文件：已存在的 `thread.workspace.json` / `.gitignore` 不会被覆盖。
pub fn init_workspace(root: &Path, name: &str) -> AppResult<WorkspaceConfig> {
    files::ensure_dir(root)?;
    files::ensure_dir(&root.join(layout::ENVIRONMENTS_DIR))?;

    let config_path = root.join(layout::WORKSPACE_FILE);
    let config = if config_path.exists() {
        files::read_json::<WorkspaceConfig>(&config_path)?
    } else {
        let config = WorkspaceConfig {
            id: Some(new_id()),
            name: name.to_string(),
            active_environment: Some("global".to_string()),
            ..WorkspaceConfig::new(name)
        };
        files::write_json(&config_path, &config)?;
        config
    };

    files::write_text_if_absent(
        &root.join(layout::GITIGNORE_FILE),
        layout::gitignore_template(),
    )?;

    let global_env = root.join(layout::ENVIRONMENTS_DIR).join("global.env.json");
    if !global_env.exists() {
        let mut environment = EnvironmentFile::new("Global");
        environment.id = Some(new_id());
        files::write_json(&global_env, &environment)?;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::formats::{HttpRequestDoc, RequestFile};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("thread-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn creates_workspace_files_only_once() {
        let root = temp_dir("create");
        let config = init_workspace(&root, "Demo").unwrap();
        assert_eq!(config.name, "Demo");
        assert!(config.id.is_some());
        assert!(root.join(layout::WORKSPACE_FILE).exists());
        assert!(root.join(layout::GITIGNORE_FILE).exists());
        assert!(root.join("environments/global.env.json").exists());

        let before = std::fs::read_to_string(root.join(layout::WORKSPACE_FILE)).unwrap();
        init_workspace(&root, "Renamed").unwrap();
        let after = std::fs::read_to_string(root.join(layout::WORKSPACE_FILE)).unwrap();
        assert_eq!(
            before, after,
            "existing workspace file must not be overwritten"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opens_refreshes_and_tracks_external_changes() {
        let root = temp_dir("refresh");
        init_workspace(&root, "Demo").unwrap();
        let state = WorkspaceState::new();
        state.open(&root).unwrap();
        assert!(state.is_open());
        assert_eq!(state.index().unwrap().request_count, 0);

        // Simulate an external `git pull` that adds a request file.
        let collection = ops::create_collection(&root, None, "Pulled API", None).unwrap();
        let mut doc = HttpRequestDoc::new("Pulled Request");
        doc.url = "https://example.com".into();
        files::write_json(
            &root.join(&collection.0).join("pulled.http.json"),
            &RequestFile::Http(doc),
        )
        .unwrap();

        let index = state.refresh().unwrap();
        assert_eq!(index.request_count, 1);
        assert_eq!(state.index().unwrap().collections[0].name, "Pulled API");

        state.close();
        assert!(!state.is_open());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn open_rejects_missing_directory() {
        let state = WorkspaceState::new();
        let error = state
            .open(Path::new("/definitely/not/here-thread-test"))
            .unwrap_err();
        assert_eq!(error.code(), "NOT_FOUND");
    }

    #[test]
    fn update_config_writes_file_and_refreshes() {
        let root = temp_dir("config");
        init_workspace(&root, "Demo").unwrap();
        let state = WorkspaceState::new();
        state.open(&root).unwrap();
        let snapshot = state
            .update_config(|config| {
                config.name = "Renamed WS".into();
                config.settings.timeout_ms = Some(1234);
            })
            .unwrap();
        assert_eq!(snapshot.index.name, "Renamed WS");
        assert_eq!(snapshot.config.settings.timeout_ms, Some(1234));
        let _ = std::fs::remove_dir_all(&root);
    }
}
