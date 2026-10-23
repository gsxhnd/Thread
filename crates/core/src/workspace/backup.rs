//! Safe workspace-only backup and restore helpers.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

use super::formats::WorkspaceConfig;
use super::{files, layout};

const BACKUP_FORMAT: &str = "thread-workspace-backup";
const BACKUP_VERSION: u32 = 1;
const MANIFEST_FILE: &str = ".thread-backup.json";
const CONTENT_DIR: &str = "workspace";
const MAX_WORKSPACE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBackupManifest {
    pub format: String,
    pub version: u32,
    pub created_at: i64,
    pub workspace_name: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub includes_git_history: bool,
    pub includes_local_app_data: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub path: PathBuf,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub workspace_path: PathBuf,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Default)]
struct CopyStats {
    files: u64,
    bytes: u64,
}

/// Creates a non-overwriting workspace-only backup directory under `destination_parent`.
/// `.git`, generated build folders, symlinks, and local app data outside the workspace are excluded.
pub fn backup_workspace(source: &Path, destination_parent: &Path) -> AppResult<BackupReport> {
    let source = canonical_directory(source, "workspace")?;
    let destination_parent = canonical_directory(destination_parent, "backup destination")?;
    if destination_parent.starts_with(&source) {
        return Err(AppError::Validation(
            "backup destination must not be inside the workspace".into(),
        ));
    }
    let workspace_file = source.join(layout::WORKSPACE_FILE);
    require_regular_file(&workspace_file, "workspace config")?;
    let config: WorkspaceConfig = files::read_json(&workspace_file)?;
    let name = config.name;
    let stage = destination_parent.join(format!(".thread-backup-stage-{}", new_id()));
    fs::create_dir(&stage)?;
    let workspace_copy = stage.join(CONTENT_DIR);
    let result = (|| {
        let mut stats = CopyStats::default();
        copy_tree(&source, &workspace_copy, &mut stats)?;
        let copied_config: WorkspaceConfig =
            files::read_json(&workspace_copy.join(layout::WORKSPACE_FILE))?;
        let manifest = WorkspaceBackupManifest {
            format: BACKUP_FORMAT.into(),
            version: BACKUP_VERSION,
            created_at: now_ms(),
            workspace_name: copied_config.name,
            file_count: stats.files,
            total_bytes: stats.bytes,
            includes_git_history: false,
            includes_local_app_data: false,
        };
        files::write_json(&stage.join(MANIFEST_FILE), &manifest)?;
        Ok::<_, AppError>((stats, manifest))
    })();
    let (stats, manifest) = match result {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    let stem = format!("{}-backup-{}", layout::slugify(&name), manifest.created_at);
    let final_path = layout::unique_path(&destination_parent, &stem, "");
    if let Err(error) = fs::rename(&stage, &final_path) {
        let _ = fs::remove_dir_all(&stage);
        return Err(AppError::Io(error.to_string()));
    }
    Ok(BackupReport {
        path: final_path,
        file_count: stats.files,
        total_bytes: stats.bytes,
    })
}

/// Restores a valid backup as a new sibling workspace; it never overwrites an existing folder.
pub fn restore_workspace(backup: &Path, destination_parent: &Path) -> AppResult<RestoreReport> {
    let backup = canonical_directory(backup, "backup")?;
    let destination_parent = canonical_directory(destination_parent, "restore destination")?;
    if destination_parent.starts_with(&backup) {
        return Err(AppError::Validation(
            "restore destination must not be inside the backup".into(),
        ));
    }
    let manifest_path = backup.join(MANIFEST_FILE);
    require_regular_file(&manifest_path, "backup manifest")?;
    let manifest: WorkspaceBackupManifest = files::read_json(&manifest_path)?;
    if manifest.format != BACKUP_FORMAT || manifest.version != BACKUP_VERSION {
        return Err(AppError::Validation(
            "unsupported Thread workspace backup format".into(),
        ));
    }
    if manifest.includes_git_history || manifest.includes_local_app_data {
        return Err(AppError::Validation(
            "this backup contains unsupported external state".into(),
        ));
    }
    let workspace_copy = backup.join(CONTENT_DIR);
    require_real_directory(&workspace_copy, "backup workspace")?;
    require_regular_file(
        &workspace_copy.join(layout::WORKSPACE_FILE),
        "backup workspace config",
    )?;
    let config: WorkspaceConfig = files::read_json(&workspace_copy.join(layout::WORKSPACE_FILE))?;
    let stage = destination_parent.join(format!(".thread-restore-stage-{}", new_id()));
    fs::create_dir(&stage)?;
    let result = copy_tree(&workspace_copy, &stage, &mut CopyStats::default());
    let stats = match result {
        Ok(stats) => stats,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    if let Err(error) = files::read_json::<WorkspaceConfig>(&stage.join(layout::WORKSPACE_FILE)) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    let final_path = layout::unique_path(
        &destination_parent,
        &format!("{}-restored", layout::slugify(&config.name)),
        "",
    );
    if let Err(error) = fs::rename(&stage, &final_path) {
        let _ = fs::remove_dir_all(&stage);
        return Err(AppError::Io(error.to_string()));
    }
    Ok(RestoreReport {
        workspace_path: final_path,
        file_count: stats.files,
        total_bytes: stats.bytes,
    })
}

fn canonical_directory(path: &Path, label: &str) -> AppResult<PathBuf> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!("{label} folder does not exist: {}", path.display()))
        } else {
            AppError::Io(format!("{}: {error}", path.display()))
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::Validation(format!(
            "{label} path must be a real directory, not a symlink"
        )));
    }
    path.canonicalize()
        .map_err(|error| AppError::Io(format!("{}: {error}", path.display())))
}

fn require_regular_file(path: &Path, label: &str) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::Io(format!("{}: {error}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::Validation(format!(
            "{label} must be a regular file, not a symlink"
        )));
    }
    Ok(())
}

fn require_real_directory(path: &Path, label: &str) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::Io(format!("{}: {error}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::Validation(format!(
            "{label} must be a real directory, not a symlink"
        )));
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, stats: &mut CopyStats) -> AppResult<CopyStats> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let target_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(AppError::Validation(format!(
                "backup does not follow symbolic links: {}",
                source_path.display()
            )));
        }
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if layout::is_skipped_dir(&name) {
                continue;
            }
            copy_tree(&source_path, &target_path, stats)?;
        } else if file_type.is_file() {
            let size = entry.metadata()?.len();
            stats.bytes = stats
                .bytes
                .checked_add(size)
                .filter(|total| *total <= MAX_WORKSPACE_BYTES)
                .ok_or_else(|| {
                    AppError::Validation("workspace backup exceeds the 8 GiB limit".into())
                })?;
            stats.files += 1;
            fs::copy(&source_path, &target_path)?;
        } else {
            return Err(AppError::Validation(format!(
                "unsupported special file in workspace: {}",
                source_path.display()
            )));
        }
    }
    Ok(CopyStats {
        files: stats.files,
        bytes: stats.bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("thread-backup-{tag}-{}", new_id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_workspace(path: &Path) {
        fs::create_dir_all(path.join("api")).unwrap();
        fs::write(
            path.join(layout::WORKSPACE_FILE),
            r#"{"version":1,"name":"Demo"}"#,
        )
        .unwrap();
        fs::write(path.join("api/request.http.json"), r#"{"name":"Request"}"#).unwrap();
        fs::create_dir_all(path.join("target/generated")).unwrap();
        fs::write(path.join("target/generated/skip.txt"), "skip").unwrap();
        fs::create_dir_all(path.join(".git")).unwrap();
        fs::write(path.join(".git/config"), "git state").unwrap();
    }

    #[test]
    fn backup_restore_copies_workspace_without_git_or_build_artifacts() {
        let source_parent = temp_dir("source");
        let source = source_parent.join("demo");
        create_workspace(&source);
        let backup_parent = temp_dir("backups");
        let backup = backup_workspace(&source, &backup_parent).unwrap();
        assert!(backup.path.join(MANIFEST_FILE).is_file());
        assert!(
            backup
                .path
                .join(CONTENT_DIR)
                .join(layout::WORKSPACE_FILE)
                .is_file()
        );
        assert!(!backup.path.join(CONTENT_DIR).join(".git").exists());
        assert!(!backup.path.join(CONTENT_DIR).join("target").exists());

        let restore_parent = temp_dir("restore");
        let restored = restore_workspace(&backup.path, &restore_parent).unwrap();
        assert!(
            restored
                .workspace_path
                .join(layout::WORKSPACE_FILE)
                .is_file()
        );
        assert!(
            restored
                .workspace_path
                .join("api/request.http.json")
                .is_file()
        );
        assert!(!restored.workspace_path.join("target").exists());
        let _ = fs::remove_dir_all(source_parent);
        let _ = fs::remove_dir_all(backup_parent);
        let _ = fs::remove_dir_all(restore_parent);
    }

    #[test]
    fn backup_rejects_a_destination_inside_the_workspace() {
        let parent = temp_dir("nested-destination");
        let workspace = parent.join("workspace");
        create_workspace(&workspace);
        assert!(backup_workspace(&workspace, &workspace).is_err());
        let _ = fs::remove_dir_all(parent);
    }

    #[cfg(unix)]
    #[test]
    fn backup_rejects_symbolic_links_instead_of_following_them() {
        use std::os::unix::fs::symlink;

        let parent = temp_dir("symlink");
        let workspace = parent.join("workspace");
        create_workspace(&workspace);
        let outside = parent.join("outside.txt");
        fs::write(&outside, "outside workspace").unwrap();
        symlink(&outside, workspace.join("outside-link")).unwrap();
        let destination = temp_dir("symlink-backup");
        assert!(backup_workspace(&workspace, &destination).is_err());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        let _ = fs::remove_dir_all(parent);
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn restore_rejects_unmarked_directory() {
        let backup = temp_dir("unmarked");
        let destination = temp_dir("destination");
        assert!(restore_workspace(&backup, &destination).is_err());
        let _ = fs::remove_dir_all(backup);
        let _ = fs::remove_dir_all(destination);
    }
}
