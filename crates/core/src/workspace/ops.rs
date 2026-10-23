//! Workspace file operations shared by the IPC command layer.
//!
//! 这些函数只依赖工作区目录，不依赖 Tauri，因此可以单独跑单元测试
//! （这是「最小验证链」中 Rust 侧测试的主要落点）。

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::services::secrets;

use super::formats::{
    AuthSpec, CollectionMeta, EnvVariable, EnvironmentFile, HttpRequestDoc, RequestFile,
};
use super::index::WorkspaceIndex;
use super::{CollectionDetail, EnvVariableView, EnvironmentDetail, RequestDetail, files, layout};

fn environment_is_global(rel: &str) -> bool {
    Path::new(rel)
        .file_stem()
        .map(|stem| {
            stem.to_string_lossy()
                .trim_end_matches(".env")
                .eq_ignore_ascii_case("global")
        })
        .unwrap_or(false)
}

fn environment_stem(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .map(|stem| stem.to_string_lossy().trim_end_matches(".env").to_string())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

/// Creates a collection folder (with `collection.json`) under `parent_rel`.
pub fn create_collection(
    root: &Path,
    parent_rel: Option<&str>,
    name: &str,
    description: Option<String>,
) -> AppResult<(String, CollectionMeta)> {
    crate::db::require_non_empty("collection name", name)?;
    let parent = match parent_rel {
        Some(rel) if !rel.trim().is_empty() => layout::resolve_inside(root, rel)?,
        _ => root.to_path_buf(),
    };
    if !parent.is_dir() {
        return Err(AppError::NotFound(format!(
            "parent collection does not exist: {}",
            parent.display()
        )));
    }

    let slug = layout::slugify(name);
    let dir = layout::unique_path(&parent, &slug, "");
    files::ensure_dir(&dir)?;

    let mut meta = CollectionMeta::new(name);
    meta.id = Some(new_id());
    meta.description = description;
    files::write_json(&dir.join(layout::COLLECTION_FILE), &meta)?;

    let rel = layout::to_relative_string(root, &dir);
    Ok((rel, meta))
}

/// Updates `collection.json`, creating it when the folder has none yet.
pub fn update_collection(
    root: &Path,
    rel: &str,
    name: Option<String>,
    description: Option<String>,
) -> AppResult<CollectionMeta> {
    let dir = layout::resolve_inside(root, rel)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!("collection not found: {rel}")));
    }
    let meta_path = dir.join(layout::COLLECTION_FILE);
    let folder_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());

    let mut meta = if meta_path.exists() {
        files::read_json::<CollectionMeta>(&meta_path)?
    } else {
        CollectionMeta::new(&folder_name)
    };
    if meta.id.is_none() {
        meta.id = Some(new_id());
    }
    if let Some(name) = name {
        crate::db::require_non_empty("collection name", &name)?;
        meta.name = name;
    }
    if let Some(description) = description {
        meta.description = if description.trim().is_empty() {
            None
        } else {
            Some(description)
        };
    }
    files::write_json(&meta_path, &meta)?;
    Ok(meta)
}

/// 把请求文件移到另一个集合目录，返回新的工作区相对路径。
pub fn move_request_file(
    root: &Path,
    source_rel: &str,
    dest_collection: &str,
) -> AppResult<String> {
    let source = layout::resolve_inside(root, source_rel)?;
    if !source.is_file() {
        return Err(AppError::NotFound(format!(
            "request not found: {source_rel}"
        )));
    }
    let dest_dir = if dest_collection.is_empty() {
        root.to_path_buf()
    } else {
        layout::resolve_inside(root, dest_collection)?
    };
    if !dest_dir.is_dir() {
        return Err(AppError::NotFound(format!(
            "collection not found: {dest_collection}"
        )));
    }
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = Path::new(&file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.clone());
    let extension = Path::new(&file_name)
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    let dest = layout::unique_path(&dest_dir, &stem, &extension);
    fs::rename(&source, &dest).map_err(|error| AppError::Io(error.to_string()))?;
    Ok(layout::to_relative_string(root, &dest))
}

/// 写入集合里请求的显示顺序。没有 `collection.json` 时会建一份。
pub fn set_request_order(root: &Path, collection_rel: &str, order: &[String]) -> AppResult<()> {
    let dir = if collection_rel.is_empty() {
        root.to_path_buf()
    } else {
        layout::resolve_inside(root, collection_rel)?
    };
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!(
            "collection not found: {collection_rel}"
        )));
    }
    let meta_path = dir.join(layout::COLLECTION_FILE);
    let folder_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| collection_rel.to_string());
    let mut meta = if meta_path.exists() {
        files::read_json::<CollectionMeta>(&meta_path)?
    } else {
        CollectionMeta::new(&folder_name)
    };
    if meta.id.is_none() {
        meta.id = Some(new_id());
    }
    meta.request_order = order.to_vec();
    files::write_json(&meta_path, &meta)?;
    Ok(())
}

pub fn delete_collection(root: &Path, rel: &str, trash_root: &Path) -> AppResult<String> {
    let dir = layout::resolve_inside(root, rel)?;
    if dir == root {
        return Err(AppError::Validation(
            "the workspace root is not a collection".into(),
        ));
    }
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!("collection not found: {rel}")));
    }
    let trashed = files::move_to_trash(root, &dir, trash_root)?;
    Ok(trashed.to_string_lossy().into_owned())
}

pub fn collection_detail(
    root: &Path,
    rel: &str,
    index: Option<&WorkspaceIndex>,
) -> AppResult<CollectionDetail> {
    let dir = layout::resolve_inside(root, rel)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!("collection not found: {rel}")));
    }
    let meta_path = dir.join(layout::COLLECTION_FILE);
    let has_meta = meta_path.exists();
    let meta = if has_meta {
        Some(files::read_json::<CollectionMeta>(&meta_path)?)
    } else {
        None
    };
    let folder_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());
    let parent_path = Path::new(rel)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| !parent.is_empty());

    let (request_count, child_count) = index
        .and_then(|index| find_collection_node(index, rel))
        .map(|node| (node.requests.len(), node.children.len()))
        .unwrap_or((0, 0));

    Ok(CollectionDetail {
        id: meta
            .as_ref()
            .and_then(|meta| meta.id.clone())
            .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, rel)),
        name: meta
            .as_ref()
            .map(|meta| meta.name.clone())
            .unwrap_or(folder_name),
        path: rel.to_string(),
        description: meta.as_ref().and_then(|meta| meta.description.clone()),
        has_meta,
        auth: meta
            .as_ref()
            .map(|meta| meta.auth.clone())
            .unwrap_or_default(),
        headers: meta
            .as_ref()
            .map(|meta| meta.headers.clone())
            .unwrap_or_default(),
        pre_script: meta
            .as_ref()
            .map(|meta| meta.pre_script.clone())
            .unwrap_or_default(),
        post_script: meta
            .as_ref()
            .map(|meta| meta.post_script.clone())
            .unwrap_or_default(),
        parent_path,
        request_count,
        child_count,
    })
}

fn find_collection_node<'a>(
    index: &'a WorkspaceIndex,
    rel: &str,
) -> Option<&'a super::index::CollectionNode> {
    fn walk<'a>(
        nodes: &'a [super::index::CollectionNode],
        rel: &str,
    ) -> Option<&'a super::index::CollectionNode> {
        for node in nodes {
            if node.path == rel {
                return Some(node);
            }
            if let Some(found) = walk(&node.children, rel) {
                return Some(found);
            }
        }
        None
    }
    walk(&index.collections, rel)
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

pub fn create_request(
    root: &Path,
    collection_rel: &str,
    name: &str,
    protocol: &str,
) -> AppResult<(String, RequestFile)> {
    crate::db::require_non_empty("request name", name)?;
    let dir = layout::resolve_inside(root, collection_rel)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!(
            "collection not found: {collection_rel}"
        )));
    }
    let suffix = layout::file_suffix_for_protocol(protocol)?;
    let path = layout::unique_path(&dir, &layout::slugify(name), suffix);

    let id = new_id();
    let mut document = match protocol {
        "http" => RequestFile::Http(HttpRequestDoc::new(name)),
        "grpc" => RequestFile::Grpc(default_grpc(name)),
        "websocket" => RequestFile::Websocket(default_ws(name)),
        "graphql" => RequestFile::Graphql(default_graphql(name)),
        "mqtt" => RequestFile::Mqtt(default_mqtt(name)),
        _ => {
            return Err(AppError::Validation(format!(
                "unsupported protocol: {protocol}"
            )));
        }
    };
    document.set_id(&id);
    files::write_json(&path, &document)?;
    Ok((layout::to_relative_string(root, &path), document))
}

fn default_grpc(name: &str) -> super::formats::GrpcRequestDoc {
    super::formats::GrpcRequestDoc {
        version: layout::FILE_VERSION,
        id: None,
        name: name.to_string(),
        host: String::new(),
        service: String::new(),
        method: String::new(),
        proto_source: "file".into(),
        proto_file_path: None,
        metadata: Vec::new(),
        payload: String::new(),
        tls_enabled: false,
        timeout_ms: 30_000,
    }
}

fn default_ws(name: &str) -> super::formats::WsRequestDoc {
    super::formats::WsRequestDoc {
        version: layout::FILE_VERSION,
        id: None,
        name: name.to_string(),
        url: String::new(),
        headers: Vec::new(),
        protocols: Vec::new(),
        auto_reconnect: false,
        saved_messages: Vec::new(),
    }
}

fn default_graphql(name: &str) -> super::formats::GraphqlRequestDoc {
    super::formats::GraphqlRequestDoc {
        version: layout::FILE_VERSION,
        id: None,
        name: name.to_string(),
        endpoint: String::new(),
        query: String::new(),
        variables: String::new(),
        headers: Vec::new(),
        auth: AuthSpec::default(),
        operation_name: None,
    }
}

fn default_mqtt(name: &str) -> super::formats::MqttRequestDoc {
    super::formats::MqttRequestDoc {
        version: layout::FILE_VERSION,
        id: None,
        name: name.to_string(),
        broker_url: String::new(),
        client_id: String::new(),
        username: String::new(),
        password: String::new(),
        subscriptions: Vec::new(),
        tls_enabled: false,
        keep_alive_secs: 60,
    }
}

pub fn read_request(root: &Path, rel: &str) -> AppResult<RequestFile> {
    let path = layout::resolve_inside(root, rel)?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("request not found: {rel}")));
    }
    files::read_relative_json(root, rel)
}

/// Nearest ancestor folder of a request that has a `collection.json`.
pub fn nearest_collection(root: &Path, request_rel: &str) -> (String, Option<CollectionMeta>) {
    let mut current = Path::new(request_rel)
        .parent()
        .map(|parent| parent.to_path_buf());
    while let Some(dir) = current {
        let rel = dir.to_string_lossy().replace('\\', "/");
        let meta_path = root.join(&dir).join(layout::COLLECTION_FILE);
        if meta_path.exists()
            && let Ok(meta) = files::read_json::<CollectionMeta>(&meta_path)
        {
            return (rel, Some(meta));
        }
        if rel.is_empty() {
            break;
        }
        current = dir.parent().map(|parent| parent.to_path_buf());
    }
    (String::new(), None)
}

pub fn request_detail(
    root: &Path,
    index: Option<&WorkspaceIndex>,
    rel: &str,
) -> AppResult<RequestDetail> {
    let path = layout::resolve_inside(root, rel)?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("request not found: {rel}")));
    }
    let (collection_path, collection_meta) = nearest_collection(root, rel);
    let location = index.and_then(|index| index.find_request(rel));
    let collection_name = location
        .as_ref()
        .map(|location| location.collection_name.clone())
        .or_else(|| {
            Path::new(&collection_path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();

    let raw = files::read_json_value(&path).unwrap_or(Value::Null);
    let document = read_request(root, rel);

    let (protocol, name, id, http, error) = match &document {
        Ok(document) => {
            let http = match document {
                RequestFile::Http(doc) => Some(doc.clone()),
                _ => None,
            };
            (
                document.protocol().to_string(),
                document.name().to_string(),
                document
                    .id()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, rel)),
                http,
                None,
            )
        }
        Err(error) => (
            layout::protocol_from_file_name(
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .as_str(),
            )
            .unwrap_or("http")
            .to_string(),
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| rel.to_string()),
            format!("{}{}", layout::PATH_ID_PREFIX, rel),
            None,
            Some(error.to_string()),
        ),
    };

    Ok(RequestDetail {
        id,
        name,
        protocol,
        path: rel.to_string(),
        collection_path,
        collection_name,
        inherited_headers: collection_meta
            .as_ref()
            .map(|meta| meta.headers.clone())
            .unwrap_or_default(),
        inherited_auth: collection_meta
            .as_ref()
            .map(|meta| meta.auth.clone())
            .unwrap_or_default(),
        raw,
        http,
        error,
    })
}

pub fn rename_request(root: &Path, rel: &str, name: &str) -> AppResult<RequestFile> {
    crate::db::require_non_empty("request name", name)?;
    let path = layout::resolve_inside(root, rel)?;
    let mut document = files::read_json::<RequestFile>(&path)?;
    document.set_name(name);
    if document.id().is_none() {
        document.set_id(&new_id());
    }
    files::write_json(&path, &document)?;
    Ok(document)
}

/// Replaces the protocol document. The request id is preserved even if the
/// incoming payload omits it.
pub fn update_request_document(root: &Path, rel: &str, incoming: Value) -> AppResult<RequestFile> {
    let path = layout::resolve_inside(root, rel)?;
    let existing = files::read_json::<RequestFile>(&path).ok();
    let existing_id = existing
        .as_ref()
        .and_then(|document| document.id().map(str::to_string))
        .unwrap_or_else(new_id);

    let mut object = match incoming {
        Value::Object(object) => object,
        _ => {
            return Err(AppError::Validation(
                "request document must be a JSON object".into(),
            ));
        }
    };
    object.insert("version".into(), Value::from(layout::FILE_VERSION));
    object.insert("id".into(), Value::from(existing_id.clone()));
    if !object.contains_key("name") {
        return Err(AppError::Validation(
            "request document requires a name".into(),
        ));
    }

    let fallback_protocol = layout::protocol_from_file_name(
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .as_str(),
    )
    .unwrap_or("http");
    object
        .entry("protocol".to_string())
        .or_insert_with(|| Value::from(fallback_protocol));

    let document: RequestFile = serde_json::from_value(Value::Object(object))
        .map_err(|error| AppError::Validation(format!("invalid request document: {error}")))?;
    files::write_json(&path, &document)?;
    Ok(document)
}

pub fn delete_request(root: &Path, rel: &str, trash_root: &Path) -> AppResult<String> {
    let path = layout::resolve_inside(root, rel)?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("request not found: {rel}")));
    }
    let trashed = files::move_to_trash(root, &path, trash_root)?;
    Ok(trashed.to_string_lossy().into_owned())
}

pub fn duplicate_request(
    root: &Path,
    rel: &str,
    name: Option<String>,
) -> AppResult<(String, RequestFile)> {
    let path = layout::resolve_inside(root, rel)?;
    let mut document = files::read_json::<RequestFile>(&path)?;
    let target_name = name.unwrap_or_else(|| format!("{} copy", document.name()));
    crate::db::require_non_empty("request name", &target_name)?;

    let protocol = document.protocol();
    let suffix = layout::file_suffix_for_protocol(protocol)?;
    let dir = path
        .parent()
        .ok_or_else(|| AppError::Validation("request has no parent folder".into()))?;
    let target = layout::unique_path(dir, &layout::slugify(&target_name), suffix);

    document.set_name(&target_name);
    document.set_id(&new_id());
    files::write_json(&target, &document)?;
    Ok((layout::to_relative_string(root, &target), document))
}

// ---------------------------------------------------------------------------
// Environments
// ---------------------------------------------------------------------------

pub fn create_environment(root: &Path, name: &str) -> AppResult<(String, EnvironmentFile)> {
    crate::db::require_non_empty("environment name", name)?;
    let dir = root.join(layout::ENVIRONMENTS_DIR);
    files::ensure_dir(&dir)?;
    let path = layout::unique_path(&dir, &layout::slugify(name), ".env.json");
    let mut environment = EnvironmentFile::new(name);
    environment.id = Some(new_id());
    files::write_json(&path, &environment)?;
    Ok((layout::to_relative_string(root, &path), environment))
}

pub fn read_environment(root: &Path, rel: &str) -> AppResult<EnvironmentFile> {
    files::read_relative_json(root, rel).map_err(|error| match error {
        AppError::NotFound(_) => AppError::NotFound(format!("environment not found: {rel}")),
        other => other,
    })
}

pub fn update_environment(
    root: &Path,
    rel: &str,
    name: Option<String>,
) -> AppResult<EnvironmentFile> {
    let path = layout::resolve_inside(root, rel)?;
    let mut environment = files::read_json::<EnvironmentFile>(&path)?;
    if let Some(name) = name {
        crate::db::require_non_empty("environment name", &name)?;
        environment.name = name;
    }
    if environment.id.is_none() {
        environment.id = Some(new_id());
    }
    files::write_json(&path, &environment)?;
    Ok(environment)
}

pub fn delete_environment(root: &Path, rel: &str, trash_root: &Path) -> AppResult<String> {
    let path = layout::resolve_inside(root, rel)?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("environment not found: {rel}")));
    }
    secrets::remove_environment(root, rel)?;
    let trashed = files::move_to_trash(root, &path, trash_root)?;
    Ok(trashed.to_string_lossy().into_owned())
}

/// Creates or updates a variable. Secret values are diverted to the app data
/// directory and never written into the workspace file.
pub fn set_env_variable(
    root: &Path,
    rel: &str,
    key: &str,
    value: Option<String>,
    enabled: Option<bool>,
    secret: Option<bool>,
    description: Option<String>,
) -> AppResult<EnvironmentFile> {
    crate::db::require_non_empty("variable key", key)?;
    let path = layout::resolve_inside(root, rel)?;
    let mut environment = files::read_json::<EnvironmentFile>(&path)?;
    if environment.id.is_none() {
        environment.id = Some(new_id());
    }

    let secret_flag = secret.unwrap_or(false);
    let position = environment
        .variables
        .iter()
        .position(|variable| variable.key == key);

    let mut target = match position {
        Some(index) => environment.variables[index].clone(),
        None => EnvVariable {
            key: key.to_string(),
            value: String::new(),
            enabled: true,
            secret: secret_flag,
            description: None,
        },
    };

    target.key = key.to_string();
    if let Some(enabled) = enabled {
        target.enabled = enabled;
    }
    if let Some(description) = description {
        target.description = if description.trim().is_empty() {
            None
        } else {
            Some(description)
        };
    }

    if let Some(flag) = secret {
        target.secret = flag
    }

    if target.secret {
        if let Some(value) = value {
            secrets::set(root, rel, key, &value)?;
        }
        target.value = String::new();
    } else {
        if let Some(value) = value {
            target.value = value;
        }
        // A variable that stopped being secret must not keep a hidden local value.
        secrets::remove(root, rel, key)?;
    }

    match position {
        Some(index) => environment.variables[index] = target,
        None => environment.variables.push(target),
    }

    files::write_json(&path, &environment)?;
    Ok(environment)
}

/// Renames a variable in place. Secret values move with the name and stay out
/// of the workspace file.
pub fn rename_env_variable(
    root: &Path,
    rel: &str,
    from: &str,
    to: &str,
) -> AppResult<EnvironmentFile> {
    let from = from.trim();
    let to = to.trim();
    crate::db::require_non_empty("variable key", to)?;
    let path = layout::resolve_inside(root, rel)?;
    let mut environment = files::read_json::<EnvironmentFile>(&path)?;
    if from == to {
        return Ok(environment);
    }
    if environment
        .variables
        .iter()
        .any(|variable| variable.key == to)
    {
        return Err(AppError::Validation(format!(
            "variable '{to}' already exists"
        )));
    }
    let Some(variable) = environment
        .variables
        .iter_mut()
        .find(|variable| variable.key == from)
    else {
        return Err(AppError::NotFound(format!("variable '{from}'")));
    };
    let secret = variable.secret;
    variable.key = to.to_string();
    if secret {
        if let Some(value) = secrets::get(root, rel, from) {
            secrets::set(root, rel, to, &value)?;
        }
        secrets::remove(root, rel, from)?;
    }
    files::write_json(&path, &environment)?;
    Ok(environment)
}

pub fn delete_env_variable(root: &Path, rel: &str, key: &str) -> AppResult<EnvironmentFile> {
    let path = layout::resolve_inside(root, rel)?;
    let mut environment = files::read_json::<EnvironmentFile>(&path)?;
    environment.variables.retain(|variable| variable.key != key);
    secrets::remove(root, rel, key)?;
    files::write_json(&path, &environment)?;
    Ok(environment)
}

pub fn environment_detail(
    root: &Path,
    rel: &str,
    active: Option<&str>,
) -> AppResult<EnvironmentDetail> {
    let environment = read_environment(root, rel)?;
    let variables = variable_views(root, rel, &environment);
    let is_active = active
        .map(|active| {
            environment.id.as_deref() == Some(active)
                || environment.name.eq_ignore_ascii_case(active)
                || environment_stem(rel).eq_ignore_ascii_case(active)
        })
        .unwrap_or(false);

    Ok(EnvironmentDetail {
        id: environment
            .id
            .clone()
            .unwrap_or_else(|| format!("{}{}", layout::PATH_ID_PREFIX, rel)),
        name: environment.name.clone(),
        path: rel.to_string(),
        is_global: environment_is_global(rel),
        is_active,
        variables,
    })
}

fn variable_views(root: &Path, rel: &str, environment: &EnvironmentFile) -> Vec<EnvVariableView> {
    let stored = secrets::for_environment(root, rel);
    environment
        .variables
        .iter()
        .map(|variable| {
            let has_stored = stored
                .get(&variable.key)
                .map(|value| !value.is_empty())
                .unwrap_or(false)
                || (!variable.secret && !variable.value.is_empty());
            EnvVariableView {
                key: variable.key.clone(),
                value: if variable.secret {
                    String::new()
                } else {
                    variable.value.clone()
                },
                enabled: variable.enabled,
                secret: variable.secret,
                description: variable.description.clone(),
                has_stored_value: has_stored,
            }
        })
        .collect()
}

/// Resolves an environment identifier to a workspace-relative file path.
///
/// Accepts a relative path (`environments/global.env.json`), a file stem
/// (`global`), the display name, or the environment id. Workspace init stores
/// the stem `"global"` in `thread.workspace.json`, while the file lives at
/// `environments/global.env.json`.
pub fn resolve_environment_rel(root: &Path, identifier: &str) -> Option<String> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return None;
    }

    if let Ok(path) = layout::resolve_inside(root, identifier)
        && path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(layout::is_environment_file)
    {
        return Some(layout::to_relative_string(root, &path));
    }

    let dir = root.join(layout::ENVIRONMENTS_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return None;
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();

    for path in paths {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !layout::is_environment_file(file_name) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|name| name.to_string_lossy().trim_end_matches(".env").to_string())
            .unwrap_or_default();
        let Ok(environment) = files::read_json::<EnvironmentFile>(&path) else {
            continue;
        };
        if crate::workspace::index::matches_active(Some(identifier), &environment, &stem) {
            return Some(layout::to_relative_string(root, &path));
        }
    }
    None
}

/// Effective variables for `http_send`: global environment first, then the active one.
///
/// `active_rel` must be a workspace-relative file path. Call
/// [`resolve_environment_rel`] first when the caller has a stem, name, or id.
/// 机密变量的值从应用数据目录读取（工作区文件里只有占位）。
pub fn resolve_variables(
    root: &Path,
    active_rel: Option<&str>,
) -> AppResult<std::collections::BTreeMap<String, String>> {
    const GLOBAL_REL: &str = "environments/global.env.json";

    let global_path = root.join(layout::ENVIRONMENTS_DIR).join("global.env.json");
    let global = if global_path.exists() {
        Some(files::read_json::<EnvironmentFile>(&global_path)?)
    } else {
        None
    };
    let active = match active_rel {
        Some(rel) if !rel.trim().is_empty() => Some(read_environment(root, rel)?),
        _ => None,
    };

    let global_secrets = secrets::for_environment(root, GLOBAL_REL);
    let active_secrets = match active_rel {
        Some(rel) if !rel.trim().is_empty() => secrets::for_environment(root, rel),
        _ => std::collections::HashMap::new(),
    };

    let mut environments: Vec<(&EnvironmentFile, &std::collections::HashMap<String, String>)> =
        Vec::new();
    if let Some(environment) = global.as_ref() {
        environments.push((environment, &global_secrets));
    }
    if let Some(environment) = active.as_ref() {
        environments.push((environment, &active_secrets));
    }

    Ok(crate::services::variables::collect_variables(&environments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::formats::KeyValue;
    use crate::workspace::init_workspace;
    use std::fs;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("thread-ops-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn setup(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        crate::db::test_data_dir();
        let root = temp_dir(tag);
        let trash = temp_dir(&format!("{tag}-trash"));
        init_workspace(&root, "Ops Test").unwrap();
        fs::create_dir_all(&trash).unwrap();
        (root, trash)
    }

    #[test]
    fn collection_lifecycle_creates_reads_and_trashes() {
        let (root, trash) = setup("collection");
        let (rel, meta) = create_collection(&root, None, "My API", None).unwrap();
        assert_eq!(rel, "My-API");
        assert!(meta.id.is_some());
        assert!(root.join("My-API/collection.json").exists());

        let read_back: CollectionMeta =
            files::read_relative_json(&root, &format!("{rel}/collection.json")).unwrap();
        assert_eq!(read_back.name, "My API");

        // Nested folders are allowed.
        let (child_rel, _) = create_collection(&root, Some(&rel), "Users", None).unwrap();
        assert_eq!(child_rel, "My-API/Users");

        let detail = collection_detail(&root, &rel, None).unwrap();
        assert!(detail.has_meta);
        assert_eq!(detail.name, "My API");

        let trashed = delete_collection(&root, &rel, &trash).unwrap();
        assert!(!root.join("My-API").exists());
        assert!(std::path::Path::new(&trashed).exists());

        let error = delete_collection(&root, "", &trash).unwrap_err();
        assert_eq!(error.code(), "VALIDATION");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn name_conflicts_get_a_numeric_suffix() {
        let (root, trash) = setup("conflict");
        let (first, _) = create_collection(&root, None, "API", None).unwrap();
        let (second, _) = create_collection(&root, None, "API", None).unwrap();
        assert_eq!(first, "API");
        assert_eq!(second, "API-2");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn request_lifecycle_round_trips_http_document() {
        let (root, trash) = setup("request");
        let (collection, _) = create_collection(&root, None, "API", None).unwrap();
        let (rel, _) = create_request(&root, &collection, "Create User", "http").unwrap();
        assert_eq!(rel, "API/Create-User.http.json");

        let mut document = read_request(&root, &rel).unwrap();
        let RequestFile::Http(doc) = &mut document else {
            panic!("expected HTTP document")
        };
        doc.method = "POST".into();
        doc.url = "{{baseUrl}}/users".into();
        fs::write(root.join(&rel), files::to_json_string(&document).unwrap()).unwrap();

        let detail = request_detail(&root, None, &rel).unwrap();
        let http = detail.http.clone().unwrap();
        assert_eq!(http.method, "POST");
        assert_eq!(http.url, "{{baseUrl}}/users");

        // Updating through a partial document keeps the id and the file format version.
        let updated = update_request_document(
            &root,
            &rel,
            serde_json::json!({
                "protocol": "http",
                "name": "Create User",
                "method": "PUT",
                "url": "{{baseUrl}}/users/1"
            }),
        )
        .unwrap();
        assert_eq!(updated.protocol(), "http");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(root.join(&rel)).unwrap()).unwrap();
        assert_eq!(value["id"], serde_json::json!(detail.id));
        assert_eq!(value["method"], "PUT");
        assert_eq!(value["version"], 1);

        let (copy_rel, _) = duplicate_request(&root, &rel, None).unwrap();
        assert_eq!(copy_rel, "API/Create-User-copy.http.json");

        let renamed = rename_request(&root, &rel, "Renamed").unwrap();
        assert_eq!(renamed.name(), "Renamed");

        let trashed = delete_request(&root, &rel, &trash).unwrap();
        assert!(!root.join(&rel).exists());
        assert!(std::path::Path::new(&trashed).exists());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn request_document_validation_rejects_bad_payloads() {
        let (root, trash) = setup("invalid");
        let (collection, _) = create_collection(&root, None, "API", None).unwrap();
        let (rel, _) = create_request(&root, &collection, "Ping", "http").unwrap();

        let error = update_request_document(&root, &rel, serde_json::json!("nope")).unwrap_err();
        assert_eq!(error.code(), "VALIDATION");

        let error = update_request_document(
            &root,
            &rel,
            serde_json::json!({"protocol": "http", "method": "GET"}),
        )
        .unwrap_err();
        assert_eq!(error.code(), "VALIDATION");

        // The file is untouched after a rejected update.
        assert!(read_request(&root, &rel).is_ok());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn inheritance_uses_nearest_collection_meta() {
        let (root, trash) = setup("inherit");
        let (collection, _) = create_collection(&root, None, "API", None).unwrap();
        update_collection(&root, &collection, None, None).unwrap();
        let meta_path = root.join(&collection).join(layout::COLLECTION_FILE);
        let mut meta: CollectionMeta = files::read_json(&meta_path).unwrap();
        meta.headers = vec![KeyValue::new("X-Api", "1")];
        meta.auth = AuthSpec {
            kind: "bearer".into(),
            token: Some("t".into()),
            ..AuthSpec::default()
        };
        files::write_json(&meta_path, &meta).unwrap();

        let (nested, _) = create_collection(&root, Some(&collection), "Nested", None).unwrap();
        // A hand-made nested folder without collection.json inherits the parent's meta.
        fs::remove_file(root.join(&nested).join(layout::COLLECTION_FILE)).unwrap();
        let (rel, _) = create_request(&root, &nested, "Deep", "http").unwrap();

        let detail = request_detail(&root, None, &rel).unwrap();
        assert_eq!(detail.collection_path, collection);
        assert_eq!(detail.inherited_headers.len(), 1);
        assert_eq!(detail.inherited_auth.kind, "bearer");

        // Giving the nested folder its own meta makes it the nearest ancestor.
        let mut nested_meta = CollectionMeta::new("Nested");
        nested_meta.headers = vec![KeyValue::new("X-Nested", "2")];
        files::write_json(
            &root.join(&nested).join(layout::COLLECTION_FILE),
            &nested_meta,
        )
        .unwrap();
        let detail = request_detail(&root, None, &rel).unwrap();
        assert_eq!(detail.collection_path, nested);
        assert_eq!(detail.inherited_headers[0].key, "X-Nested");
        assert!(detail.inherited_auth.is_inherit());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn secret_variables_never_touch_the_workspace_file() {
        let (root, trash) = setup("secrets");
        let (rel, _) = create_environment(&root, "dev").unwrap();
        set_env_variable(
            &root,
            &rel,
            "apiKey",
            Some("sk-live-secret".into()),
            Some(true),
            Some(true),
            None,
        )
        .unwrap();
        set_env_variable(
            &root,
            &rel,
            "baseUrl",
            Some("http://localhost:3000".into()),
            Some(true),
            Some(false),
            None,
        )
        .unwrap();

        let text = fs::read_to_string(root.join(&rel)).unwrap();
        assert!(!text.contains("sk-live-secret"));
        assert!(text.contains("\"secret\": true"));

        let detail = environment_detail(&root, &rel, None).unwrap();
        let api_key = detail
            .variables
            .iter()
            .find(|variable| variable.key == "apiKey")
            .unwrap();
        assert!(api_key.secret);
        assert_eq!(api_key.value, "");
        assert!(api_key.has_stored_value);
        assert_eq!(
            secrets::get(&root, &rel, "apiKey").unwrap(),
            "sk-live-secret"
        );

        // Resolution picks up the local secret value.
        let values = resolve_variables(&root, Some(&rel)).unwrap();
        assert_eq!(values.get("apiKey").unwrap(), "sk-live-secret");
        assert_eq!(values.get("baseUrl").unwrap(), "http://localhost:3000");

        // Toggling back to a plain variable removes the local secret.
        set_env_variable(
            &root,
            &rel,
            "apiKey",
            Some("plain".into()),
            None,
            Some(false),
            None,
        )
        .unwrap();
        assert!(secrets::get(&root, &rel, "apiKey").is_none());
        let text = fs::read_to_string(root.join(&rel)).unwrap();
        assert!(text.contains("plain"));

        rename_env_variable(&root, &rel, "baseUrl", "host").unwrap();
        let renamed = environment_detail(&root, &rel, None).unwrap();
        assert_eq!(renamed.variables[1].key, "host");
        assert_eq!(renamed.variables[1].value, "http://localhost:3000");
        let duplicate = rename_env_variable(&root, &rel, "host", "apiKey").unwrap_err();
        assert_eq!(duplicate.code(), "VALIDATION");

        delete_env_variable(&root, &rel, "apiKey").unwrap();
        let deleted = environment_detail(&root, &rel, None).unwrap();
        assert!(
            deleted
                .variables
                .iter()
                .all(|variable| variable.key != "apiKey")
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn renaming_a_secret_variable_moves_the_local_value() {
        let (root, trash) = setup("rename-secret");
        let (rel, _) = create_environment(&root, "dev").unwrap();
        set_env_variable(
            &root,
            &rel,
            "apiKey",
            Some("sk-live-secret".into()),
            Some(true),
            Some(true),
            None,
        )
        .unwrap();

        rename_env_variable(&root, &rel, "apiKey", "token").unwrap();

        assert!(secrets::get(&root, &rel, "apiKey").is_none());
        assert_eq!(
            secrets::get(&root, &rel, "token").unwrap(),
            "sk-live-secret"
        );
        let text = fs::read_to_string(root.join(&rel)).unwrap();
        assert!(!text.contains("sk-live-secret"));
        assert!(text.contains("token"));
        let detail = environment_detail(&root, &rel, None).unwrap();
        assert_eq!(detail.variables[0].key, "token");
        assert!(detail.variables[0].secret);
        assert_eq!(detail.variables[0].value, "");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn resolve_environment_rel_maps_init_stem_to_the_file() {
        let (root, trash) = setup("env-stem");
        let rel = resolve_environment_rel(&root, "global").expect("stem resolves");
        assert_eq!(rel, "environments/global.env.json");
        assert_eq!(
            resolve_environment_rel(&root, "Global").as_deref(),
            Some(rel.as_str())
        );
        assert_eq!(
            resolve_environment_rel(&root, &rel).as_deref(),
            Some(rel.as_str())
        );
        assert!(resolve_environment_rel(&root, "missing").is_none());
        resolve_variables(&root, Some(&rel)).unwrap();
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }

    #[test]
    fn path_traversal_is_rejected() {
        let (root, trash) = setup("traversal");
        let error = read_request(&root, "../../etc/passwd").unwrap_err();
        assert_eq!(error.code(), "VALIDATION");
        let error = create_request(&root, "../outside", "x", "http").unwrap_err();
        assert_eq!(error.code(), "VALIDATION");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&trash);
    }
}
