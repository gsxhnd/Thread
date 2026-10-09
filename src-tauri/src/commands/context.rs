//! Shared workspace lookup for protocol commands: variables, collection auth, timeouts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::config::ThreadConfigStore;
use crate::error::{AppError, AppResult};
use crate::workspace::formats::AuthSpec;
use crate::workspace::{SharedWorkspace, files, layout, ops};

pub struct CallContext {
    pub root: PathBuf,
    pub variables: BTreeMap<String, String>,
    pub collection_auth: Option<AuthSpec>,
    pub request_name: Option<String>,
    pub request_document: Option<Value>,
    pub timeout_ms: u64,
    pub verify_ssl: bool,
    pub history_limit: i64,
}

pub fn prepare(
    workspace: &SharedWorkspace,
    config_store: &ThreadConfigStore,
    request_path: Option<&str>,
    environment_path: Option<&str>,
) -> AppResult<CallContext> {
    let root = workspace.root()?;
    let settings = config_store.app_settings();
    let (collection_auth, request_name, request_document) = match request_path {
        Some(path) => match layout::resolve_inside(&root, path) {
            Ok(absolute) => {
                let (_, meta) = ops::nearest_collection(&root, path);
                let document = files::read_json_value(&absolute).ok();
                let name = document
                    .as_ref()
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                (meta.as_ref().map(|meta| meta.auth.clone()), name, document)
            }
            Err(_) => (None, None, None),
        },
        None => (None, None, None),
    };

    let config_environment = if environment_path.is_none_or(|value| value.trim().is_empty()) {
        workspace
            .config()
            .ok()
            .and_then(|config| config.active_environment)
    } else {
        None
    };
    let identifier = environment_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            config_environment
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    let explicit = environment_path.is_some_and(|value| !value.trim().is_empty());
    let active_rel = match identifier {
        Some(value) => match ops::resolve_environment_rel(&root, value) {
            Some(rel) => Some(rel),
            None if explicit => {
                return Err(AppError::NotFound(format!(
                    "environment not found: {value}"
                )));
            }
            None => None,
        },
        None => None,
    };
    let variables = ops::resolve_variables(&root, active_rel.as_deref())?;

    Ok(CallContext {
        root,
        variables,
        collection_auth: collection_auth.filter(|auth| !auth.is_none_type()),
        request_name,
        request_document,
        timeout_ms: settings.default_timeout_ms.max(1),
        verify_ssl: settings.verify_ssl,
        history_limit: settings.history_limit,
    })
}
