//! Application error type serialized as `{ code, message }` for IPC.
//!
//! 应用统一错误类型。所有 command 返回 `AppResult<T>`，错误序列化为
//! `{ code, message }`（见 docs/04.ipc-api.md §7）。

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("db error: {0}")]
    Db(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("tls error: {0}")]
    Tls(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("io error: {0}")]
    Io(String),
    /// Non-domain failures: runtime setup, window creation, unexpected state.
    #[error("internal error: {0}")]
    Internal(String),
    /// Reserved for Phase 2 (gRPC/proto parsing).
    #[allow(dead_code)]
    #[error("proto error: {0}")]
    Proto(String),
    /// Capture proxy failures (bind errors, invalid state).
    #[error("proxy error: {0}")]
    Proxy(String),
    #[error("cancelled")]
    Cancelled,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::Validation(_) => "VALIDATION",
            Self::Db(_) => "DB_ERROR",
            Self::Network(_) => "NETWORK_ERROR",
            Self::Tls(_) => "TLS_ERROR",
            Self::Timeout(_) => "TIMEOUT",
            Self::Io(_) => "IO_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Proto(_) => "PROTO_ERROR",
            Self::Proxy(_) => "PROXY_ERROR",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn cancelled() -> Self {
        Self::Cancelled
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorPayload {
    pub code: String,
    pub message: String,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        AppErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Validation(value.to_string())
    }
}

impl AppError {
    /// Classifies a hyper / rustls failure. Timeouts raised by `tokio::time::timeout`
    /// are reported separately by the caller.
    pub fn from_transport(error: &(dyn std::error::Error + 'static)) -> Self {
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
        while let Some(current) = source {
            let text = current.to_string().to_lowercase();
            if text.contains("certificate")
                || text.contains("tls")
                || text.contains("ssl")
                || text.contains("handshake")
            {
                return Self::Tls(error.to_string());
            }
            source = std::error::Error::source(current);
        }
        Self::Network(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_payload_exposes_code_and_message() {
        let error = AppError::NotFound("request.json".into());
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
        assert_eq!(json["message"], "not found: request.json");
    }

    #[test]
    fn cancelled_maps_to_its_own_code() {
        assert_eq!(AppError::cancelled().code(), "CANCELLED");
    }
}
