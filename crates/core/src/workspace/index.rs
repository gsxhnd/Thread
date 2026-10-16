//! Workspace file scanner and in-memory index.
//!
//! 阶段 0 的存储验证结论：**不引入 SQLite 文件索引**，改为「打开/刷新时全量扫描
//! 工作区文件 → 内存索引」。原因见 docs/08.refactor-baseline.md（实测扫描 1200 个
//! 请求文件耗时与搜索耗时均在毫秒级，Git pull 后整树刷新成本可接受）。
//! 请求历史等不可重建数据仍写 SQLite（`db` 模块）。

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::now_ms;
use crate::error::AppResult;

use super::files;
use super::formats::{CollectionMeta, EnvironmentFile, RequestFile, WorkspaceConfig};
use super::layout;

/// A request file discovered in the workspace.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestNode {
    pub id: String,
    pub name: String,
    pub protocol: String,
    /// Workspace-relative path, always forward slashes.
    pub path: String,
    pub method: Option<String>,
    pub url: Option<String>,
    /// Set when the file exists but cannot be parsed (still listed so the user sees it).
    pub error: Option<String>,
}

/// A request plus the collection it lives in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLocation {
    pub request: RequestNode,
    pub collection_path: String,
    pub collection_name: String,
}

/// A collection folder (recursively).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionNode {
    pub id: String,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    /// True when `collection.json` exists for this folder.
    pub has_meta: bool,
    pub requests: Vec<RequestNode>,
    pub children: Vec<CollectionNode>,
}

impl CollectionNode {
    pub fn request_count(&self) -> usize {
        self.requests.len() + self.children.iter().map(Self::request_count).sum::<usize>()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub variable_count: usize,
    pub secret_count: usize,
    /// Variable names only — used by the frontend for `{{variable}}` autocomplete.
    /// 变量名（非机密）随索引发给前端，用于 `{{variable}}` 自动补全；机密值永不外发。
    pub variable_names: Vec<String>,
    /// Subset of `variable_names` whose value is stored locally (masked in the UI).
    pub secret_variable_names: Vec<String>,
    pub is_global: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub path: String,
    pub collection_path: String,
    pub collection_name: String,
    pub method: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIndex {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub root: String,
    pub active_environment: Option<String>,
    pub collections: Vec<CollectionNode>,
    pub environments: Vec<EnvironmentSummary>,
    pub collection_count: usize,
    pub request_count: usize,
    pub warnings: Vec<String>,
    pub scanned_at: i64,
    pub digest: String,
}

impl WorkspaceIndex {
    /// Case-insensitive substring search across request and collection names/URLs/paths.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<SearchHit> = Vec::new();
        for collection in &self.collections {
            collect_hits(collection, "", "", &needle, &mut hits);
            if hits.len() >= limit {
                break;
            }
        }
        hits.truncate(limit);
        hits
    }

    /// Locates a request by workspace-relative path, together with its collection.
    pub fn find_request(&self, path: &str) -> Option<RequestLocation> {
        let mut stack: Vec<(&CollectionNode, String)> = self
            .collections
            .iter()
            .map(|node| (node, node.name.clone()))
            .collect();
        while let Some((node, display_name)) = stack.pop() {
            if let Some(request) = node.requests.iter().find(|request| request.path == path) {
                return Some(RequestLocation {
                    request: request.clone(),
                    collection_path: node.path.clone(),
                    collection_name: display_name,
                });
            }
            for child in &node.children {
                stack.push((child, format!("{display_name} / {}", child.name)));
            }
        }
        None
    }

    fn digest(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.root.hash(&mut hasher);
        for collection in &self.collections {
            hash_collection(collection, &mut hasher);
        }
        for environment in &self.environments {
            environment.id.hash(&mut hasher);
            environment.name.hash(&mut hasher);
            environment.variable_count.hash(&mut hasher);
            // 变量名参与摘要：改名/新增变量也要触发 workspace://changed。
            for name in &environment.variable_names {
                name.hash(&mut hasher);
            }
        }
        format!("{:x}", hasher.finish())
    }
}

fn hash_collection(node: &CollectionNode, hasher: &mut DefaultHasher) {
    node.id.hash(hasher);
    node.name.hash(hasher);
    node.path.hash(hasher);
    for request in &node.requests {
        request.id.hash(hasher);
        request.name.hash(hasher);
        request.path.hash(hasher);
        request.protocol.hash(hasher);
        request.method.hash(hasher);
        request.url.hash(hasher);
        request.error.hash(hasher);
    }
    for child in &node.children {
        hash_collection(child, hasher);
    }
}

fn collect_hits(
    node: &CollectionNode,
    _parent_path: &str,
    parent_name: &str,
    needle: &str,
    hits: &mut Vec<SearchHit>,
) {
    // `node.path` is already the full workspace-relative folder path.
    let collection_path = node.path.clone();
    let collection_name = if parent_name.is_empty() {
        node.name.clone()
    } else {
        format!("{parent_name} / {}", node.name)
    };

    for request in &node.requests {
        let haystacks = [
            request.name.to_lowercase(),
            request.path.to_lowercase(),
            request.url.clone().unwrap_or_default().to_lowercase(),
            request.method.clone().unwrap_or_default().to_lowercase(),
            node.name.to_lowercase(),
        ];
        if haystacks.iter().any(|value| value.contains(needle)) {
            hits.push(SearchHit {
                id: request.id.clone(),
                name: request.name.clone(),
                protocol: request.protocol.clone(),
                path: request.path.clone(),
                collection_path: collection_path.clone(),
                collection_name: collection_name.clone(),
                method: request.method.clone(),
                url: request.url.clone(),
            });
        }
    }

    for child in &node.children {
        collect_hits(child, &collection_path, &collection_name, needle, hits);
    }
}

pub struct ScanOutcome {
    pub index: WorkspaceIndex,
    pub config: WorkspaceConfig,
}

/// Full workspace scan. Never fails hard: unreadable files become warnings.
pub fn scan(root: &Path, existing_config: Option<WorkspaceConfig>) -> AppResult<ScanOutcome> {
    let mut warnings: Vec<String> = Vec::new();
    let root = root.to_path_buf();

    let config = match existing_config {
        Some(config) => config,
        None => load_workspace_config(&root, &mut warnings),
    };

    let environments = scan_environments(&root, &config, &mut warnings);
    let active_environment = environments
        .iter()
        .find(|environment| environment.is_active)
        .map(|environment| environment.path.clone());

    let mut collections = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if layout::is_skipped_dir(&name) || name == layout::ENVIRONMENTS_DIR {
                continue;
            }
            let rel = layout::to_relative_string(&root, &dir);
            if let Some(node) = scan_collection(&dir, &rel, &mut warnings) {
                collections.push(node);
            }
        }
    } else {
        warnings.push(format!(
            "cannot read workspace directory: {}",
            root.display()
        ));
    }

    sort_collections(&mut collections);

    let request_count = collections
        .iter()
        .map(CollectionNode::request_count)
        .sum::<usize>();

    let id = config
        .id
        .clone()
        .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, root.to_string_lossy()));

    let mut index = WorkspaceIndex {
        id,
        name: config.name.clone(),
        version: config.version,
        root: root.to_string_lossy().into_owned(),
        active_environment,
        collections,
        environments,
        collection_count: 0,
        request_count,
        warnings,
        scanned_at: now_ms(),
        digest: String::new(),
    };
    index.collection_count = index.collections.len();
    index.digest = index.digest();
    Ok(ScanOutcome { index, config })
}

fn load_workspace_config(root: &Path, warnings: &mut Vec<String>) -> WorkspaceConfig {
    let path = root.join(layout::WORKSPACE_FILE);
    if !path.exists() {
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".to_string());
        warnings.push(format!(
            "{} not found; using the folder name as the workspace name",
            layout::WORKSPACE_FILE
        ));
        return WorkspaceConfig::new(&name);
    }
    match files::read_json::<WorkspaceConfig>(&path) {
        Ok(config) => config,
        Err(error) => {
            warnings.push(format!("{}: {error}", layout::WORKSPACE_FILE));
            let name = root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Workspace".to_string());
            WorkspaceConfig::new(&name)
        }
    }
}

fn scan_environments(
    root: &Path,
    config: &WorkspaceConfig,
    warnings: &mut Vec<String>,
) -> Vec<EnvironmentSummary> {
    let dir = root.join(layout::ENVIRONMENTS_DIR);
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return summaries;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .map(|name| layout::is_environment_file(&name.to_string_lossy()))
                    .unwrap_or(false)
        })
        .collect();
    paths.sort();

    for path in paths {
        let rel = layout::to_relative_string(root, &path);
        let stem = path
            .file_stem()
            .map(|name| name.to_string_lossy().trim_end_matches(".env").to_string())
            .unwrap_or_default();
        match files::read_json::<EnvironmentFile>(&path) {
            Ok(environment) => {
                let is_active =
                    matches_active(config.active_environment.as_deref(), &environment, &stem);
                let mut variable_names = names_of(&environment, None);
                variable_names.sort();
                variable_names.dedup();
                let mut secret_variable_names = names_of(&environment, Some(true));
                secret_variable_names.sort();
                secret_variable_names.dedup();

                summaries.push(EnvironmentSummary {
                    id: environment
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, rel)),
                    name: environment.name.clone(),
                    path: rel,
                    variable_count: environment.variables.iter().filter(|v| v.enabled).count(),
                    secret_count: environment
                        .variables
                        .iter()
                        .filter(|variable| variable.secret)
                        .count(),
                    variable_names,
                    secret_variable_names,
                    is_global: stem.eq_ignore_ascii_case("global"),
                    is_active,
                });
            }
            Err(error) => warnings.push(format!("{rel}: {error}")),
        }
    }
    summaries.sort_by(|a, b| {
        a.is_global
            .cmp(&b.is_global)
            .reverse()
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    summaries
}

/// Non-empty variable names, optionally filtered by the `secret` flag.
///
/// 包含被禁用的变量：自动补全列出文件里「已定义」的键，是否生效由启用状态决定。
fn names_of(environment: &EnvironmentFile, secret: Option<bool>) -> Vec<String> {
    environment
        .variables
        .iter()
        .filter(|variable| secret.map(|flag| variable.secret == flag).unwrap_or(true))
        .map(|variable| variable.key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect()
}

pub(crate) fn matches_active(
    active: Option<&str>,
    environment: &EnvironmentFile,
    stem: &str,
) -> bool {
    let Some(active) = active else {
        return false;
    };
    let active = active.trim();
    if active.is_empty() {
        return false;
    }
    environment
        .id
        .as_deref()
        .map(|id| id == active)
        .unwrap_or(false)
        || environment.name.eq_ignore_ascii_case(active)
        || stem.eq_ignore_ascii_case(active)
}

fn scan_collection(dir: &Path, rel: &str, warnings: &mut Vec<String>) -> Option<CollectionNode> {
    let meta_path = dir.join(layout::COLLECTION_FILE);
    let mut has_meta = false;
    let mut meta: Option<CollectionMeta> = None;
    if meta_path.exists() {
        has_meta = true;
        match files::read_json::<CollectionMeta>(&meta_path) {
            Ok(parsed) => meta = Some(parsed),
            Err(error) => warnings.push(format!("{rel}/{}: {error}", layout::COLLECTION_FILE)),
        }
    }

    let fallback_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());

    let mut requests: Vec<RequestNode> = Vec::new();
    let mut children: Vec<CollectionNode> = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();

    for path in paths {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        if path.is_dir() {
            if layout::is_skipped_dir(&file_name) {
                continue;
            }
            let child_rel = format!("{rel}/{file_name}");
            if let Some(child) = scan_collection(&path, &child_rel, warnings) {
                children.push(child);
            }
            continue;
        }

        if file_name == layout::COLLECTION_FILE {
            continue;
        }

        let Some(protocol) = layout::protocol_from_file_name(&file_name) else {
            continue;
        };

        let request_rel = format!("{rel}/{file_name}");
        let fallback_id = format!("{}{}", layout::PATH_ID_PREFIX, request_rel);
        match files::read_json::<RequestFile>(&path) {
            Ok(document) => {
                let (method, url) = document.summary();
                requests.push(RequestNode {
                    id: document.id().map(str::to_string).unwrap_or(fallback_id),
                    name: document.name().to_string(),
                    protocol: document.protocol().to_string(),
                    path: request_rel,
                    method,
                    url,
                    error: None,
                });
            }
            Err(error) => {
                warnings.push(format!("{request_rel}: {error}"));
                requests.push(RequestNode {
                    id: fallback_id,
                    name: file_name.trim_end_matches(".json").to_string(),
                    protocol: protocol.to_string(),
                    path: request_rel,
                    method: None,
                    url: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    sort_requests(&mut requests);
    sort_collections(&mut children);

    let is_interesting = has_meta || !requests.is_empty() || !children.is_empty();
    if !is_interesting {
        return None;
    }

    let name = meta
        .as_ref()
        .map(|meta| meta.name.clone())
        .unwrap_or(fallback_name);
    let id = meta
        .as_ref()
        .and_then(|meta| meta.id.clone())
        .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, rel));

    Some(CollectionNode {
        id,
        name,
        path: rel.to_string(),
        description: meta.as_ref().and_then(|meta| meta.description.clone()),
        has_meta,
        requests,
        children,
    })
}

fn sort_collections(nodes: &mut [CollectionNode]) {
    nodes.sort_by_key(|a| a.name.to_lowercase());
}

fn sort_requests(nodes: &mut [RequestNode]) {
    nodes.sort_by_key(|a| a.name.to_lowercase());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::files::write_json;
    use crate::workspace::formats::{EnvironmentFile, HttpRequestDoc};
    use std::time::Instant;

    fn temp_workspace(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("thread-index-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_request(root: &Path, rel: &str, name: &str, url: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut doc = HttpRequestDoc::new(name);
        doc.url = url.to_string();
        write_json(&path, &RequestFile::Http(doc)).unwrap();
    }

    #[test]
    fn scans_collections_requests_and_environments() {
        let root = temp_workspace("scan");
        write_json(
            &root.join(layout::WORKSPACE_FILE),
            &WorkspaceConfig {
                active_environment: Some("dev".into()),
                ..WorkspaceConfig::new("My Workspace")
            },
        )
        .unwrap();
        write_json(
            &root.join("environments/dev.env.json"),
            &EnvironmentFile::new("dev"),
        )
        .unwrap();
        write_json(
            &root.join("my-api/collection.json"),
            &CollectionMeta::new("My API"),
        )
        .unwrap();
        write_request(
            &root,
            "my-api/users/list.http.json",
            "List Users",
            "{{baseUrl}}/users",
        );
        write_request(
            &root,
            "my-api/users/create.http.json",
            "Create User",
            "{{baseUrl}}/users",
        );
        write_request(
            &root,
            "my-api/auth/login.http.json",
            "Login",
            "{{baseUrl}}/login",
        );

        let outcome = scan(&root, None).unwrap();
        assert_eq!(outcome.index.name, "My Workspace");
        assert_eq!(outcome.index.collection_count, 1);
        assert_eq!(outcome.index.request_count, 3);
        assert_eq!(outcome.index.environments.len(), 1);
        assert!(outcome.index.environments[0].is_active);
        assert_eq!(
            outcome.index.active_environment.as_deref(),
            Some("environments/dev.env.json")
        );

        let api = &outcome.index.collections[0];
        assert_eq!(api.name, "My API");
        assert_eq!(api.children.len(), 2);
        assert_eq!(api.children[0].name, "auth");
        assert_eq!(api.request_count(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unparsable_request_is_listed_with_an_error_warning() {
        let root = temp_workspace("broken");
        write_json(
            &root.join("api/collection.json"),
            &CollectionMeta::new("API"),
        )
        .unwrap();
        fs::create_dir_all(root.join("api")).unwrap();
        fs::write(root.join("api/broken.http.json"), "{ not json").unwrap();

        let outcome = scan(&root, None).unwrap();
        assert_eq!(outcome.index.request_count, 1);
        assert!(outcome.index.collections[0].requests[0].error.is_some());
        assert!(!outcome.index.warnings.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skips_vcs_dirs_and_ignores_random_folders() {
        let root = temp_workspace("skip");
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        write_request(
            &root,
            "node_modules/pkg/hidden.http.json",
            "Hidden",
            "http://x",
        );
        write_request(&root, ".git/hidden.http.json", "Hidden", "http://x");
        fs::create_dir_all(root.join("empty-folder")).unwrap();
        assert!(scan(&root, None).unwrap().index.collections.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn external_edits_are_visible_after_rescan() {
        let root = temp_workspace("external");
        write_request(&root, "api/a.http.json", "Alpha", "http://example.com/a");
        let first = scan(&root, None).unwrap();
        assert_eq!(first.index.request_count, 1);

        // Simulate an external edit + a git checkout adding a file.
        let path = root.join("api/a.http.json");
        let mut doc = HttpRequestDoc::new("Alpha Renamed");
        doc.url = "http://example.com/b".to_string();
        write_json(&path, &RequestFile::Http(doc)).unwrap();
        write_request(&root, "api/b.http.json", "Beta", "http://example.com/beta");

        let second = scan(&root, None).unwrap();
        assert_eq!(second.index.request_count, 2);
        assert_ne!(first.index.digest, second.index.digest);
        let names: Vec<_> = second.index.collections[0]
            .requests
            .iter()
            .map(|request| request.name.clone())
            .collect();
        assert!(names.contains(&"Alpha Renamed".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn environment_variable_names_are_exposed_for_autocomplete() {
        let root = temp_workspace("var-names");
        write_json(
            &root.join(layout::WORKSPACE_FILE),
            &WorkspaceConfig {
                active_environment: Some("dev".into()),
                ..WorkspaceConfig::new("Vars")
            },
        )
        .unwrap();
        fs::create_dir_all(root.join("environments")).unwrap();
        fs::write(
            root.join("environments/dev.env.json"),
            r#"{
                "version": 1,
                "name": "dev",
                "variables": [
                    { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true, "secret": false },
                    { "key": "apiKey", "value": "", "enabled": true, "secret": true },
                    { "key": "legacyToken", "value": "x", "enabled": false, "secret": true },
                    { "key": "  ", "value": "ignored", "enabled": true, "secret": false }
                ]
            }"#,
        )
        .unwrap();

        let outcome = scan(&root, None).unwrap();
        let dev = &outcome.index.environments[0];
        assert_eq!(dev.name, "dev");
        assert_eq!(dev.variable_names, vec!["apiKey", "baseUrl", "legacyToken"]);
        assert_eq!(dev.secret_variable_names, vec!["apiKey", "legacyToken"]);
        assert!(dev.is_active);
        // Values are never part of the index payload.
        let serialized = serde_json::to_string(&outcome.index).unwrap();
        assert!(!serialized.contains("localhost:3000"));
        assert!(!serialized.contains("\"x\""));

        // Renaming a variable changes the digest, so the UI refreshes its completions.
        fs::write(
            root.join("environments/dev.env.json"),
            r#"{"version":1,"name":"dev","variables":[{"key":"host","value":"http://localhost:3000","enabled":true,"secret":false}]}"#,
        )
        .unwrap();
        let renamed = scan(&root, None).unwrap();
        assert_ne!(outcome.index.digest, renamed.index.digest);
        assert_eq!(renamed.index.environments[0].variable_names, vec!["host"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn search_matches_name_url_and_path() {
        let root = temp_workspace("search");
        write_request(
            &root,
            "api/users.http.json",
            "List Users",
            "{{baseUrl}}/api/users",
        );
        write_request(
            &root,
            "api/orders.http.json",
            "List Orders",
            "{{baseUrl}}/api/orders",
        );
        let index = scan(&root, None).unwrap().index;

        assert_eq!(index.search("users", 10).len(), 1);
        assert_eq!(index.search("orders", 10)[0].name, "List Orders");
        assert_eq!(index.search("BASEURL", 10).len(), 2);
        assert_eq!(index.search("api/", 10).len(), 2);
        assert!(index.search("  ", 10).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// 阶段 0 性能验证：全量扫描 + 搜索的实测成本（结果记录在 docs/08）。
    #[test]
    fn scan_and_search_performance_is_acceptable() {
        let root = temp_workspace("perf");
        let collections = 40;
        let per_collection = 30; // 1200 request files total
        for collection in 0..collections {
            let dir = format!("service-{collection:02}");
            write_json(
                &root.join(format!("{dir}/collection.json")),
                &CollectionMeta::new(&format!("Service {collection:02}")),
            )
            .unwrap();
            for request in 0..per_collection {
                write_request(
                    &root,
                    &format!("{dir}/nested/request-{request:03}.http.json"),
                    &format!("Request {collection}-{request}"),
                    &format!("https://api.example.com/v1/service-{collection}/items/{request}"),
                );
            }
        }
        for environment in ["dev", "staging", "prod"] {
            write_json(
                &root.join(format!("environments/{environment}.env.json")),
                &EnvironmentFile::new(environment),
            )
            .unwrap();
        }

        let started = Instant::now();
        let outcome = scan(&root, None).unwrap();
        let scan_elapsed = started.elapsed();

        let search_started = Instant::now();
        for _ in 0..1_000 {
            let hits = outcome.index.search("service-7", 20);
            assert!(!hits.is_empty());
        }
        let search_elapsed = search_started.elapsed();

        assert_eq!(outcome.index.request_count, collections * per_collection);
        println!(
            "scan: {} files in {:?} ({:.2} ms/file) | 1000 searches in {:?}",
            outcome.index.request_count,
            scan_elapsed,
            scan_elapsed.as_secs_f64() * 1000.0 / (collections * per_collection) as f64,
            search_elapsed
        );
        assert!(
            scan_elapsed.as_secs() < 10,
            "scan too slow: {scan_elapsed:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
