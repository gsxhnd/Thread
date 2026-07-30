//! AI BYOK proxy — OpenAI-compatible chat completions with streaming emit.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::models::{AiCompleteResult, AiConfigPublic};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AiConfigFile {
    base_url: String,
    model: String,
    api_key: String,
}

pub struct AiState {
    cancellations: Mutex<HashMap<String, bool>>,
}

impl AiState {
    pub fn new() -> Self {
        Self {
            cancellations: Mutex::new(HashMap::new()),
        }
    }

    pub fn cancel(&self, request_id: &str) {
        self.cancellations.lock().insert(request_id.to_string(), true);
    }

    pub fn is_cancelled(&self, request_id: &str) -> bool {
        self.cancellations
            .lock()
            .get(request_id)
            .copied()
            .unwrap_or(false)
    }

    pub fn clear(&self, request_id: &str) {
        self.cancellations.lock().remove(request_id);
    }
}

fn ai_config_path() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("thread")
        .join("ai_secrets.json")
}

fn load_config() -> AppResult<AiConfigFile> {
    let path = ai_config_path();
    if !path.exists() {
        return Ok(AiConfigFile::default());
    }
    let raw = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn save_config(config: &AiConfigFile) -> AppResult<()> {
    let path = ai_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(&path, raw)?;
    Ok(())
}

pub fn ai_config_set(base_url: &str, api_key: &str, model: &str) -> AppResult<()> {
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err(AppError::Validation(
            "baseUrl and model are required".into(),
        ));
    }
    let existing = load_config().unwrap_or_else(|_| AiConfigFile::default());
    let next_key = if api_key.trim().is_empty() {
        if existing.api_key.is_empty() {
            return Err(AppError::Validation("apiKey is required".into()));
        }
        existing.api_key
    } else {
        api_key.trim().to_string()
    };
    save_config(&AiConfigFile {
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        model: model.trim().to_string(),
        api_key: next_key,
    })
}

pub fn ai_config_get() -> AppResult<AiConfigPublic> {
    let cfg = load_config()?;
    Ok(AiConfigPublic {
        base_url: cfg.base_url,
        model: cfg.model,
        has_key: !cfg.api_key.is_empty(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletePayload {
    pub mode: String,
    pub prompt: String,
    pub context: Option<String>,
    pub stream: Option<bool>,
    pub request_id: Option<String>,
}

pub async fn ai_complete(
    app: AppHandle,
    state: Arc<AiState>,
    payload: AiCompletePayload,
) -> AppResult<AiCompleteResult> {
    let cfg = load_config()?;
    if cfg.api_key.is_empty() {
        return Err(AppError::AiNoKey);
    }
    if payload.prompt.trim().is_empty() {
        return Err(AppError::Validation("prompt is required".into()));
    }

    let request_id = payload.request_id.clone().unwrap_or_else(new_id);
    let stream = payload.stream.unwrap_or(true);
    let system = system_prompt_for_mode(&payload.mode);
    let user_content = match &payload.context {
        Some(ctx) if !ctx.trim().is_empty() => {
            format!("【设定/上下文】\n{ctx}\n\n【用户指令】\n{}", payload.prompt)
        }
        _ => payload.prompt.clone(),
    };

    let url = format!("{}/v1/chat/completions", cfg.base_url);
    let body = json!({
        "model": cfg.model,
        "stream": stream,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user_content }
        ]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Ai(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let _ = app.emit(
            "ai://error",
            json!({
                "requestId": request_id,
                "code": "AI_ERROR",
                "message": format!("{status}: {text}")
            }),
        );
        return Err(AppError::Ai(format!("{status}: {text}")));
    }

    if !stream {
        let data: Value = response
            .json()
            .await
            .map_err(|e| AppError::Ai(e.to_string()))?;
        let content = data
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let _ = app.emit(
            "ai://chunk",
            json!({ "requestId": request_id, "delta": content }),
        );
        let finish = data
            .pointer("/choices/0/finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop");
        let _ = app.emit(
            "ai://done",
            json!({ "requestId": request_id, "finishReason": finish }),
        );
        state.clear(&request_id);
        return Ok(AiCompleteResult {
            finish_reason: finish.to_string(),
        });
    }

    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut finish_reason = "stop".to_string();

    while let Some(chunk) = byte_stream.next().await {
        if state.is_cancelled(&request_id) {
            finish_reason = "cancelled".into();
            break;
        }
        let chunk = chunk.map_err(|e| AppError::Ai(e.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                continue;
            }
            let Ok(parsed) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            if let Some(delta) = parsed
                .pointer("/choices/0/delta/content")
                .and_then(|v| v.as_str())
            {
                let _ = app.emit(
                    "ai://chunk",
                    json!({ "requestId": request_id, "delta": delta }),
                );
            }
            if let Some(fr) = parsed
                .pointer("/choices/0/finish_reason")
                .and_then(|v| v.as_str())
            {
                finish_reason = fr.to_string();
            }
        }
    }

    let _ = app.emit(
        "ai://done",
        json!({ "requestId": request_id, "finishReason": finish_reason }),
    );
    state.clear(&request_id);
    Ok(AiCompleteResult { finish_reason })
}

fn system_prompt_for_mode(mode: &str) -> &'static str {
    match mode {
        "continue" => "你是一位长篇小说写作助手。根据已有正文自然续写，保持文风与人物一致，直接输出续写内容。",
        "polish" => "你是一位文学润色助手。在保持原意的前提下优化表达，直接输出润色后的文本。",
        "inspire" => "你是一位故事灵感助手。给出具体、可用的情节/对白/冲突点子，简洁有力。",
        "ask" => "你是一位故事设定顾问。基于提供的人物/世界观设定回答问题，不确定时明确说明。",
        _ => "你是一位小说创作助手。",
    }
}
