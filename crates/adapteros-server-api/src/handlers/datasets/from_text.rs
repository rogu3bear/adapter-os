//! Handlers for creating datasets from raw text or chat messages.
//!
//! These endpoints enable the UI to create training datasets directly from:
//! - Pasted text content (from-text)
//! - Selected chat messages (from-chat)

use crate::api_error::ApiError;
use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::handlers::chunked_upload::FileValidator;
use crate::handlers::datasets::{
    bind_dataset_to_tenant, dataset_quota_limits, ensure_dirs, hash_file, quota_error,
    resolve_dataset_root, DatasetPaths, STREAM_BUFFER_SIZE,
};
use crate::ip_extraction::ClientIp;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::storage_usage::compute_tenant_storage_usage;
use crate::types::ErrorResponse;
use adapteros_api_types::training::{
    ChatDatasetFormat, ChatMessageInput, CreateDatasetFromChatRequest,
    CreateDatasetFromChatResponse, CreateDatasetFromTextRequest, CreateDatasetFromTextResponse,
    TextDatasetFormat, TextDatasetSourceType,
};
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde_json::json;
use tokio::fs;
use tracing::{info, warn};

/// Maximum JSONL file size (100MB)
const MAX_JSONL_SIZE: i64 = 100 * 1024 * 1024;

/// Create a training dataset from pasted or extracted text
///
/// Converts raw text content into JSONL training format based on the specified format.
/// Supports line-by-line processing, Q&A extraction, and raw text preservation.
///
/// The JSONL format depends on the `format` field:
/// - `lines`: Each non-empty line becomes `{"text": "<line>"}`
/// - `qa`: Attempts to parse Q:/A: pairs into `{"prompt": "...", "completion": "..."}`
/// - `raw`: Single entry with full content as `{"text": "<content>"}`
#[utoipa::path(
    post,
    path = "/v1/datasets/from-text",
    request_body = CreateDatasetFromTextRequest,
    responses(
        (status = 200, description = "Dataset created successfully", body = CreateDatasetFromTextResponse),
        (status = 400, description = "Invalid request - content is empty"),
        (status = 403, description = "Permission denied or quota exceeded"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_from_text(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(request): Json<CreateDatasetFromTextRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Input size limits
    const MAX_INPUT_CONTENT_SIZE: usize = 10 * 1024 * 1024; // 10MB

    // Validate input
    if request.content.trim().is_empty() {
        return Err(ApiError::bad_request("Content cannot be empty").into());
    }
    if request.content.len() > MAX_INPUT_CONTENT_SIZE {
        return Err(ApiError::bad_request("Content too large (max 10MB)").into());
    }

    let input_char_count = request.content.len();

    // Convert content to JSONL based on format
    let jsonl_lines = convert_text_to_jsonl(&request.content, request.format);

    if jsonl_lines.is_empty() {
        return Err(
            ApiError::bad_request("No training samples could be generated from content").into(),
        );
    }

    let sample_count = jsonl_lines.len();
    let dataset_name = resolve_text_dataset_name(
        request.name.clone(),
        request.source_type,
        request.format,
        sample_count,
    );

    // Build JSONL content
    let jsonl_content = jsonl_lines.join("\n");
    let content_bytes = jsonl_content.as_bytes();
    let file_size = content_bytes.len() as i64;

    if file_size > MAX_JSONL_SIZE {
        return Err(ApiError::bad_request(format!(
            "Generated dataset too large ({} bytes). Maximum allowed is {} bytes.",
            file_size, MAX_JSONL_SIZE
        ))
        .into());
    }

    let content_hash = hash_file(content_bytes);
    let dataset_root =
        resolve_dataset_root(&state).map_err(|e| ApiError::internal(e.to_string()))?;
    let dataset_paths = DatasetPaths::new(dataset_root);
    let allowed_roots = [dataset_paths.root().to_path_buf()];
    ensure_dirs([
        dataset_paths.files.as_path(),
        dataset_paths.temp.as_path(),
        dataset_paths.chunked.as_path(),
        dataset_paths.logs.as_path(),
    ])
    .await?;

    // Create dataset record
    let dataset_id = state
        .db
        .create_training_dataset(
            &dataset_name,
            request.description.as_deref(),
            "jsonl",
            &content_hash,
            "",
            Some(&claims.sub),
            None,
            Some("ready"),
            Some(&content_hash),
            None,
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to create dataset record: {}", e)))?;

    // Create directory for dataset
    let dataset_path = dataset_paths.dataset_dir(&claims.tenant_id, &dataset_id);
    if let Err(e) = ensure_dirs([dataset_path.as_path()]).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(e);
    }
    let dataset_path = canonicalize_strict_in_allowed_roots(&dataset_path, &allowed_roots)
        .map_err(|e| ApiError::internal(format!("Dataset path rejected: {}", e)))?;

    let file_name = "training.jsonl";
    let file_path = dataset_path.join(file_name);

    // Check quota
    let (soft_quota, hard_quota) = dataset_quota_limits();
    let usage = compute_tenant_storage_usage(&state, &claims.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to compute storage usage: {}", e)))?;
    let predicted_usage = usage.total_bytes().saturating_add(file_size as u64);
    if predicted_usage > hard_quota {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(quota_error(format!(
            "Dataset storage quota exceeded: {} > {} bytes",
            predicted_usage, hard_quota
        )));
    }
    if predicted_usage > soft_quota {
        warn!(
            tenant_id = %claims.tenant_id,
            predicted_usage,
            soft_quota,
            "Dataset storage soft quota exceeded"
        );
    }

    // Write JSONL file
    if let Err(e) = fs::write(&file_path, content_bytes).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::internal(format!("Failed to write dataset file: {}", e)).into());
    }

    // Update storage path
    if let Err(e) = state
        .db
        .update_dataset_storage_path(&dataset_id, &dataset_path.to_string_lossy())
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::db_error(format!("Failed to update storage path: {}", e)).into());
    }

    // Bind dataset to tenant
    if let Err(e) = bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(e);
    }

    // Add file record
    if let Err(e) = state
        .db
        .add_dataset_file(
            &dataset_id,
            file_name,
            &file_path.to_string_lossy(),
            file_size,
            &content_hash,
            Some("application/jsonl"),
        )
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::db_error(format!("Failed to add file record: {}", e)).into());
    }

    // Validate generated JSONL
    let validation_result =
        FileValidator::quick_validate(&file_path, "jsonl", STREAM_BUFFER_SIZE).await;
    let (validation_status, validation_errors) = match validation_result {
        Ok(()) => ("valid".to_string(), None),
        Err(e) => ("invalid".to_string(), Some(e.to_string())),
    };

    if let Err(e) = state
        .db
        .update_dataset_validation(
            &dataset_id,
            &validation_status,
            validation_errors.as_deref(),
            None,
        )
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(
            ApiError::db_error(format!("Failed to update validation status: {}", e)).into(),
        );
    }

    // Create initial dataset version
    let dataset_version_id = match state
        .db
        .create_training_dataset_version(
            &dataset_id,
            Some(&claims.tenant_id),
            None,
            &file_path.to_string_lossy(),
            &content_hash,
            None,
            None,
            Some(&claims.sub),
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            cleanup_dataset(&state, &dataset_id, &dataset_path).await;
            return Err(
                ApiError::db_error(format!("Failed to create dataset version: {}", e)).into(),
            );
        }
    };

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        actions::DATASET_CREATE,
        resources::DATASET,
        Some(&dataset_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        name = %dataset_name,
        samples = sample_count,
        format = %request.format,
        source_type = %request.source_type,
        "Created dataset from text"
    );

    Ok(Json(CreateDatasetFromTextResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        dataset_version_id: Some(dataset_version_id),
        name: dataset_name,
        sample_count,
        input_char_count,
        dataset_hash_b3: Some(content_hash),
        format: request.format,
        source_type: request.source_type,
        auto_generated: request.auto_generate,
    }))
}

/// Create a training dataset from selected chat messages
///
/// Converts chat messages into JSONL training format based on the specified format.
/// Supports multi-turn conversation format, instruction-response extraction, and raw preservation.
///
/// The JSONL format depends on the `format` field:
/// - `conversation`: Multi-turn format with `{"messages": [{"role": "...", "content": "..."}]}`
/// - `instruction_response`: Extract user/assistant pairs as `{"prompt": "...", "completion": "..."}`
/// - `raw`: Each message as `{"role": "...", "content": "..."}`
#[utoipa::path(
    post,
    path = "/v1/datasets/from-chat",
    request_body = CreateDatasetFromChatRequest,
    responses(
        (status = 200, description = "Dataset created successfully", body = CreateDatasetFromChatResponse),
        (status = 400, description = "Invalid request - no messages provided"),
        (status = 403, description = "Permission denied or quota exceeded"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_from_chat(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(request): Json<CreateDatasetFromChatRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Input size limits
    const MAX_MESSAGES: usize = 10_000;
    const MAX_MESSAGE_CONTENT_SIZE: usize = 100_000;

    // Validate input
    if request.messages.is_empty() {
        return Err(ApiError::bad_request("At least one message is required").into());
    }
    if request.messages.len() > MAX_MESSAGES {
        return Err(
            ApiError::bad_request(format!("Too many messages (max {})", MAX_MESSAGES)).into(),
        );
    }

    // Check individual message sizes
    for (i, msg) in request.messages.iter().enumerate() {
        if msg.content.len() > MAX_MESSAGE_CONTENT_SIZE {
            return Err(ApiError::bad_request(format!(
                "Message {} content too large (max {} chars)",
                i, MAX_MESSAGE_CONTENT_SIZE
            ))
            .into());
        }
    }

    let message_count = request.messages.len();

    // Filter messages based on include_system_messages flag
    let filtered_messages: Vec<&ChatMessageInput> = if request.include_system_messages {
        request.messages.iter().collect()
    } else {
        request
            .messages
            .iter()
            .filter(|m| m.role.to_lowercase() != "system")
            .collect()
    };

    if filtered_messages.is_empty() {
        return Err(ApiError::bad_request(
            "No messages remaining after filtering (all were system messages)",
        )
        .into());
    }

    // Convert messages to JSONL based on format
    let (jsonl_lines, turn_count) = convert_chat_to_jsonl(
        &filtered_messages,
        request.format,
        request.include_system_messages,
    );

    if jsonl_lines.is_empty() {
        return Err(
            ApiError::bad_request("No training samples could be generated from messages").into(),
        );
    }

    let sample_count = jsonl_lines.len();
    let dataset_name = resolve_chat_dataset_name(
        request.name.clone(),
        request.format,
        message_count,
        turn_count,
    );

    // Build JSONL content
    let jsonl_content = jsonl_lines.join("\n");
    let content_bytes = jsonl_content.as_bytes();
    let file_size = content_bytes.len() as i64;

    if file_size > MAX_JSONL_SIZE {
        return Err(ApiError::bad_request(format!(
            "Generated dataset too large ({} bytes). Maximum allowed is {} bytes.",
            file_size, MAX_JSONL_SIZE
        ))
        .into());
    }

    let content_hash = hash_file(content_bytes);
    let dataset_root =
        resolve_dataset_root(&state).map_err(|e| ApiError::internal(e.to_string()))?;
    let dataset_paths = DatasetPaths::new(dataset_root);
    let allowed_roots = [dataset_paths.root().to_path_buf()];
    ensure_dirs([
        dataset_paths.files.as_path(),
        dataset_paths.temp.as_path(),
        dataset_paths.chunked.as_path(),
        dataset_paths.logs.as_path(),
    ])
    .await?;

    // Create dataset record
    let dataset_id = state
        .db
        .create_training_dataset(
            &dataset_name,
            request.description.as_deref(),
            "jsonl",
            &content_hash,
            "",
            Some(&claims.sub),
            None,
            Some("ready"),
            Some(&content_hash),
            None,
        )
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to create dataset record: {}", e)))?;

    // Create directory for dataset
    let dataset_path = dataset_paths.dataset_dir(&claims.tenant_id, &dataset_id);
    if let Err(e) = ensure_dirs([dataset_path.as_path()]).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(e);
    }
    let dataset_path = canonicalize_strict_in_allowed_roots(&dataset_path, &allowed_roots)
        .map_err(|e| ApiError::internal(format!("Dataset path rejected: {}", e)))?;

    let file_name = "training.jsonl";
    let file_path = dataset_path.join(file_name);

    // Check quota
    let (soft_quota, hard_quota) = dataset_quota_limits();
    let usage = compute_tenant_storage_usage(&state, &claims.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to compute storage usage: {}", e)))?;
    let predicted_usage = usage.total_bytes().saturating_add(file_size as u64);
    if predicted_usage > hard_quota {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(quota_error(format!(
            "Dataset storage quota exceeded: {} > {} bytes",
            predicted_usage, hard_quota
        )));
    }
    if predicted_usage > soft_quota {
        warn!(
            tenant_id = %claims.tenant_id,
            predicted_usage,
            soft_quota,
            "Dataset storage soft quota exceeded"
        );
    }

    // Write JSONL file
    if let Err(e) = fs::write(&file_path, content_bytes).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::internal(format!("Failed to write dataset file: {}", e)).into());
    }

    // Update storage path
    if let Err(e) = state
        .db
        .update_dataset_storage_path(&dataset_id, &dataset_path.to_string_lossy())
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::db_error(format!("Failed to update storage path: {}", e)).into());
    }

    // Bind dataset to tenant
    if let Err(e) = bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(e);
    }

    // Add file record
    if let Err(e) = state
        .db
        .add_dataset_file(
            &dataset_id,
            file_name,
            &file_path.to_string_lossy(),
            file_size,
            &content_hash,
            Some("application/jsonl"),
        )
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(ApiError::db_error(format!("Failed to add file record: {}", e)).into());
    }

    // Validate generated JSONL
    let validation_result =
        FileValidator::quick_validate(&file_path, "jsonl", STREAM_BUFFER_SIZE).await;
    let (validation_status, validation_errors) = match validation_result {
        Ok(()) => ("valid".to_string(), None),
        Err(e) => ("invalid".to_string(), Some(e.to_string())),
    };

    if let Err(e) = state
        .db
        .update_dataset_validation(
            &dataset_id,
            &validation_status,
            validation_errors.as_deref(),
            None,
        )
        .await
    {
        cleanup_dataset(&state, &dataset_id, &dataset_path).await;
        return Err(
            ApiError::db_error(format!("Failed to update validation status: {}", e)).into(),
        );
    }

    // Create initial dataset version
    let dataset_version_id = match state
        .db
        .create_training_dataset_version(
            &dataset_id,
            Some(&claims.tenant_id),
            None,
            &file_path.to_string_lossy(),
            &content_hash,
            None,
            None,
            Some(&claims.sub),
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            cleanup_dataset(&state, &dataset_id, &dataset_path).await;
            return Err(
                ApiError::db_error(format!("Failed to create dataset version: {}", e)).into(),
            );
        }
    };

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        actions::DATASET_CREATE,
        resources::DATASET,
        Some(&dataset_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    info!(
        dataset_id = %dataset_id,
        name = %dataset_name,
        samples = sample_count,
        messages = message_count,
        turns = turn_count,
        format = %request.format,
        session_id = ?request.session_id,
        "Created dataset from chat"
    );

    Ok(Json(CreateDatasetFromChatResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        dataset_version_id: Some(dataset_version_id),
        name: dataset_name,
        sample_count,
        message_count,
        turn_count,
        dataset_hash_b3: Some(content_hash),
        format: request.format,
        session_id: request.session_id,
    }))
}

/// Convert text content to JSONL lines based on format
fn convert_text_to_jsonl(content: &str, format: TextDatasetFormat) -> Vec<String> {
    match format {
        TextDatasetFormat::Lines => {
            // Each non-empty line becomes a training sample
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| json!({ "text": line.trim() }).to_string())
                .collect()
        }
        TextDatasetFormat::Qa => {
            // Try to parse Q:/A: pairs
            parse_qa_pairs(content)
        }
        TextDatasetFormat::Raw => {
            // Single entry with full content
            let trimmed = content.trim();
            if trimmed.is_empty() {
                vec![]
            } else {
                vec![json!({ "text": trimmed }).to_string()]
            }
        }
    }
}

/// Parse Q:/A: pairs from text content
fn parse_qa_pairs(content: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut current_question: Option<String> = None;
    let mut current_answer_lines: Vec<String> = Vec::new();
    let mut in_answer = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for question markers
        if trimmed.starts_with("Q:") || trimmed.starts_with("Question:") {
            // Save previous Q/A pair if exists
            if let Some(q) = current_question.take() {
                if !current_answer_lines.is_empty() {
                    let answer = current_answer_lines.join("\n").trim().to_string();
                    if !answer.is_empty() {
                        results.push(
                            json!({
                                "prompt": q,
                                "completion": answer
                            })
                            .to_string(),
                        );
                    }
                }
            }
            current_answer_lines.clear();
            in_answer = false;

            // Extract question text
            let q_text = trimmed
                .trim_start_matches("Q:")
                .trim_start_matches("Question:")
                .trim();
            if !q_text.is_empty() {
                current_question = Some(q_text.to_string());
            }
        } else if trimmed.starts_with("A:") || trimmed.starts_with("Answer:") {
            in_answer = true;
            // Extract answer text
            let a_text = trimmed
                .trim_start_matches("A:")
                .trim_start_matches("Answer:")
                .trim();
            if !a_text.is_empty() {
                current_answer_lines.push(a_text.to_string());
            }
        } else if in_answer && current_question.is_some() {
            // Continue collecting answer lines
            if !trimmed.is_empty() {
                current_answer_lines.push(trimmed.to_string());
            }
        }
    }

    // Save last Q/A pair
    if let Some(q) = current_question {
        if !current_answer_lines.is_empty() {
            let answer = current_answer_lines.join("\n").trim().to_string();
            if !answer.is_empty() {
                results.push(
                    json!({
                        "prompt": q,
                        "completion": answer
                    })
                    .to_string(),
                );
            }
        }
    }

    // If no Q/A pairs found, fall back to line-by-line
    if results.is_empty() {
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| json!({ "text": line.trim() }).to_string())
            .collect()
    } else {
        results
    }
}

/// Convert chat messages to JSONL lines based on format
/// Returns (jsonl_lines, turn_count)
fn convert_chat_to_jsonl(
    messages: &[&ChatMessageInput],
    format: ChatDatasetFormat,
    _include_system: bool,
) -> (Vec<String>, usize) {
    match format {
        ChatDatasetFormat::Conversation => {
            // Multi-turn conversation format
            let conversation_messages: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| {
                    json!({
                        "role": m.role.to_lowercase(),
                        "content": m.content
                    })
                })
                .collect();

            let turn_count = count_turns(messages);

            if conversation_messages.is_empty() {
                (vec![], 0)
            } else {
                (
                    vec![json!({ "messages": conversation_messages }).to_string()],
                    turn_count,
                )
            }
        }
        ChatDatasetFormat::InstructionResponse => {
            // Extract user/assistant pairs as prompt/completion
            let mut results = Vec::new();
            let mut i = 0;
            let mut turn_count = 0;

            while i < messages.len() {
                let msg = messages[i];
                let role = msg.role.to_lowercase();

                if role == "user" {
                    // Look for assistant response
                    if i + 1 < messages.len() {
                        let next_msg = messages[i + 1];
                        if next_msg.role.to_lowercase() == "assistant" {
                            results.push(
                                json!({
                                    "prompt": msg.content,
                                    "completion": next_msg.content
                                })
                                .to_string(),
                            );
                            turn_count += 1;
                            i += 2;
                            continue;
                        }
                    }
                }
                i += 1;
            }

            (results, turn_count)
        }
        ChatDatasetFormat::Raw => {
            // Each message as a separate entry
            let turn_count = count_turns(messages);
            let lines: Vec<String> = messages
                .iter()
                .map(|m| {
                    json!({
                        "role": m.role.to_lowercase(),
                        "content": m.content
                    })
                    .to_string()
                })
                .collect();

            (lines, turn_count)
        }
    }
}

/// Count conversation turns (user + assistant pairs)
fn count_turns(messages: &[&ChatMessageInput]) -> usize {
    let mut turns = 0;
    let mut i = 0;

    while i < messages.len() {
        let role = messages[i].role.to_lowercase();
        if role == "user"
            && i + 1 < messages.len()
            && messages[i + 1].role.to_lowercase() == "assistant"
        {
            turns += 1;
            i += 2;
            continue;
        }
        i += 1;
    }

    turns
}

fn resolve_text_dataset_name(
    _request_name: Option<String>,
    source_type: TextDatasetSourceType,
    format: TextDatasetFormat,
    sample_count: usize,
) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    format!(
        "{} - {} mode - {} samples - {}",
        text_dataset_source_label(source_type),
        text_dataset_format_label(format),
        sample_count,
        timestamp
    )
}

fn resolve_chat_dataset_name(
    _request_name: Option<String>,
    format: ChatDatasetFormat,
    message_count: usize,
    turn_count: usize,
) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    format!(
        "Chat dataset - {} mode - {} messages - {} turns - {}",
        chat_dataset_format_label(format),
        message_count,
        turn_count,
        timestamp
    )
}

fn text_dataset_source_label(source: TextDatasetSourceType) -> &'static str {
    match source {
        TextDatasetSourceType::Pasted => "Pasted text dataset",
        TextDatasetSourceType::Extracted => "Extracted text dataset",
        TextDatasetSourceType::External => "External text dataset",
    }
}

fn text_dataset_format_label(format: TextDatasetFormat) -> &'static str {
    match format {
        TextDatasetFormat::Lines => "line_by_line",
        TextDatasetFormat::Qa => "qa_pairs",
        TextDatasetFormat::Raw => "raw_text",
    }
}

fn chat_dataset_format_label(format: ChatDatasetFormat) -> &'static str {
    match format {
        ChatDatasetFormat::Conversation => "conversation",
        ChatDatasetFormat::InstructionResponse => "instruction_response",
        ChatDatasetFormat::Raw => "raw_messages",
    }
}

/// Clean up dataset on error
async fn cleanup_dataset(state: &AppState, dataset_id: &str, dataset_path: &std::path::Path) {
    // Delete from database (best effort)
    if let Err(e) = state.db.delete_training_dataset(dataset_id).await {
        warn!(
            dataset_id = %dataset_id,
            error = %e,
            "Failed to delete dataset record during cleanup"
        );
    }

    // Delete from filesystem (best effort)
    if tokio::fs::try_exists(dataset_path).await.unwrap_or(false) {
        if let Err(e) = tokio::fs::remove_dir_all(dataset_path).await {
            warn!(
                path = %dataset_path.display(),
                error = %e,
                "Failed to delete dataset directory during cleanup"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_name_is_derived_from_source_context() {
        let name = resolve_text_dataset_name(
            Some("Customer FAQ - qa_pairs".to_string()),
            TextDatasetSourceType::Pasted,
            TextDatasetFormat::Qa,
            24,
        );
        assert!(name.starts_with("Pasted text dataset - qa_pairs mode - 24 samples - "));
    }

    #[test]
    fn generic_text_name_is_derived_from_source_context() {
        let name = resolve_text_dataset_name(
            Some("pasted-text".to_string()),
            TextDatasetSourceType::Pasted,
            TextDatasetFormat::Lines,
            12,
        );
        assert!(name.starts_with("Pasted text dataset - line_by_line mode - 12 samples - "));
    }

    #[test]
    fn chat_name_is_derived_from_message_context() {
        let name = resolve_chat_dataset_name(
            Some("Support triage training chat".to_string()),
            ChatDatasetFormat::InstructionResponse,
            38,
            17,
        );
        assert!(name
            .starts_with("Chat dataset - instruction_response mode - 38 messages - 17 turns - "));
    }

    #[test]
    fn rewrites_generic_chat_name() {
        let name = resolve_chat_dataset_name(
            Some("chat-selection".to_string()),
            ChatDatasetFormat::Conversation,
            16,
            7,
        );
        assert!(name.starts_with("Chat dataset - conversation mode - 16 messages - 7 turns - "));
    }
}
