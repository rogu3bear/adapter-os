use crate::auth::Claims;
use crate::state::AppState;
use crate::types::{ErrorResponse, WorkerInferRequest};
use crate::uds_client::UdsClient;
use adapteros_api_types::openai::{
    ChatChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatUsage, ModelInfo,
    ModelsListResponse,
};
use adapteros_api_types::InferRequest;
use adapteros_lora_router::features::CodeFeatures;
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

    // Extract features from prompt for router decision
    let code_features = CodeFeatures::from_context(&prompt);
    let feature_vec = code_features.to_vector();

    // Get router scoring for debugging/telemetry
    let scoring_explanation = state.router.read().unwrap().explain_score(&feature_vec);

    // Log router analysis for this inference request
    tracing::info!(
        tenant_id = %claims.tenant_id,
        prompt_len = prompt.len(),
        language_score = scoring_explanation.language_score,
        framework_score = scoring_explanation.framework_score,
        "Router analyzed prompt for inference request"
    );

    let infer_req = InferRequest {
        prompt,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        top_k: None,
        top_p: None,
        seed: None,
        require_evidence: Some(true),
    };

    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!(
                "Failed to list workers (falling back to default UDS): {}",
                e
            );
            Vec::new()
        }
    };

    // Resolve UDS path: prefer registered worker; otherwise fall back to per-tenant default
    let uds_path_buf = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        // Fallback: honor env override or use /var/run/aos/<tenant>/aos.sock
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| format!("/var/run/aos/{}/aos.sock", claims.tenant_id));
        std::path::PathBuf::from(fallback)
    };
    let uds_path = uds_path_buf.as_path();
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    // Create adapter hints based on router scoring
    let adapter_hints = if scoring_explanation.language_score > 0.1 {
        Some(vec!["rust-code-v1".to_string()])
    } else if scoring_explanation.framework_score > 0.1 {
        Some(vec!["framework-specific-v1".to_string()])
    } else {
        Some(vec!["general-coding-v1".to_string()])
    };

    let router_features = Some(crate::types::RouterFeatureScores {
        language_score: scoring_explanation.language_score,
        framework_score: scoring_explanation.framework_score,
        symbol_hits_score: scoring_explanation.symbol_hits_score,
        path_tokens_score: scoring_explanation.path_tokens_score,
        prompt_verb_score: scoring_explanation.prompt_verb_score,
        total_score: scoring_explanation.total_score,
    });

    let worker_request = WorkerInferRequest {
        cpid: claims.tenant_id.clone(),
        prompt: infer_req.prompt.clone(),
        max_tokens: infer_req.max_tokens.unwrap_or(100),
        require_evidence: true,
        adapter_hints: None, // No pre-routing for OpenAI-compatible endpoint
        router_features: None,
    };

    // Record inference latency
    let start_time = std::time::Instant::now();
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
    let latency_secs = start_time.elapsed().as_secs_f64();

    // Record real inference latency metrics
    state.metrics_collector.record_inference_latency(
        &claims.tenant_id,
        "qwen2.5-7b", // adapter_id - could be dynamic based on actual adapter used
        latency_secs,
    );

    // Record tokens generated
    let tokens_generated = worker_response
        .text
        .as_ref()
        .map(|text| text.split_whitespace().count() as u64)
        .unwrap_or(0);
    if tokens_generated > 0 {
        state.metrics_collector.record_tokens_generated(
            &claims.tenant_id,
            "qwen2.5-7b",
            tokens_generated,
        );
    }

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
///
/// Returns models from database if available, otherwise returns default hardcoded list.
#[utoipa::path(
    get,
    path = "/v1/models",
    responses((status = 200, description = "Models list", body = ModelsListResponse)),
    tag = "openai"
)]
pub async fn list_models(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ModelsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    // Try to query database for models
    let db_models = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name, bms.status, bms.loaded_at 
         FROM base_model_status bms 
         JOIN models m ON bms.model_id = m.id 
         WHERE bms.tenant_id = ? 
         ORDER BY bms.updated_at DESC",
        tenant_id
    )
    .fetch_all(state.db.pool())
    .await;

    let mut model_list = Vec::new();

    match db_models {
        Ok(rows) if !rows.is_empty() => {
            // Use database models
            for row in rows {
                let model_id = row.model_id;
                let model_name = row.model_name;
                // Format as OpenAI-compatible model ID
                let openai_id =
                    format!("adapteros-{}", model_name.to_lowercase().replace(' ', "-"));

                let created_timestamp = row
                    .loaded_at
                    .and_then(|dt_str| {
                        chrono::DateTime::parse_from_rfc3339(&dt_str)
                            .ok()
                            .map(|dt| dt.timestamp() as u64)
                    })
                    .unwrap_or(1_704_067_200);

                model_list.push(ModelInfo {
                    id: openai_id,
                    object: "model".to_string(),
                    created: created_timestamp,
                    owned_by: "adapteros".to_string(),
                });
            }
        }
        _ => {
            // Fallback to hardcoded default model for OpenAI compatibility
            model_list.push(ModelInfo {
                id: "adapteros-qwen2.5-7b".to_string(),
                object: "model".to_string(),
                created: 1_704_067_200,
                owned_by: "adapteros".to_string(),
            });
        }
    }

    let response = ModelsListResponse {
        object: "list".to_string(),
        data: model_list,
    };

    Ok(Json(response))
}
