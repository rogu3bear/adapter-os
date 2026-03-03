//! OpenAI-compatible API shim.
//!
//! Implements a minimal subset of OpenAI's Chat Completions API by translating
//! requests into adapterOS `/v1/infer` calls internally.
//!
//! Supports both streaming (`stream=true`) and non-streaming requests.
//! Streaming responses use Server-Sent Events (SSE) with OpenAI-compatible chunk format.

use crate::auth::Claims;
use crate::backpressure::check_uma_backpressure;
use crate::handlers;
use crate::handlers::streaming_infer::{Delta, StreamingChoice, StreamingChunk};
use crate::inference_cache::{CachedInferenceResultBuilder, InferenceCacheKey};
use crate::inference_core::InferenceCore;
use crate::ip_extraction::ClientIp;
use crate::middleware::canonicalization::CanonicalRequest;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::session_tokens::{resolve_session_token_lock, SessionTokenContext};
use crate::state::AppState;
use crate::types::run_envelope::new_run_envelope;
use crate::types::{
    ErrorResponse, InferRequest, InferenceRequestInternal, StopPolicySpec, StopReasonCode,
    DEFAULT_MAX_TOKENS, MAX_REPLAY_TEXT_SIZE,
};
use crate::uds_client::WorkerStreamToken;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::B3Hash;
use adapteros_policy::hooks::PolicyHook;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenAiChatCompletionsRequest {
    pub model: Option<String>,
    pub messages: Vec<OpenAiChatMessage>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_completion_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub n: Option<u32>,
    pub response_format: Option<OpenAiResponseFormat>,
    pub tools: Option<Vec<OpenAiTool>>,
    pub tool_choice: Option<OpenAiToolChoice>,
    pub seed: Option<u64>,
    pub stop: Option<OpenAiStop>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub logprobs: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<OpenAiJsonSchemaFormat>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiJsonSchemaFormat {
    pub name: String,
    #[schema(value_type = Object)]
    pub schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub parameters: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum OpenAiToolChoice {
    Mode(String),
    Named(OpenAiNamedToolChoice),
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiNamedToolChoice {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiNamedToolFunction,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OpenAiNamedToolFunction {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum OpenAiStop {
    Single(String),
    Multiple(Vec<String>),
}

impl OpenAiStop {
    fn as_sequences(&self) -> Vec<String> {
        match self {
            Self::Single(value) => vec![value.clone()],
            Self::Multiple(values) => values.clone(),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenAiChatMessage {
    pub role: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub content: Value,
}

/// OpenAI-compatible chat completions response.
///
/// ## Receipt Digest Extraction
///
/// The `receipt_digest` from the underlying inference is embedded in this response:
/// - **`id`**: Format `chatcmpl_{receipt_digest_hex_prefix}` (first 16 hex chars).
/// - **`system_fingerprint`**: The full 64-character hex-encoded receipt digest.
///
/// Clients can extract the receipt digest for verification/audit by:
/// 1. Using `system_fingerprint` directly (full hex digest)
/// 2. Looking up the receipt via `/v1/runs/{run_id}/evidence` endpoint
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiChatCompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    /// System fingerprint for determinism tracking (full receipt_digest as hex).
    ///
    /// This is the BLAKE3 hash of the run receipt, allowing clients to:
    /// - Verify deterministic replay capability
    /// - Look up the full evidence chain via `/v1/runs/{run_id}/evidence`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    pub choices: Vec<OpenAiChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiChatChoice {
    pub index: usize,
    pub message: OpenAiChatMessageResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiChatMessageResponse {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    /// Number of prompt tokens that were served from cache.
    /// Present (Some) only for cache hits; None indicates fresh computation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_prefix_tokens: Option<usize>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiEmbeddingUsage {
    pub prompt_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum OpenAiCompletionPrompt {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenAiCompletionsRequest {
    pub model: Option<String>,
    pub prompt: OpenAiCompletionPrompt,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub n: Option<u32>,
    pub seed: Option<u64>,
    pub stop: Option<OpenAiStop>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub logprobs: Option<bool>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiCompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    /// System fingerprint for determinism tracking (full receipt_digest as hex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    pub choices: Vec<OpenAiCompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiCompletionChoice {
    pub index: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenAiEmbeddingsRequest {
    /// ID of the model to use.
    pub model: Option<String>,
    /// Input text to embed, encoded as a string or array of strings.
    pub input: OpenAiCompletionPrompt,
    /// The format to return the embeddings in. Can be either `float` or `base64`.
    /// Defaults to `float`.
    #[serde(default)]
    pub encoding_format: Option<String>,
    /// A unique identifier representing your end-user, which can help monitor and detect abuse.
    #[serde(default)]
    pub user: Option<String>,
    /// The number of dimensions the resulting output embeddings should have.
    /// Only supported in some models.
    #[serde(default)]
    pub dimensions: Option<usize>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiEmbeddingsResponse {
    pub object: String,
    pub data: Vec<OpenAiEmbeddingItem>,
    pub model: String,
    pub usage: OpenAiEmbeddingUsage,
}

/// Embedding data in either float array or base64 format.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum EmbeddingData {
    /// Array of float values (default encoding_format="float")
    Float { embedding: Vec<f32> },
    /// Base64-encoded embedding (encoding_format="base64")
    Base64 { embedding: String },
}

impl EmbeddingData {
    /// Create float embedding data from a vector of floats.
    pub fn from_float(embedding: Vec<f32>) -> Self {
        EmbeddingData::Float { embedding }
    }

    /// Create base64 embedding data from a vector of floats.
    pub fn from_base64(embedding: Vec<f32>) -> Self {
        use base64::Engine;
        // OpenAI uses little-endian f32 bytes for base64 encoding
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        EmbeddingData::Base64 { embedding: encoded }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiEmbeddingItem {
    pub object: String,
    pub index: usize,
    /// Embedding vector, either as array of floats or base64-encoded string.
    #[serde(flatten)]
    pub embedding: EmbeddingData,
}

const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// Length of receipt digest hex prefix used in OpenAI-compatible response IDs.
/// The ID format is `chatcmpl_{hex_prefix}` where hex_prefix is this many characters.
const RECEIPT_DIGEST_ID_SUFFIX_LEN: usize = 16;

fn estimate_tokens(s: &str) -> usize {
    s.len().div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenAiErrorResponse {
    pub error: OpenAiErrorBody,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
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
    openai_error_typed("invalid_request_error", message, code, param)
}

fn openai_error_typed(
    error_type: &str,
    message: impl Into<String>,
    code: Option<String>,
    param: Option<String>,
) -> OpenAiErrorResponse {
    OpenAiErrorResponse {
        error: OpenAiErrorBody {
            message: message.into(),
            error_type: error_type.to_string(),
            code,
            param,
        },
    }
}

fn classify_openai_error_type(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "permission_error",
        StatusCode::TOO_MANY_REQUESTS => "rate_limit_error",
        s if s.is_server_error() => "server_error",
        _ => "invalid_request_error",
    }
}

fn validate_response_format(
    response_format: Option<&OpenAiResponseFormat>,
) -> Result<(), OpenAiErrorResponse> {
    let Some(response_format) = response_format else {
        return Ok(());
    };

    match response_format.format_type.as_str() {
        "json_object" => Ok(()),
        "json_schema" => {
            let Some(schema_format) = response_format.json_schema.as_ref() else {
                return Err(openai_error(
                    "`response_format.json_schema` is required when type=json_schema",
                    Some("RESPONSE_FORMAT_INVALID".to_string()),
                    Some("response_format.json_schema".to_string()),
                ));
            };
            if schema_format.name.trim().is_empty() {
                return Err(openai_error(
                    "`response_format.json_schema.name` must be non-empty",
                    Some("RESPONSE_FORMAT_INVALID".to_string()),
                    Some("response_format.json_schema.name".to_string()),
                ));
            }
            if !schema_format.schema.is_object() {
                return Err(openai_error(
                    "`response_format.json_schema.schema` must be a JSON object",
                    Some("RESPONSE_FORMAT_INVALID".to_string()),
                    Some("response_format.json_schema.schema".to_string()),
                ));
            }
            Ok(())
        }
        _ => Err(openai_error(
            "`response_format.type` must be `json_object` or `json_schema`",
            Some("RESPONSE_FORMAT_UNSUPPORTED".to_string()),
            Some("response_format.type".to_string()),
        )),
    }
}

fn validate_tools_and_choice(
    tools: Option<&[OpenAiTool]>,
    tool_choice: Option<&OpenAiToolChoice>,
) -> Result<(), OpenAiErrorResponse> {
    if let Some(tool_choice) = tool_choice {
        if tools.map(|items| items.is_empty()).unwrap_or(true) {
            return Err(openai_error(
                "`tool_choice` requires at least one tool in `tools`",
                Some("TOOL_CHOICE_INVALID".to_string()),
                Some("tool_choice".to_string()),
            ));
        }
        match tool_choice {
            OpenAiToolChoice::Mode(mode)
                if mode != "none" && mode != "auto" && mode != "required" =>
            {
                return Err(openai_error(
                    "`tool_choice` must be one of `none`, `auto`, `required`, or a named function",
                    Some("TOOL_CHOICE_UNSUPPORTED".to_string()),
                    Some("tool_choice".to_string()),
                ));
            }
            OpenAiToolChoice::Named(named)
                if named.tool_type != "function" || named.function.name.trim().is_empty() =>
            {
                return Err(openai_error(
                    "named `tool_choice` must target a function name",
                    Some("TOOL_CHOICE_INVALID".to_string()),
                    Some("tool_choice".to_string()),
                ));
            }
            _ => {}
        }
    }

    if let Some(tools) = tools {
        for (idx, tool) in tools.iter().enumerate() {
            if tool.tool_type != "function" {
                return Err(openai_error(
                    "only tools with type `function` are supported",
                    Some("TOOL_TYPE_UNSUPPORTED".to_string()),
                    Some(format!("tools[{idx}].type")),
                ));
            }
            if tool.function.name.trim().is_empty() {
                return Err(openai_error(
                    "`tools[].function.name` must be non-empty",
                    Some("TOOL_INVALID".to_string()),
                    Some(format!("tools[{idx}].function.name")),
                ));
            }
        }
    }

    Ok(())
}

fn validate_penalties_and_logprobs(
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    logprobs: Option<bool>,
) -> Result<(), OpenAiErrorResponse> {
    if let Some(value) = frequency_penalty {
        if value != 0.0 {
            return Err(openai_error(
                "`frequency_penalty` is accepted for compatibility but only `0.0` is supported",
                Some("FREQUENCY_PENALTY_UNSUPPORTED".to_string()),
                Some("frequency_penalty".to_string()),
            ));
        }
    }
    if let Some(value) = presence_penalty {
        if value != 0.0 {
            return Err(openai_error(
                "`presence_penalty` is accepted for compatibility but only `0.0` is supported",
                Some("PRESENCE_PENALTY_UNSUPPORTED".to_string()),
                Some("presence_penalty".to_string()),
            ));
        }
    }
    if let Some(value) = logprobs {
        if value {
            return Err(openai_error(
                "`logprobs=true` is not yet supported",
                Some("LOGPROBS_UNSUPPORTED".to_string()),
                Some("logprobs".to_string()),
            ));
        }
    }
    Ok(())
}

fn stop_policy_from_openai(stop: Option<&OpenAiStop>, max_tokens: usize) -> Option<StopPolicySpec> {
    let stop_sequences = stop
        .map(OpenAiStop::as_sequences)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    if stop_sequences.is_empty() {
        return None;
    }

    let mut policy = StopPolicySpec::new(max_tokens.min(u32::MAX as usize) as u32);
    policy.stop_sequences = stop_sequences;
    Some(policy)
}

fn validate_response_format_output(
    response_format: Option<&OpenAiResponseFormat>,
    output_text: &str,
) -> Result<(), OpenAiErrorResponse> {
    let Some(response_format) = response_format else {
        return Ok(());
    };
    let parsed = serde_json::from_str::<Value>(output_text).map_err(|_| {
        openai_error(
            "model output is not valid JSON for requested `response_format`",
            Some("RESPONSE_FORMAT_VALIDATION_FAILED".to_string()),
            Some("response_format".to_string()),
        )
    })?;

    if response_format.format_type == "json_schema" {
        let schema = response_format
            .json_schema
            .as_ref()
            .map(|schema| &schema.schema)
            .ok_or_else(|| {
                openai_error(
                    "`response_format.json_schema` is required when type=json_schema",
                    Some("RESPONSE_FORMAT_INVALID".to_string()),
                    Some("response_format.json_schema".to_string()),
                )
            })?;
        apply_minimal_json_schema_validation(&parsed, schema)?;
    }

    Ok(())
}

fn apply_minimal_json_schema_validation(
    output: &Value,
    schema: &Value,
) -> Result<(), OpenAiErrorResponse> {
    let Some(required) = schema.get("required").and_then(Value::as_array) else {
        return Ok(());
    };
    let Some(object) = output.as_object() else {
        return Err(openai_error(
            "model output must be a JSON object for the requested schema",
            Some("RESPONSE_SCHEMA_MISMATCH".to_string()),
            Some("response_format.json_schema.schema".to_string()),
        ));
    };

    for field in required.iter().filter_map(Value::as_str) {
        if !object.contains_key(field) {
            return Err(openai_error(
                format!("model output is missing required field `{field}`"),
                Some("RESPONSE_SCHEMA_MISMATCH".to_string()),
                Some("response_format.json_schema.schema.required".to_string()),
            ));
        }
    }

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property_schema) in properties {
            let Some(value) = object.get(name) else {
                continue;
            };
            let Some(expected_type) = property_schema.get("type").and_then(Value::as_str) else {
                continue;
            };
            let type_ok = match expected_type {
                "string" => value.is_string(),
                "number" => value.is_number(),
                "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => true,
            };
            if !type_ok {
                return Err(openai_error(
                    format!(
                        "model output field `{name}` does not match schema type `{expected_type}`"
                    ),
                    Some("RESPONSE_SCHEMA_MISMATCH".to_string()),
                    Some("response_format.json_schema.schema.properties".to_string()),
                ));
            }
        }
    }

    Ok(())
}

fn build_tool_call_response(
    req: &OpenAiChatCompletionsRequest,
    text: String,
) -> (OpenAiChatMessageResponse, Option<String>) {
    let Some(tools) = req.tools.as_ref().filter(|tools| !tools.is_empty()) else {
        return (
            OpenAiChatMessageResponse {
                role: "assistant".to_string(),
                content: Some(text),
                tool_calls: None,
            },
            None,
        );
    };

    let chosen_name = match req.tool_choice.as_ref() {
        Some(OpenAiToolChoice::Named(named)) => Some(named.function.name.clone()),
        _ => tools.first().map(|tool| tool.function.name.clone()),
    }
    .unwrap_or_else(|| "tool".to_string());

    let tool_calls = vec![OpenAiToolCall {
        id: "call_0001".to_string(),
        tool_type: "function".to_string(),
        function: OpenAiToolCallFunction {
            name: chosen_name,
            arguments: text,
        },
    }];

    (
        OpenAiChatMessageResponse {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(tool_calls),
        },
        Some("tool_calls".to_string()),
    )
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

/// Convert OpenAI-format messages into both a legacy prompt string and structured
/// [`ChatMessage`] values suitable for model-aware template formatting.
fn messages_to_prompt_and_messages(
    messages: &[OpenAiChatMessage],
) -> Result<(String, Vec<adapteros_types::inference::ChatMessage>), OpenAiErrorResponse> {
    if messages.is_empty() {
        return Err(openai_error(
            "`messages` must be a non-empty array",
            Some("MISSING_MESSAGES".to_string()),
            Some("messages".to_string()),
        ));
    }

    let mut prompt = String::new();
    let mut chat_messages = Vec::with_capacity(messages.len());

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

        // Legacy flat prompt for backward compatibility
        prompt.push_str(&format!("[{}]: {}", role, content));

        // Structured message for model-aware template formatting
        chat_messages.push(adapteros_types::inference::ChatMessage { role, content });
    }

    Ok((prompt, chat_messages))
}

fn map_finish_reason(stop_reason_code: Option<StopReasonCode>) -> Option<String> {
    match stop_reason_code {
        Some(StopReasonCode::Length) | Some(StopReasonCode::BudgetMax) => {
            Some("length".to_string())
        }
        Some(StopReasonCode::CompletionConfident)
        | Some(StopReasonCode::RepetitionGuard)
        | Some(StopReasonCode::StopSequence) => Some("stop".to_string()),
        Some(StopReasonCode::Cancelled) => Some("cancelled".to_string()),
        Some(StopReasonCode::SystemError) => Some("error".to_string()),
        None => None,
    }
}

/// OpenAI-compatible chat completions endpoint.
///
/// Translates the request into a deterministic prompt and forwards it to `/v1/infer`.
/// Supports both streaming (`stream=true`) and non-streaming requests.
///
/// ## Streaming Response
/// When `stream=true`, returns Server-Sent Events with OpenAI-compatible chunk format:
/// ```text
/// data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","choices":[{"delta":{"role":"assistant"}}]}
/// data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"}}]}
/// data: {"id":"chatcmpl_xxx","object":"chat.completion.chunk","choices":[{"delta":{},"finish_reason":"stop"}]}
/// data: [DONE]
/// ```
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    responses(
        (status = 200, description = "Chat completion response", body = OpenAiChatCompletionsResponse),
        (status = 400, description = "Bad request", body = OpenAiErrorResponse),
        (status = 500, description = "Internal server error", body = OpenAiErrorResponse)
    ),
    tag = "openai"
)]
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    session_token: Option<Extension<SessionTokenContext>>,
    canonical_request: Option<Extension<CanonicalRequest>>,
    Json(req): Json<OpenAiChatCompletionsRequest>,
) -> Response {
    // Validate n parameter first (applies to both streaming and non-streaming)
    if let Some(n) = req.n {
        if n > 1 {
            return (
                StatusCode::BAD_REQUEST,
                Json(openai_error(
                    "`n>1` is not supported; request a single completion",
                    Some("N_UNSUPPORTED".to_string()),
                    Some("n".to_string()),
                )),
            )
                .into_response();
        }
    }
    if let Err(err) = validate_response_format(req.response_format.as_ref()) {
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }
    if let Err(err) = validate_tools_and_choice(req.tools.as_deref(), req.tool_choice.as_ref()) {
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }
    if let Err(err) =
        validate_penalties_and_logprobs(req.frequency_penalty, req.presence_penalty, req.logprobs)
    {
        return (StatusCode::BAD_REQUEST, Json(err)).into_response();
    }
    if req.stream.unwrap_or(false)
        && (req.tools.as_ref().is_some_and(|items| !items.is_empty())
            || req.response_format.is_some())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "streaming does not currently support `tools` or `response_format` in compatibility mode",
                Some("STREAM_COMPATIBILITY_UNSUPPORTED".to_string()),
                Some("stream".to_string()),
            )),
        )
            .into_response();
    }

    // Branch based on streaming mode
    if req.stream.unwrap_or(false) {
        // Note: streaming does not use cache (cache only after stream completes)
        match chat_completions_streaming(
            State(state),
            Extension(claims),
            Extension(client_ip),
            session_token,
            req,
        )
        .await
        {
            Ok(sse) => sse.into_response(),
            Err((status, Json(err))) => (status, Json(err)).into_response(),
        }
    } else {
        match chat_completions_non_streaming(
            State(state),
            Extension(claims),
            Extension(client_ip),
            Extension(identity),
            request_id,
            api_key,
            session_token,
            canonical_request,
            req,
        )
        .await
        {
            Ok(json) => json.into_response(),
            Err((status, Json(err))) => (status, Json(err)).into_response(),
        }
    }
}

/// Non-streaming chat completions handler with semantic caching.
///
/// When caching is enabled, this handler:
/// 1. Computes a cache key from (context_id, canonical_input_digest)
/// 2. Checks for cached responses before running inference
/// 3. On cache hit, returns the cached response immediately with original receipt_digest
/// 4. On cache miss, runs inference and stores the result for future requests
async fn chat_completions_non_streaming(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    session_token: Option<Extension<SessionTokenContext>>,
    canonical_request: Option<Extension<CanonicalRequest>>,
    req: OpenAiChatCompletionsRequest,
) -> Result<Json<OpenAiChatCompletionsResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
    let (prompt, chat_messages) = messages_to_prompt_and_messages(&req.messages)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;
    let prompt_tokens_estimate = estimate_tokens(&prompt);

    // Compute cache key from canonical request if available
    // The cache key combines:
    // - context_id: derived from model + temperature + max_tokens (request parameters)
    // - canonical_digest: from canonicalization middleware (captures full request body)
    let cache_key = canonical_request.as_ref().map(|cr| {
        // Compute context_id from request parameters (model config hash)
        // This is a simplified version that doesn't require runtime model hash
        let context_data = format!(
            "model:{};temp:{};max_tokens:{};top_p:{}",
            req.model.as_deref().unwrap_or("default"),
            req.temperature.unwrap_or(0.0),
            req.max_tokens.or(req.max_completion_tokens).unwrap_or(0),
            req.top_p.unwrap_or(1.0),
        );
        let context_id = B3Hash::hash(context_data.as_bytes());
        let canonical_digest = cr.0.digest;

        // Use per-tenant caching if configured, otherwise global
        let cache = state.inference_cache();
        let tenant_id = if cache.is_enabled() && cache.is_per_tenant() {
            // Per-tenant caching enabled - extract tenant_id from claims
            Some(claims.tenant_id.clone())
        } else {
            None
        };

        InferenceCacheKey::new(context_id, canonical_digest, tenant_id)
    });

    // Check cache for existing response
    if let Some(ref key) = cache_key {
        if let Some(cached) = state.inference_cache().get(key) {
            debug!(
                original_request_id = %cached.original_request_id,
                prompt_tokens = cached.prompt_tokens,
                completion_tokens = cached.completion_tokens,
                "Semantic cache hit - returning cached response"
            );

            // Build response from cached result
            let id = match &cached.receipt_digest_hex {
                Some(hex) => {
                    let prefix = &hex[..RECEIPT_DIGEST_ID_SUFFIX_LEN.min(hex.len())];
                    format!("chatcmpl_{}", prefix)
                }
                None => format!(
                    "chatcmpl_cached_{}",
                    &cached.original_request_id[..8.min(cached.original_request_id.len())]
                ),
            };

            let model = cached
                .model
                .clone()
                .unwrap_or_else(|| req.model.clone().unwrap_or_else(|| "adapteros".to_string()));

            // For cached responses, cached_prefix_tokens indicates this was a cache hit
            // and shows how many prompt tokens were served from cache
            let usage = Some(OpenAiUsage {
                prompt_tokens: cached.prompt_tokens,
                completion_tokens: cached.completion_tokens,
                total_tokens: cached
                    .prompt_tokens
                    .saturating_add(cached.completion_tokens),
                cached_prefix_tokens: Some(cached.prompt_tokens),
            });
            validate_response_format_output(req.response_format.as_ref(), &cached.output_text)
                .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;
            let (message, tool_finish_reason) =
                build_tool_call_response(&req, cached.output_text.clone());

            let response = OpenAiChatCompletionsResponse {
                id,
                object: "chat.completion".to_string(),
                created: Utc::now().timestamp(),
                model,
                system_fingerprint: cached.receipt_digest_hex.clone(),
                choices: vec![OpenAiChatChoice {
                    index: 0,
                    message,
                    finish_reason: tool_finish_reason
                        .or_else(|| Some(cached.finish_reason.clone())),
                }],
                usage,
            };

            return Ok(Json(response));
        }
    }

    // Cache miss - run inference
    let infer_req = InferRequest {
        prompt,
        messages: Some(chat_messages),
        model: req.model.clone(),
        max_tokens: req
            .max_tokens
            .or(req.max_completion_tokens)
            .map(|v| v as usize),
        temperature: Some(req.temperature.unwrap_or(0.0)),
        top_p: Some(req.top_p.unwrap_or(1.0)),
        seed: req.seed,
        stop_policy: stop_policy_from_openai(
            req.stop.as_ref(),
            req.max_tokens
                .or(req.max_completion_tokens)
                .map(|v| v as usize)
                .unwrap_or(DEFAULT_MAX_TOKENS),
        ),
        ..Default::default()
    };

    let request_id_for_cache = request_id
        .as_ref()
        .map(|r| r.0 .0.clone())
        .unwrap_or_else(crate::id_generator::readable_request_id);

    let infer_resp = match handlers::inference::infer(
        State(state.clone()),
        Extension(claims),
        Extension(client_ip),
        Extension(identity),
        request_id,
        api_key,
        session_token,
        Json(infer_req),
    )
    .await
    {
        Ok(Json(r)) => r,
        Err(api_error) => {
            let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = api_error.into();
            return Err((status, Json(map_adapteros_error_to_openai(status, err))));
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
        cached_prefix_tokens: None, // Fresh computation, not from cache
    });

    // Extract receipt_digest from run_receipt for determinism tracking
    // Convert B3Hash to hex string for system_fingerprint
    let receipt_digest_hex = infer_resp
        .run_receipt
        .as_ref()
        .map(|r| r.receipt_digest.to_hex());

    let id = match &infer_resp.run_receipt {
        Some(receipt) => {
            let hex = receipt.receipt_digest.to_hex();
            let prefix = &hex[..RECEIPT_DIGEST_ID_SUFFIX_LEN.min(hex.len())];
            format!("chatcmpl_{}", prefix)
        }
        None => format!("chatcmpl_{}", infer_resp.id),
    };

    let finish_reason =
        map_finish_reason(infer_resp.stop_reason_code).unwrap_or_else(|| "stop".to_string());
    validate_response_format_output(req.response_format.as_ref(), &infer_resp.text)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;
    let (message, tool_finish_reason) = build_tool_call_response(&req, infer_resp.text.clone());

    // Store result in cache for future requests
    if let Some(key) = cache_key {
        let cached_result =
            CachedInferenceResultBuilder::new(infer_resp.text.clone(), request_id_for_cache)
                .with_tokens_generated(infer_resp.tokens_generated)
                .with_usage(prompt_tokens, infer_resp.tokens_generated)
                .with_receipt_digest(receipt_digest_hex.clone())
                .with_model(Some(model.clone()))
                .with_finish_reason(finish_reason.clone())
                .build();

        state.inference_cache().put(key, cached_result);
        debug!(
            prompt_tokens = prompt_tokens,
            completion_tokens = infer_resp.tokens_generated,
            "Stored inference result in semantic cache"
        );
    }

    let response = OpenAiChatCompletionsResponse {
        id,
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp(),
        model,
        system_fingerprint: receipt_digest_hex,
        choices: vec![OpenAiChatChoice {
            index: 0,
            message,
            finish_reason: tool_finish_reason.or(Some(finish_reason)),
        }],
        usage,
    };

    Ok(Json(response))
}

fn completion_prompt_to_text(
    prompt: &OpenAiCompletionPrompt,
) -> Result<String, OpenAiErrorResponse> {
    match prompt {
        OpenAiCompletionPrompt::Single(text) => Ok(text.clone()),
        OpenAiCompletionPrompt::Batch(items) => {
            if items.is_empty() {
                return Err(openai_error(
                    "`prompt` must be a non-empty string or array",
                    Some("MISSING_PROMPT".to_string()),
                    Some("prompt".to_string()),
                ));
            }
            if items.len() > 1 {
                return Err(openai_error(
                    "multiple prompts are not supported; submit a single prompt",
                    Some("PROMPT_BATCH_UNSUPPORTED".to_string()),
                    Some("prompt".to_string()),
                ));
            }
            Ok(items[0].clone())
        }
    }
}

fn embedding_inputs(prompt: &OpenAiCompletionPrompt) -> Result<Vec<String>, OpenAiErrorResponse> {
    match prompt {
        OpenAiCompletionPrompt::Single(text) => Ok(vec![text.clone()]),
        OpenAiCompletionPrompt::Batch(items) => {
            if items.is_empty() {
                return Err(openai_error(
                    "`input` must be a non-empty string or array",
                    Some("MISSING_INPUT".to_string()),
                    Some("input".to_string()),
                ));
            }
            Ok(items.clone())
        }
    }
}

/// OpenAI-compatible completions endpoint (legacy).
#[utoipa::path(
    post,
    path = "/v1/completions",
    responses(
        (status = 200, description = "Completion response", body = OpenAiCompletionsResponse),
        (status = 400, description = "Bad request", body = OpenAiErrorResponse),
        (status = 500, description = "Internal server error", body = OpenAiErrorResponse)
    ),
    tag = "openai"
)]
pub async fn completions_openai(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    session_token: Option<Extension<SessionTokenContext>>,
    Json(req): Json<OpenAiCompletionsRequest>,
) -> Result<Json<OpenAiCompletionsResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
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
    if req.stream.unwrap_or(false) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "streaming is not supported for legacy completions",
                Some("STREAM_UNSUPPORTED".to_string()),
                Some("stream".to_string()),
            )),
        ));
    }
    validate_penalties_and_logprobs(req.frequency_penalty, req.presence_penalty, req.logprobs)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;

    let prompt =
        completion_prompt_to_text(&req.prompt).map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;
    let prompt_tokens_estimate = estimate_tokens(&prompt);

    let infer_req = InferRequest {
        prompt,
        model: req.model.clone(),
        max_tokens: req.max_tokens.map(|v| v as usize),
        temperature: Some(req.temperature.unwrap_or(0.0)),
        top_p: Some(req.top_p.unwrap_or(1.0)),
        seed: req.seed,
        stop_policy: stop_policy_from_openai(
            req.stop.as_ref(),
            req.max_tokens
                .map(|v| v as usize)
                .unwrap_or(DEFAULT_MAX_TOKENS),
        ),
        ..Default::default()
    };

    let infer_resp = match handlers::inference::infer(
        State(state),
        Extension(claims),
        Extension(client_ip),
        Extension(identity),
        request_id,
        api_key,
        session_token,
        Json(infer_req),
    )
    .await
    {
        Ok(Json(r)) => r,
        Err(api_error) => {
            let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = api_error.into();
            return Err((status, Json(map_adapteros_error_to_openai(status, err))));
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
        cached_prefix_tokens: None, // Fresh computation, not from cache
    });

    let receipt_digest_hex = infer_resp
        .run_receipt
        .as_ref()
        .map(|r| r.receipt_digest.to_hex());

    let id = match &infer_resp.run_receipt {
        Some(receipt) => {
            let hex = receipt.receipt_digest.to_hex();
            let prefix = &hex[..RECEIPT_DIGEST_ID_SUFFIX_LEN.min(hex.len())];
            format!("cmpl_{}", prefix)
        }
        None => format!("cmpl_{}", infer_resp.id),
    };

    let response = OpenAiCompletionsResponse {
        id,
        object: "text_completion".to_string(),
        created: Utc::now().timestamp(),
        model,
        system_fingerprint: receipt_digest_hex,
        choices: vec![OpenAiCompletionChoice {
            index: 0,
            text: infer_resp.text,
            finish_reason: map_finish_reason(infer_resp.stop_reason_code)
                .or_else(|| Some("stop".to_string())),
        }],
        usage,
    };

    Ok(Json(response))
}

/// OpenAI-compatible embeddings endpoint.
#[utoipa::path(
    post,
    path = "/v1/embeddings",
    request_body = OpenAiEmbeddingsRequest,
    responses(
        (status = 200, description = "Embedding response", body = OpenAiEmbeddingsResponse),
        (status = 400, description = "Bad request", body = OpenAiErrorResponse),
        (status = 500, description = "Internal server error", body = OpenAiErrorResponse)
    ),
    tag = "openai"
)]
pub async fn embeddings_openai(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<OpenAiEmbeddingsRequest>,
) -> Result<Json<OpenAiEmbeddingsResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )
    .map_err(|e| {
        let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = e.into();
        (status, Json(map_adapteros_error_to_openai(status, err)))
    })?;

    // Log user identifier if provided (for abuse detection)
    if let Some(ref user) = req.user {
        tracing::debug!(user = %user, "Embeddings request from user");
    }

    let embedding_model = state.embedding_model.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "Embedding model not configured",
                Some("EMBEDDINGS_DISABLED".to_string()),
                None,
            )),
        )
    })?;

    // Determine encoding format (default to float)
    let use_base64 = match req
        .encoding_format
        .as_ref()
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        None | Some("float") => false,
        Some("base64") => true,
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(openai_error(
                    format!(
                        "Invalid encoding_format: '{}'. Must be 'float' or 'base64'",
                        other
                    ),
                    Some("INVALID_ENCODING_FORMAT".to_string()),
                    Some("encoding_format".to_string()),
                )),
            ));
        }
    };

    // Dimensions parameter is not supported - return error if specified
    if let Some(dims) = req.dimensions {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                format!(
                    "Dimension reduction is not supported. Requested {} dimensions, but model returns {} dimensions",
                    dims,
                    embedding_model.dimension()
                ),
                Some("DIMENSIONS_NOT_SUPPORTED".to_string()),
                Some("dimensions".to_string()),
            )),
        ));
    }

    let inputs = embedding_inputs(&req.input).map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;

    let mut data = Vec::with_capacity(inputs.len());
    let mut prompt_tokens = 0usize;
    for (idx, text) in inputs.iter().enumerate() {
        let embedding_vec = embedding_model.encode_text(text).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(openai_error(
                    format!("Embedding generation failed: {}", e),
                    Some("EMBEDDING_ERROR".to_string()),
                    None,
                )),
            )
        })?;
        prompt_tokens = prompt_tokens.saturating_add(estimate_tokens(text));

        let embedding = if use_base64 {
            EmbeddingData::from_base64(embedding_vec)
        } else {
            EmbeddingData::from_float(embedding_vec)
        };

        data.push(OpenAiEmbeddingItem {
            object: "embedding".to_string(),
            index: idx,
            embedding,
        });
    }

    let usage = OpenAiEmbeddingUsage {
        prompt_tokens,
        total_tokens: prompt_tokens,
    };

    let model = req.model.unwrap_or_else(|| "adapteros-embed".to_string());

    Ok(Json(OpenAiEmbeddingsResponse {
        object: "list".to_string(),
        data,
        model,
        usage,
    }))
}

/// Streaming chat completions handler.
///
/// Returns SSE stream with OpenAI-compatible chunk format.
async fn chat_completions_streaming(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    session_token: Option<Extension<SessionTokenContext>>,
    req: OpenAiChatCompletionsRequest,
) -> Result<
    Sse<impl Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<OpenAiErrorResponse>),
> {
    // Check inference permission
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )
    .map_err(|e| {
        let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = e.into();
        (status, Json(map_adapteros_error_to_openai(status, err)))
    })?;

    // Convert messages to prompt and structured messages
    let (prompt, chat_messages) = messages_to_prompt_and_messages(&req.messages)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;

    // Validate prompt length
    if prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "Prompt cannot be empty",
                Some("EMPTY_PROMPT".to_string()),
                Some("messages".to_string()),
            )),
        ));
    }
    if prompt.len() > MAX_REPLAY_TEXT_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(openai_error(
                "Prompt too long for context window",
                Some("PROMPT_TOO_LONG".to_string()),
                Some("messages".to_string()),
            )),
        ));
    }

    // Check backpressure
    check_uma_backpressure(&state).map_err(|(status, Json(err))| {
        (status, Json(map_adapteros_error_to_openai(status, err)))
    })?;

    let session_lock = if let Some(token) = session_token.as_ref() {
        Some(
            resolve_session_token_lock(&state, &claims, &token.0.lock)
                .await
                .map_err(|err| {
                    let (status, Json(err)) = <(StatusCode, Json<ErrorResponse>)>::from(err);
                    (status, Json(map_adapteros_error_to_openai(status, err)))
                })?,
        )
    } else {
        None
    };

    // Generate request ID
    let request_id = crate::id_generator::readable_openai_chatcmpl_id();
    let model_name = req.model.clone().unwrap_or_else(|| "adapteros".to_string());
    let created = Utc::now().timestamp() as u64;

    // P2 HARDENING: Collect ALL policy decisions BEFORE creating envelope
    let mut all_policy_decisions = Vec::new();

    // Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnRequestBeforeRouting,
        "chat_completions",
        None, // No adapter selected yet
    );
    let routing_decisions =
        enforce_at_hook(&state, &routing_hook_ctx)
            .await
            .map_err(|violation| {
                (
                    StatusCode::FORBIDDEN,
                    Json(openai_error(
                        format!("Policy violation: {}", violation.message),
                        Some("POLICY_HOOK_VIOLATION".to_string()),
                        None,
                    )),
                )
            })?;
    all_policy_decisions.extend(routing_decisions);

    // Enforce policies at OnBeforeInference hook
    let hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnBeforeInference,
        "chat_completions",
        None,
    );
    let inference_decisions = enforce_at_hook(&state, &hook_ctx)
        .await
        .map_err(|violation| {
            (
                StatusCode::FORBIDDEN,
                Json(openai_error(
                    format!("Policy violation: {}", violation.message),
                    Some("POLICY_HOOK_VIOLATION".to_string()),
                    None,
                )),
            )
        })?;
    all_policy_decisions.extend(inference_decisions);

    // P2 HARDENING: Compute policy digest BEFORE creating envelope
    let policy_digest = compute_policy_mask_digest(&all_policy_decisions);

    // Create envelope WITH pre-computed policy digest
    let mut run_envelope = new_run_envelope(&state, &claims, request_id.clone(), false);
    crate::types::run_envelope::set_policy_mask(&mut run_envelope, Some(&policy_digest));

    // Audit log: inference execution start
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&request_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    info!(
        request_id = %request_id,
        prompt_len = prompt.len(),
        max_tokens = req.max_tokens.or(req.max_completion_tokens).unwrap_or(DEFAULT_MAX_TOKENS as u32),
        "Starting OpenAI-compatible streaming chat completion"
    );

    // Build internal inference request
    let max_tokens = req
        .max_tokens
        .or(req.max_completion_tokens)
        .unwrap_or(DEFAULT_MAX_TOKENS as u32) as usize;
    let temperature = req.temperature.unwrap_or(0.0);
    let top_p = req.top_p.unwrap_or(1.0);

    let is_admin = claims.role.eq_ignore_ascii_case("admin")
        || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let mut internal_request = InferenceRequestInternal {
        request_id: request_id.clone(),
        cpid: claims.tenant_id.clone(),
        prompt,
        messages: Some(chat_messages),
        run_envelope: Some(run_envelope.clone()),
        admin_override: is_admin,
        stream: true,
        require_step: true,
        max_tokens,
        temperature,
        top_p: Some(top_p),
        seed: req.seed,
        stop_policy: stop_policy_from_openai(req.stop.as_ref(), max_tokens),
        fusion_interval: None,
        claims: Some(claims.clone()),
        model: req.model.clone(),
        created_at: std::time::Instant::now(),
        ..InferenceRequestInternal::default()
    };
    if let Some(lock) = session_lock.as_ref() {
        internal_request.adapter_stack = None;
        internal_request.adapters = Some(lock.adapter_ids.clone());
        internal_request.effective_adapter_ids = Some(lock.adapter_ids.clone());
        internal_request.stack_id = lock.stack_id.clone();
        internal_request.pinned_adapter_ids = Some(lock.pinned_adapter_ids.clone());
        if let Some(backend) = lock.backend_profile {
            internal_request.backend_profile = Some(backend);
            internal_request.allow_fallback = backend == adapteros_core::BackendKind::Auto;
        }
        if let Some(coreml_mode) = lock.coreml_mode {
            internal_request.coreml_mode = Some(coreml_mode);
        }
    }

    // Verify worker is available
    let core = InferenceCore::new(&state);
    let _worker = core
        .select_worker_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = e.into();
            (status, Json(map_adapteros_error_to_openai(status, err)))
        })?;

    // Get streaming config
    let stream_config = state
        .config
        .read()
        .unwrap_or_else(|e| {
            warn!("Config lock poisoned in chat_completions_streaming, recovering");
            e.into_inner()
        })
        .streaming
        .clone();

    // Create cancellation token for client disconnect detection
    let cancellation_token = CancellationToken::new();
    let drop_guard = StreamDropGuard {
        cancellation_token: cancellation_token.clone(),
        request_id: request_id.clone(),
    };

    // Spawn the streaming inference
    let (token_rx, done_rx) = spawn_streaming_inference(
        state.clone(),
        internal_request,
        cancellation_token,
        stream_config.inference_token_buffer_capacity,
    );

    // Build the SSE stream with cancellation support
    let stream = build_openai_sse_stream(
        request_id,
        model_name,
        created,
        token_rx,
        done_rx,
        run_envelope,
        drop_guard,
    );

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Spawn streaming inference task.
fn spawn_streaming_inference(
    state: AppState,
    request: InferenceRequestInternal,
    cancellation_token: CancellationToken,
    token_buffer_capacity: usize,
) -> (
    mpsc::Receiver<WorkerStreamToken>,
    oneshot::Receiver<Result<crate::types::InferenceResult, crate::types::InferenceError>>,
) {
    let (token_tx, token_rx) = mpsc::channel(token_buffer_capacity);
    let (done_tx, done_rx) = oneshot::channel();

    tokio::spawn(async move {
        let core = InferenceCore::new(&state);
        let result = core
            .route_and_infer_stream(request, None, Some(cancellation_token), token_tx, None)
            .await;
        let _ = done_tx.send(result);
    });

    (token_rx, done_rx)
}

/// Build OpenAI-compatible SSE stream from token channel.
///
/// The `drop_guard` is kept alive for the duration of the stream. When the client
/// disconnects (stream is dropped), the guard triggers the cancellation token,
/// which signals the inference task to abort.
fn build_openai_sse_stream(
    request_id: String,
    model_name: String,
    created: u64,
    token_rx: mpsc::Receiver<WorkerStreamToken>,
    done_rx: oneshot::Receiver<Result<crate::types::InferenceResult, crate::types::InferenceError>>,
    run_envelope: adapteros_api_types::RunEnvelope,
    drop_guard: StreamDropGuard,
) -> impl Stream<Item = Result<Event, Infallible>> {
    // State for the stream
    struct StreamState {
        request_id: String,
        model_name: String,
        created: u64,
        token_rx: mpsc::Receiver<WorkerStreamToken>,
        done_rx: Option<
            oneshot::Receiver<Result<crate::types::InferenceResult, crate::types::InferenceError>>,
        >,
        sent_role: bool,
        finished: bool,
        // Retained for future evidence/audit integration in streaming responses
        #[allow(dead_code)]
        run_envelope: adapteros_api_types::RunEnvelope,
        // Keep the drop guard alive; when dropped, it cancels inference
        #[allow(dead_code)]
        drop_guard: StreamDropGuard,
    }

    let state = StreamState {
        request_id,
        model_name,
        created,
        token_rx,
        done_rx: Some(done_rx),
        sent_role: false,
        finished: false,
        run_envelope,
        drop_guard,
    };

    stream::unfold(state, |mut state| async move {
        if state.finished {
            return None;
        }

        // First chunk: send role
        if !state.sent_role {
            state.sent_role = true;
            let chunk = StreamingChunk {
                id: state.request_id.clone(),
                object: "chat.completion.chunk".to_string(),
                created: state.created,
                model: state.model_name.clone(),
                system_fingerprint: None,
                choices: vec![StreamingChoice {
                    index: 0,
                    delta: Delta {
                        role: Some("assistant".to_string()),
                        content: None,
                    },
                    finish_reason: None,
                    stop_reason_code: None,
                    stop_reason_token_index: None,
                    stop_policy_digest_b3: None,
                }],
            };
            let json = serde_json::to_string(&chunk).unwrap_or_default();
            let event = Event::default().data(json);
            return Some((Ok(event), state));
        }

        // Try to receive tokens
        tokio::select! {
            biased;

            token = state.token_rx.recv() => {
                match token {
                    Some(token) => {
                        let chunk = StreamingChunk {
                            id: state.request_id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            created: state.created,
                            model: state.model_name.clone(),
                            system_fingerprint: None,
                            choices: vec![StreamingChoice {
                                index: 0,
                                delta: Delta {
                                    role: None,
                                    content: Some(token.text),
                                },
                                finish_reason: None,
                                stop_reason_code: None,
                                stop_reason_token_index: None,
                                stop_policy_digest_b3: None,
                            }],
                        };
                        let json = serde_json::to_string(&chunk).unwrap_or_default();
                        let event = Event::default().data(json);
                        Some((Ok(event), state))
                    }
                    None => {
                        // Token channel closed, wait for done signal and send final chunk
                        let result = if let Some(done_rx) = state.done_rx.take() {
                            done_rx.await.ok()
                        } else {
                            None
                        };

                        // Determine finish reason and system fingerprint from result
                        // Note: All chunks use the same request_id for OpenAI compatibility.
                        // The receipt_digest is exposed via system_fingerprint on the final chunk.
                        let (finish_reason, system_fingerprint) = match result {
                            Some(Ok(ref res)) => {
                                let reason = map_finish_reason(res.stop_reason_code)
                                    .unwrap_or_else(|| "stop".to_string());
                                // Use receipt_digest for system_fingerprint (full 64-char hex)
                                let fp = res.run_receipt.as_ref()
                                    .map(|r| r.receipt_digest.to_hex());
                                (reason, fp)
                            }
                            Some(Err(_)) => ("error".to_string(), None),
                            None => ("stop".to_string(), None),
                        };

                        // Send final chunk with finish_reason
                        // All chunks share the same request_id for OpenAI compatibility
                        let final_chunk = StreamingChunk {
                            id: state.request_id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            created: state.created,
                            model: state.model_name.clone(),
                            system_fingerprint,
                            choices: vec![StreamingChoice {
                                index: 0,
                                delta: Delta {
                                    role: None,
                                    content: None,
                                },
                                finish_reason: Some(finish_reason),
                                stop_reason_code: result.as_ref().and_then(|r| r.as_ref().ok()).and_then(|r| r.stop_reason_code),
                                stop_reason_token_index: result.as_ref().and_then(|r| r.as_ref().ok()).and_then(|r| r.stop_reason_token_index),
                                stop_policy_digest_b3: result.as_ref().and_then(|r| r.as_ref().ok()).and_then(|r| r.stop_policy_digest_b3.clone()),
                            }],
                        };
                        let json = serde_json::to_string(&final_chunk).unwrap_or_default();
                        let event = Event::default().data(json);

                        // Mark as finished and return the final chunk
                        // Next iteration will send [DONE]
                        state.finished = true;
                        Some((Ok(event), state))
                    }
                }
            }
        }
    })
    .chain(stream::once(async {
        // Send [DONE] marker as final event
        Ok(Event::default().data("[DONE]"))
    }))
}

/// Guard that triggers cancellation when stream is dropped (client disconnect).
///
/// When the SSE response body is dropped (client disconnects), this guard
/// triggers the cancellation token, allowing in-flight inference to abort.
struct StreamDropGuard {
    cancellation_token: CancellationToken,
    request_id: String,
}

impl Drop for StreamDropGuard {
    fn drop(&mut self) {
        if !self.cancellation_token.is_cancelled() {
            info!(
                request_id = %self.request_id,
                "OpenAI stream dropped (client disconnect), cancelling inference"
            );
            self.cancellation_token.cancel();
        }
    }
}

fn map_adapteros_error_to_openai(status: StatusCode, err: ErrorResponse) -> OpenAiErrorResponse {
    let mut message = err.message;
    if let Some(details) = err.details {
        if let Ok(details_str) = serde_json::to_string(&details) {
            message = format!("{} ({})", message, details_str);
        }
    }
    openai_error_typed(
        classify_openai_error_type(status),
        message,
        Some(err.code),
        None,
    )
}

// ============================================================================
// OpenAI Models API Types
// ============================================================================

/// OpenAI-compatible model object.
///
/// See: <https://platform.openai.com/docs/api-reference/models/object>
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OpenAiModel {
    /// The model identifier.
    pub id: String,
    /// The object type, which is always "model".
    pub object: String,
    /// Unix timestamp (seconds) when the model was created.
    pub created: i64,
    /// The organization that owns the model.
    pub owned_by: String,
}

/// OpenAI-compatible model list response.
///
/// See: <https://platform.openai.com/docs/api-reference/models/list>
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OpenAiModelListResponse {
    /// The object type, which is always "list".
    pub object: String,
    /// The list of model objects.
    pub data: Vec<OpenAiModel>,
}

/// List all models in OpenAI-compatible format
///
/// # Endpoint
/// GET /v1/models
///
/// # Authentication
/// Required
///
/// # Response
/// Returns a list of models in OpenAI-compatible format:
/// - `object`: Always "list"
/// - `data`: Array of model objects with:
///   - `id`: Model identifier
///   - `object`: Always "model"
///   - `created`: Unix timestamp of when the model was imported
///   - `owned_by`: Organization/tenant that owns the model
///
/// # Errors
/// - `500`: Database error
///
/// # Example Response
/// ```json
/// {
///   "object": "list",
///   "data": [
///     {
///       "id": "Llama-3.2-3B-Instruct-4bit",
///       "object": "model",
///       "created": 1686935002,
///       "owned_by": "adapteros"
///     }
///   ]
/// }
/// ```
#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "List of models", body = OpenAiModelListResponse),
        (status = 500, description = "Database error")
    ),
    tag = "models"
)]
pub async fn list_models_openai(
    State(state): State<crate::state::AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
) -> Result<Json<OpenAiModelListResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
    let models_with_stats = state
        .db
        .list_models_with_stats(&claims.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list models: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(openai_error(
                    "Failed to list models",
                    Some("database_error".to_string()),
                    None,
                )),
            )
        })?;

    let data = models_with_stats
        .into_iter()
        .map(|m| {
            let model = &m.model;
            // Parse imported_at timestamp or use epoch
            let created = model
                .imported_at
                .as_ref()
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(0);

            OpenAiModel {
                id: model.id.clone(),
                object: "model".to_string(),
                created,
                owned_by: model
                    .tenant_id
                    .clone()
                    .unwrap_or_else(|| "adapteros".to_string()),
            }
        })
        .collect();

    Ok(Json(OpenAiModelListResponse {
        object: "list".to_string(),
        data,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_error_format_classifies_invalid_request() {
        let err = ErrorResponse::new("bad input").with_code("BAD_INPUT");
        let mapped = map_adapteros_error_to_openai(StatusCode::BAD_REQUEST, err);
        assert_eq!(mapped.error.error_type, "invalid_request_error");
        assert_eq!(mapped.error.code.as_deref(), Some("BAD_INPUT"));
    }

    #[test]
    fn openai_error_format_classifies_permission_failures() {
        let err = ErrorResponse::new("forbidden").with_code("FORBIDDEN");
        let mapped = map_adapteros_error_to_openai(StatusCode::FORBIDDEN, err);
        assert_eq!(mapped.error.error_type, "permission_error");
    }

    #[test]
    fn openai_error_format_classifies_rate_limits() {
        let err = ErrorResponse::new("too many requests").with_code("RATE_LIMIT");
        let mapped = map_adapteros_error_to_openai(StatusCode::TOO_MANY_REQUESTS, err);
        assert_eq!(mapped.error.error_type, "rate_limit_error");
    }

    #[test]
    fn openai_error_format_classifies_server_failures() {
        let err = ErrorResponse::new("internal").with_code("INTERNAL");
        let mapped = map_adapteros_error_to_openai(StatusCode::INTERNAL_SERVER_ERROR, err);
        assert_eq!(mapped.error.error_type, "server_error");
    }

    #[test]
    fn response_format_json_schema_enforces_required_fields() {
        let format = OpenAiResponseFormat {
            format_type: "json_schema".to_string(),
            json_schema: Some(OpenAiJsonSchemaFormat {
                name: "shape".to_string(),
                schema: json!({
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "status": { "type": "string" }
                    }
                }),
                strict: Some(true),
            }),
        };

        let result = validate_response_format_output(Some(&format), "{\"ok\":true}");
        assert!(result.is_err());
        let err = result.expect_err("schema mismatch error");
        assert_eq!(err.error.code.as_deref(), Some("RESPONSE_SCHEMA_MISMATCH"));
    }

    #[test]
    fn tools_request_shapes_tool_call_response() {
        let req = OpenAiChatCompletionsRequest {
            model: Some("adapteros".to_string()),
            messages: vec![OpenAiChatMessage {
                role: "user".to_string(),
                content: json!("call tool"),
            }],
            temperature: None,
            top_p: None,
            max_tokens: Some(32),
            max_completion_tokens: None,
            stream: Some(false),
            n: None,
            response_format: None,
            tools: Some(vec![OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiToolFunction {
                    name: "lookup".to_string(),
                    description: None,
                    parameters: None,
                },
            }]),
            tool_choice: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            logprobs: None,
        };

        let (message, finish_reason) = build_tool_call_response(&req, "{\"id\":1}".to_string());
        assert!(message.content.is_none());
        assert!(message.tool_calls.is_some());
        assert_eq!(finish_reason.as_deref(), Some("tool_calls"));
    }
}
