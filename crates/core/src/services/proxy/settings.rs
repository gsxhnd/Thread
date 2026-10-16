//! Capture preferences (app-level, stored in `config.toml` section `proxy_settings`).
//!
//! 端口属于工作区配置（`thread.workspace.json` 的 `proxy.port`），这里只放与抓包行为有关的
//! 应用级偏好：是否持久化、限额、敏感字段隐藏、HTTPS 解密开关与 CA 信任确认。

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_ENTRIES: usize = 1_000;
pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
const MIN_MAX_ENTRIES: usize = 50;
const MAX_MAX_ENTRIES: usize = 50_000;
const MAX_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptureSettings {
    /// 写入 SQLite（会话与条目元数据、Body 文件）。关闭时流量只保留在内存中。
    pub persist_sessions: bool,
    /// 每个会话保留的最大条目数（内存与持久化共用）。
    pub max_entries: usize,
    /// 单个 Body 最多保存的字节数；超出部分只计数，不保存。0 表示不保存 Body。
    pub max_body_bytes: usize,
    /// 隐藏 Authorization / Cookie 等请求头与敏感查询参数的值。
    pub mask_sensitive: bool,
    /// 允许对 CONNECT 目标做 HTTPS 解密（仍需 `ca_trust_acknowledged`）。
    pub decrypt_https: bool,
    /// 用户确认已在操作系统或浏览器中信任 Thread CA。未确认前不做 MITM 解密。
    pub ca_trust_acknowledged: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            persist_sessions: false,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            mask_sensitive: true,
            decrypt_https: false,
            ca_trust_acknowledged: false,
        }
    }
}

impl CaptureSettings {
    /// Clamps user input into supported ranges so a bad value cannot disable limits.
    pub fn normalized(mut self) -> Self {
        self.max_entries = self.max_entries.clamp(MIN_MAX_ENTRIES, MAX_MAX_ENTRIES);
        self.max_body_bytes = self.max_body_bytes.min(MAX_MAX_BODY_BYTES);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_never_persist_and_never_claim_trust() {
        let settings = CaptureSettings::default();
        assert!(!settings.persist_sessions);
        assert!(!settings.ca_trust_acknowledged);
        assert!(!settings.decrypt_https);
        assert!(settings.mask_sensitive);
    }

    #[test]
    fn normalization_clamps_limits() {
        let settings = CaptureSettings {
            max_entries: 1,
            max_body_bytes: usize::MAX,
            ..CaptureSettings::default()
        }
        .normalized();
        assert_eq!(settings.max_entries, MIN_MAX_ENTRIES);
        assert_eq!(settings.max_body_bytes, MAX_MAX_BODY_BYTES);
    }

    #[test]
    fn partial_json_uses_defaults() {
        let settings: CaptureSettings =
            serde_json::from_str(r#"{"persistSessions": true}"#).unwrap();
        assert!(settings.persist_sessions);
        assert_eq!(settings.max_entries, DEFAULT_MAX_ENTRIES);
    }
}
