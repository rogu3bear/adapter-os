//! OpenAI-compatible API shim.
//!
//! Implements a minimal subset of OpenAI's Chat Completions API by translating
//! requests into AdapterOS `/v1/infer` calls internally.

use crate::auth::Claims;
use crate::handlers;
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::state::AppState;
use crate::types::{ErrorResponse, InferRequest, StopReasonCode};
use adapteros_core::identity::IdentityEnvelope;
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsRequest {
    pub model: Option<String>,
    pub messages: Vec<OpenAiChatMessage>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_completion_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub n: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: Value,
}

#[derive(Debug, Serialize)]
pub struct OpenAiChatCompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiChatChoice {
    pub index: usize,
    pub message: OpenAiChatMessageResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiChatMessageResponse {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

fn estimate_tokens(s: &str) -> usize {
    s.len().div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorResponse {
    pub error: OpenAiErrorBody,
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorBody {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

fn openai_error(
    message: impl Into<String>,
    code: Option<String>,
    param: Option<String>,
) -> OpenAiErrorResponse {
    OpenAiErrorResponse {
        error: OpenAiErrorBody {
            message: message.into(),
            error_type: "invalid_request_error".to_string(),
            code,
            param,
        },
    }
}

fn content_to_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Null => Some(String::new()),
        Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                let Value::Object(map) = part else { continue };
                let Some(part_type) = map.get("type").and_then(Value::as_str) else {
                    continue;
                };
                if part_type != "text" {
                    continue;
                }
                if let Some(text) = map.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn messages_to_prompt(messages: &[OpenAiChatMessage]) -> Result<String, OpenAiErrorResponse> {
    if messages.is_empty() {
        return Err(openai_error(
            "`messages` must be a non-empty array",
            Some("MISSING_MESSAGES".to_string()),
            Some("messages".to_string()),
        ));
    }

    let mut prompt = String::new();
    for (idx, msg) in messages.iter().enumerate() {
        if idx > 0 {
            prompt.push('\n');
        }
        let role = msg.role.trim().to_lowercase();
        let content = content_to_text(&msg.content).ok_or_else(|| {
            openai_error(
                "unsupported `messages[].content` type (expected string or array of text parts)",
                Some("UNSUPPORTED_MESSAGE_CONTENT".to_string()),
                Some("messages[].content".to_string()),
            )
        })?;

        prompt.push_str(&format!("[{}]: {}", role, content));
    }

    Ok(prompt)
}

fn map_finish_reason(stop_reason_code: Option<StopReasonCode>) -> Option<String> {
    match stop_reason_code {
        Some(StopReasonCode::Length) | Some(StopReasonCode::BudgetMax) => {
            Some("length".to_string())
        }
        Some(StopReasonCode::CompletionConfident)
        | Some(StopReasonCode::RepetitionGuard)
        | Some(StopReasonCode::StopSequence) => Some("stop".to_string()),
        None => None,
    }
}

/// OpenAI-compatible chat completions endpoint.
///
/// Translates the request into a deterministic prompt and forwards it to `/v1/infer`.
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    Json(req): Json<OpenAiChatCompletionsRequest>,
) -> Result<Json<OpenAiChatCompletionsResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
    if req.stream.unwrap_or(false) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "`stream=true` is not supported; use non-streaming requests",
                Some("STREAMING_UNSUPPORTED".to_string()),
                Some("stream".to_string()),
            )),
        ));
    }

    if let Some(n) = req.n {
        if n > 1 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(openai_error(
                    "`n>1` is not supported; request a single completion",
                    Some("N_UNSUPPORTED".to_string()),
                    Some("n".to_string()),
                )),
            ));
        }
    }

    let prompt =
        messages_to_prompt(&req.messages).map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;
    let prompt_tokens_estimate = estimate_tokens(&prompt);

    let infer_req = InferRequest {
        prompt,
        model: req.model.clone(),
        max_tokens: req
            .max_tokens
            .or(req.max_completion_tokens)
            .map(|v| v as usize),
        temperature: req.temperature,
        top_p: req.top_p,
        ..Default::default()
    };

    let infer_resp = match handlers::inference::infer(
        State(state),
        Extension(claims),
        Extension(identity),
        request_id,
        api_key,
        Json(infer_req),
    )
    .await
    {
        Ok(Json(r)) => r,
        Err((status, Json(err))) => {
            return Err((status, Json(map_adapteros_error_to_openai(err))));
        }
    };

    let model = req
        .model
        .clone()
        .or(infer_resp.model.clone())
        .unwrap_or_else(|| "adapteros".to_string());

    let prompt_tokens = infer_resp.prompt_tokens.unwrap_or(prompt_tokens_estimate);
    let usage = Some(OpenAiUsage {
        prompt_tokens,
        completion_tokens: infer_resp.tokens_generated,
        total_tokens: prompt_tokens.saturating_add(infer_resp.tokens_generated),
    });

    let response = OpenAiChatCompletionsResponse {
        id: format!("chatcmpl-{}", infer_resp.id),
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp(),
        model,
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessageResponse {
                role: "assistant".to_string(),
                content: infer_resp.text,
            },
            finish_reason: map_finish_reason(infer_resp.stop_reason_code)
                .or_else(|| Some("stop".to_string())),
        }],
        usage,
    };

    Ok(Json(response))
}

fn map_adapteros_error_to_openai(err: ErrorResponse) -> OpenAiErrorResponse {
    let mut message = err.error;
    if let Some(details) = err.details {
        if let Ok(details_str) = serde_json::to_string(&details) {
            message = format!("{} ({})", message, details_str);
        }
    }
    openai_error(message, Some(err.code), None)
}
