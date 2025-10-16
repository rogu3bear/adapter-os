use crate::auth::Claims;
use crate::state::AppState;
use crate::types::{ErrorResponse, WorkerInferRequest};
use crate::uds_client::UdsClient;
use adapteros_api_types::{
    ChatChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatUsage,
    InferRequest, ModelInfo, ModelsListResponse,
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use uuid::Uuid;

/// OpenAI-compatible chat completions endpoint
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    request_body = ChatCompletionRequest,
    responses(
        (status = 200, description = "Chat completion successful", body = ChatCompletionResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 500, description = "Inference failed", body = ErrorResponse)
    ),
    tag = "openai"
)]
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.model.as_str() != "adapteros-qwen2.5-7b" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Unsupported model")
                    .with_code("INVALID_MODEL")
                    .with_string_details("Supported models: adapteros-qwen2.5-7b"),
            ),
        ));
    }

    let prompt = req
        .messages
        .iter()
        .map(|msg| match msg.role.as_str() {
            "system" => format!("System: {}", msg.content),
            "user" => format!("User: {}", msg.content),
            "assistant" => format!("Assistant: {}", msg.content),
            other => format!("{}: {}", other, msg.content),
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let infer_req = InferRequest {
        prompt,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        top_k: None,
        top_p: None,
        seed: None,
        require_evidence: Some(true),
    };

    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no workers available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details("No active workers found for inference"),
            ),
        ));
    }

    let worker = &workers[0];
    let uds_path = std::path::Path::new(&worker.uds_path);
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    let worker_request = WorkerInferRequest {
        cpid: claims.tenant_id.clone(),
        prompt: infer_req.prompt.clone(),
        max_tokens: infer_req.max_tokens.unwrap_or(100),
        require_evidence: true,
    };

    let worker_response = uds_client
        .infer(uds_path, worker_request)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("inference failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let finish_reason = worker_response.status.clone();
    let completion_text = worker_response.text.unwrap_or_default();

    let prompt_tokens = infer_req.prompt.split_whitespace().count();
    let completion_tokens = completion_text.split_whitespace().count();

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp() as u64,
        model: req.model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: completion_text,
            },
            finish_reason,
        }],
        usage: ChatUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    };

    Ok(Json(response))
}

/// OpenAI-compatible models list endpoint
#[utoipa::path(
    get,
    path = "/v1/models",
    responses((status = 200, description = "Models list", body = ModelsListResponse)),
    tag = "openai"
)]
pub async fn list_models() -> Result<Json<ModelsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let response = ModelsListResponse {
        object: "list".to_string(),
        data: vec![ModelInfo {
            id: "adapteros-qwen2.5-7b".to_string(),
            object: "model".to_string(),
            created: 1_704_067_200,
            owned_by: "adapteros".to_string(),
        }],
    };

    Ok(Json(response))
}
