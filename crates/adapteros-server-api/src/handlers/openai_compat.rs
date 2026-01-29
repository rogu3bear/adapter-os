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
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::middleware::request_id::RequestId;
use crate::middleware::ApiKeyToken;
use crate::state::AppState;
use crate::types::run_envelope::new_run_envelope;
use crate::types::{
    ErrorResponse, InferRequest, InferenceRequestInternal, StopReasonCode, DEFAULT_MAX_TOKENS,
    MAX_REPLAY_TEXT_SIZE,
};
use crate::uds_client::WorkerStreamToken;
use adapteros_core::identity::IdentityEnvelope;
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
use tracing::{info, warn};

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
/// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"role":"assistant"}}]}
/// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"}}]}
/// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{},"finish_reason":"stop"}]}
/// data: [DONE]
/// ```
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
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

    // Branch based on streaming mode
    if req.stream.unwrap_or(false) {
        match chat_completions_streaming(State(state), Extension(claims), req).await {
            Ok(sse) => sse.into_response(),
            Err((status, Json(err))) => (status, Json(err)).into_response(),
        }
    } else {
        match chat_completions_non_streaming(
            State(state),
            Extension(claims),
            Extension(identity),
            request_id,
            api_key,
            req,
        )
        .await
        {
            Ok(json) => json.into_response(),
            Err((status, Json(err))) => (status, Json(err)).into_response(),
        }
    }
}

/// Non-streaming chat completions handler.
async fn chat_completions_non_streaming(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(identity): Extension<IdentityEnvelope>,
    request_id: Option<Extension<RequestId>>,
    api_key: Option<Extension<ApiKeyToken>>,
    req: OpenAiChatCompletionsRequest,
) -> Result<Json<OpenAiChatCompletionsResponse>, (StatusCode, Json<OpenAiErrorResponse>)> {
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
        Err(api_error) => {
            let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = api_error.into();
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

/// Streaming chat completions handler.
///
/// Returns SSE stream with OpenAI-compatible chunk format.
async fn chat_completions_streaming(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
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
        (status, Json(map_adapteros_error_to_openai(err)))
    })?;

    // Convert messages to prompt
    let prompt =
        messages_to_prompt(&req.messages).map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;

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
        (status, Json(map_adapteros_error_to_openai(err)))
    })?;

    // Generate request ID
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
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
    let temperature = req.temperature.unwrap_or(0.7);
    let top_p = req.top_p;

    let is_admin = claims.role.eq_ignore_ascii_case("admin")
        || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let internal_request = InferenceRequestInternal {
        request_id: request_id.clone(),
        cpid: claims.tenant_id.clone(),
        prompt,
        run_envelope: Some(run_envelope.clone()),
        reasoning_mode: false,
        admin_override: is_admin,
        stream: true,
        require_step: true,
        require_determinism: false,
        allow_fallback: true,
        batch_item_id: None,
        rag_enabled: false,
        rag_collection_id: None,
        dataset_version_id: None,
        adapter_stack: None,
        adapters: None,
        stack_id: None,
        stack_routing_determinism_mode: None,
        domain_hint: None,
        stack_version: None,
        stack_determinism_mode: None,
        effective_adapter_ids: None,
        adapter_strength_overrides: None,
        determinism_mode: None,
        routing_determinism_mode: None,
        seed_mode: None,
        request_seed: None,
        backend_profile: None,
        coreml_mode: None,
        max_tokens,
        temperature,
        top_k: None,
        top_p,
        seed: None,
        require_evidence: false,
        session_id: None,
        pinned_adapter_ids: None,
        chat_context_hash: None,
        claims: Some(claims.clone()),
        model: req.model.clone(),
        stop_policy: None,
        created_at: std::time::Instant::now(),
        router_seed: None,
        worker_auth_token: None,
        policy_mask_digest_b3: None,
        utf8_healing: None,
        abstention_threshold: None,
        citation_mode: None,
    };

    // Verify worker is available
    let core = InferenceCore::new(&state);
    let _worker = core
        .select_worker_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            let (status, Json(err)): (StatusCode, Json<ErrorResponse>) = e.into();
            (status, Json(map_adapteros_error_to_openai(err)))
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
            .route_and_infer_stream(request, None, Some(cancellation_token), token_tx)
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
                system_fingerprint: state.run_envelope.manifest_hash_b3.clone(),
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

                        // Determine finish reason and receipt digest from result
                        let (finish_reason, system_fingerprint) = match result {
                            Some(Ok(ref res)) => {
                                let reason = map_finish_reason(res.stop_reason_code)
                                    .unwrap_or_else(|| "stop".to_string());
                                // Use run_envelope manifest hash or deterministic receipt digest for system_fingerprint
                                let fp = res.run_envelope.as_ref()
                                    .and_then(|env| env.manifest_hash_b3.clone())
                                    .or_else(|| state.run_envelope.manifest_hash_b3.clone());
                                (reason, fp)
                            }
                            Some(Err(_)) => ("error".to_string(), None),
                            None => ("stop".to_string(), None),
                        };

                        // Send final chunk with finish_reason
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

fn map_adapteros_error_to_openai(err: ErrorResponse) -> OpenAiErrorResponse {
    let mut message = err.message;
    if let Some(details) = err.details {
        if let Ok(details_str) = serde_json::to_string(&details) {
            message = format!("{} ({})", message, details_str);
        }
    }
    openai_error(message, Some(err.code), None)
}
