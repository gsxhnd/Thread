//! `{{variable}}` substitution with explicit scopes and missing-variable reporting.
//!
//! 作用域（后者覆盖前者）：全局环境 `environments/global.env.json` → 当前激活环境。
//! 机密变量的值来自应用数据目录（见 `secrets`），工作区文件里只有占位。
//! 替换是纯文本的：URL、Query、Headers、Body、Auth 字段与上传文件路径都会经过它。

use std::collections::BTreeMap;

use crate::db::now_ms;
use crate::workspace::formats::{EnvVariable, EnvironmentFile};

/// Builds the effective variable map from global + active environments.
///
/// Each environment is paired with its locally stored secret values (see `secrets`).
/// Disabled variables are skipped and later environments override earlier ones.
pub fn collect_variables(
    environments: &[(&EnvironmentFile, &std::collections::HashMap<String, String>)],
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for (environment, secrets) in environments {
        for variable in &environment.variables {
            let EnvVariable {
                key,
                value,
                enabled,
                secret,
                ..
            } = variable;
            if !enabled || key.trim().is_empty() {
                continue;
            }
            let resolved = if *secret {
                secrets
                    .get(key)
                    .cloned()
                    .filter(|stored| !stored.is_empty())
                    .unwrap_or_else(|| value.clone())
            } else {
                value.clone()
            };
            values.insert(key.clone(), resolved);
        }
    }
    values
}

/// Replaces every `{{name}}` occurrence. Unknown names are reported once and left
/// untouched so the user can see what is missing.
pub fn substitute(
    text: &str,
    variables: &BTreeMap<String, String>,
    missing: &mut Vec<String>,
) -> String {
    if !text.contains("{{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{'
            && index + 1 < bytes.len()
            && bytes[index + 1] == b'{'
            && let Some(close) = text[index + 2..].find("}}")
        {
            let raw_name = &text[index + 2..index + 2 + close];
            let name = raw_name.trim();
            if name.is_empty() {
                out.push_str("{{}}");
            } else if let Some(value) = variables.get(name) {
                out.push_str(value);
            } else if let Some(value) = resolve_builtin(name) {
                out.push_str(&value);
            } else {
                if !missing.iter().any(|item| item == name) {
                    missing.push(name.to_string());
                }
                out.push_str(&text[index..index + 2 + close + 2]);
            }
            index += 2 + close + 2;
            continue;
        }
        let ch = text[index..].chars().next().unwrap_or(' ');
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// Postman-style dynamic values: `{{$uuid}}`, `{{$timestamp}}`, `{{$randomInt}}`.
fn resolve_builtin(name: &str) -> Option<String> {
    match name {
        "$uuid" => Some(crate::db::new_id()),
        "$timestamp" => Some((now_ms() / 1000).to_string()),
        "$isoTimestamp" => Some(chrono::Utc::now().to_rfc3339()),
        "$randomInt" => Some((now_ms() % 1000).to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::formats::EnvVariable;

    fn environment(name: &str, variables: Vec<(&str, &str, bool, bool)>) -> EnvironmentFile {
        EnvironmentFile {
            version: 1,
            id: None,
            name: name.to_string(),
            variables: variables
                .into_iter()
                .map(|(key, value, enabled, secret)| EnvVariable {
                    key: key.to_string(),
                    value: value.to_string(),
                    enabled,
                    secret,
                    description: None,
                })
                .collect(),
        }
    }

    #[test]
    fn active_environment_overrides_global() {
        let global = environment("Global", vec![("baseUrl", "https://global", true, false)]);
        let dev = environment(
            "dev",
            vec![("baseUrl", "http://localhost:3000", true, false)],
        );
        let no_secrets = std::collections::HashMap::new();
        let values = collect_variables(&[(&global, &no_secrets), (&dev, &no_secrets)]);
        assert_eq!(values.get("baseUrl").unwrap(), "http://localhost:3000");
    }

    #[test]
    fn disabled_variables_and_secrets_are_handled() {
        let global = environment("Global", vec![("timeout", "5000", false, false)]);
        let dev = environment("dev", vec![("apiKey", "should-not-be-used", true, true)]);
        let no_secrets = std::collections::HashMap::new();
        let mut secrets = std::collections::HashMap::new();
        secrets.insert("apiKey".to_string(), "sk-live-123".to_string());
        let values = collect_variables(&[(&global, &no_secrets), (&dev, &secrets)]);
        assert!(!values.contains_key("timeout"));
        assert_eq!(values.get("apiKey").unwrap(), "sk-live-123");
    }

    #[test]
    fn substitutes_known_variables_and_reports_missing_ones() {
        let mut variables = BTreeMap::new();
        variables.insert("baseUrl".to_string(), "http://localhost:3000".to_string());
        variables.insert("id".to_string(), "42".to_string());

        let mut missing = Vec::new();
        let resolved = substitute(
            "{{ baseUrl }}/api/users/{{id}}?t={{token}}&again={{token}}",
            &variables,
            &mut missing,
        );
        assert_eq!(
            resolved,
            "http://localhost:3000/api/users/42?t={{token}}&again={{token}}"
        );
        assert_eq!(missing, vec!["token".to_string()]);
    }

    #[test]
    fn text_without_placeholders_is_untouched() {
        let mut missing = Vec::new();
        let source = r#"{"name":"John","email":"john@example.com"}"#;
        assert_eq!(substitute(source, &BTreeMap::new(), &mut missing), source);
        assert!(missing.is_empty());
    }

    #[test]
    fn dynamic_builtins_are_resolved() {
        let mut missing = Vec::new();
        let resolved = substitute("{{$uuid}}", &BTreeMap::new(), &mut missing);
        assert_eq!(resolved.len(), 36);
        assert!(missing.is_empty());

        let timestamp = substitute("{{$timestamp}}", &BTreeMap::new(), &mut missing);
        assert!(timestamp.parse::<i64>().is_ok());
    }

    #[test]
    fn cjk_values_round_trip() {
        let mut variables = BTreeMap::new();
        variables.insert("name".to_string(), "张三".to_string());
        let mut missing = Vec::new();
        assert_eq!(
            substitute("{\"name\":\"{{name}}\"}", &variables, &mut missing),
            "{\"name\":\"张三\"}"
        );
    }
}
