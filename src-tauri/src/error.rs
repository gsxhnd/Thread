//! Generic, serializable error type for Tauri commands.
//!
//! 可复用的错误骨架：命令返回的错误会被序列化为带 `code` 的 JSON 负载，方便
//! 前端根据错误码做本地化与差异化处理。业务模块可按需扩展构造函数。
//!
//! 作为模板预留的基础设施，暂无调用方，因此允许 dead_code。

#![allow(dead_code)]

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ThreadError {
    Message(String),
    Coded {
        code: &'static str,
        message: String,
        details: Option<String>,
    },
}

impl std::fmt::Display for ThreadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().to_payload().message)
    }
}

impl std::error::Error for ThreadError {}

impl ThreadError {
    pub fn network(message: impl Into<String>) -> Self {
        Self::Coded {
            code: "network_error",
            message: message.into(),
            details: None,
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Coded {
            code: "config_error",
            message: message.into(),
            details: None,
        }
    }

    pub fn to_payload(self) -> ThreadErrorPayload {
        match self {
            Self::Message(message) => ThreadErrorPayload {
                code: "unknown_error".into(),
                message,
                details: None,
            },
            Self::Coded {
                code,
                message,
                details,
            } => ThreadErrorPayload {
                code: code.into(),
                message,
                details,
            },
        }
    }
}

impl From<std::io::Error> for ThreadError {
    fn from(value: std::io::Error) -> Self {
        Self::network(value.to_string())
    }
}

impl From<String> for ThreadError {
    fn from(value: String) -> Self {
        Self::config(value)
    }
}

pub type ThreadResult<T> = Result<T, ThreadError>;

pub fn thread_err<T>(error: ThreadError) -> Result<T, String> {
    let payload = error.to_payload();
    Err(serde_json::to_string(&payload).unwrap_or(payload.message))
}
