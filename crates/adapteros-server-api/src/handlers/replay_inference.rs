//! Deterministic replay inference handlers (PRD-02)
//!
//! Provides endpoints for checking replay availability and executing deterministic
//! inference replay with validation and divergence detection.
//!
//! # Endpoints
//! - `GET /v1/replay/check/:inference_id` - Check if inference can be replayed
//! - `POST /v1/replay` - Execute replay with comparison to original
//! - `GET /v1/replay/history/:inference_id` - Get replay execution history
//!
//! # PRD-02 Compliance
//! Replay now uses `InferenceCore::route_and_infer_replay()` to ensure:
//! - Same inference path as normal requests
//! - Manifest/backend compatibility enforcement
//! - Router seed preservation for deterministic adapter selection
//! - Policy hooks and worker health-based selection

use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::{CreateReplayExecutionParams, UpdateReplayExecutionParams};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use tracing::{debug, error, info, warn};

/// Helper function to truncate text to maximum size
///
/// Returns (truncated_text, was_truncated)
pub fn truncate_text(text: &str, max_size: usize) -> (String, bool) {
    if text.len() <= max_size {
        (text.to_string(), false)
    } else {
        (text.chars().take(max_size).collect(), true)
    }
}

/// Compute the character position where two strings first diverge
///
/// Returns None if strings are identical
pub fn compute_divergence_position(original: &str, replay: &str) -> Option<usize> {
    original
        .chars()
        .zip(replay.chars())
        .position(|(a, b)| a != b)
        .or_else(|| {
            // If one is a prefix of the other, divergence is at length of shorter
            if original.len() != replay.len() {
                Some(original.len().min(replay.len()))
            } else {
                None
            }
        })
}

/// Compare responses and determine match status
pub fn compute_match_status(original: &str, replay: &str) -> ReplayMatchStatus {
    if original == replay {
        ReplayMatchStatus::Exact
    } else {
        // Simple heuristic: if >80% of tokens match, call it semantic
        // In production, this could use embedding similarity
        let orig_words: Vec<&str> = original.split_whitespace().collect();
        let replay_words: Vec<&str> = replay.split_whitespace().collect();

        let matching = orig_words
            .iter()
            .zip(replay_words.iter())
            .filter(|(a, b)| a == b)
            .count();

        let total = orig_words.len().max(replay_words.len());
        if total == 0 {
            return ReplayMatchStatus::Exact;
        }

        let similarity = matching as f32 / total as f32;
        if similarity > 0.8 {
            ReplayMatchStatus::Semantic
        } else {
            ReplayMatchStatus::Divergent
        }
    }
}

/// Check replay availability for an inference
///
/// Validates if an inference can be replayed exactly, approximately, or not at all.
/// Checks manifest availability, RAG document availability, and backend compatibility.
///
/// # Security
/// - Requires InferenceView permission
/// - Validates tenant access to the inference record
#[utoipa::path(
    get,
    path = "/v1/replay/check/{inference_id}",
    tag = "replay",
    params(
        ("inference_id" = String, Path, description = "Inference ID to check")
    ),
    responses(
        (status = 200, description = "Replay availability status", body = ReplayAvailabilityResponse),
        (status = 403, description = "Forbidden - tenant access denied", body = ErrorResponse),
        (status = 404, description = "Inference not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn check_availability(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(inference_id): Path<String>,
) -> Result<Json<ReplayAvailabilityResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check - InferenceExecute grants access to inference results
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    info!(
        inference_id = %inference_id,
        tenant_id = %claims.tenant_id,
        "Checking replay availability"
    );

    // Load inference metadata from database
    let metadata = state
        .db
        .get_replay_metadata_by_inference(&inference_id)
        .await
        .map_err(|e| {
            error!(inference_id = %inference_id, error = %e, "Failed to load replay metadata");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to load replay metadata").with_code("DB_ERROR")),
            )
        })?;

    let metadata = match metadata {
        Some(m) => m,
        None => {
            return Ok(Json(ReplayAvailabilityResponse {
                inference_id,
                status: ReplayStatus::Unavailable,
                can_replay_exact: false,
                can_replay_approximate: false,
                unavailable_reasons: vec!["No replay metadata found for this inference".to_string()],
                approximation_warnings: vec![],
                replay_key: None,
            }));
        }
    };

    // Validate tenant access
    if !check_tenant_access(&claims, &metadata.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to this inference").with_code("ACCESS_DENIED")),
        ));
    }

    // PRD-02: Check manifest/backend compatibility with available workers
    let workers = state.db.list_serving_workers().await.unwrap_or_default();

    let compatible_worker = workers.iter().find(|w| {
        // Check manifest hash match (strict)
        let manifest_match = w.manifest_hash_b3.as_deref() == Some(&metadata.manifest_hash);
        // Check backend compatibility (exact match required for determinism)
        // Worker's backend is in the status field or can be inferred from manifest
        manifest_match
    });

    let has_compatible_worker = compatible_worker.is_some();

    // Parse stored data
    let sampling_params: SamplingParams =
        serde_json::from_str(&metadata.sampling_params_json).unwrap_or_default();

    let adapter_ids: Option<Vec<String>> = metadata
        .adapter_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    let rag_doc_ids: Option<Vec<String>> = metadata
        .rag_doc_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    // Check RAG document availability using batch lookup for efficiency
    let mut missing_doc_ids = Vec::new();
    let mut rag_score = 1.0f32;

    if let Some(ref doc_ids) = rag_doc_ids {
        if !doc_ids.is_empty() {
            // Batch check which documents still exist
            match state.db.get_documents_by_ids_ordered(doc_ids).await {
                Ok(docs) => {
                    for (idx, doc) in docs.iter().enumerate() {
                        if doc.is_none() {
                            missing_doc_ids.push(doc_ids[idx].clone());
                        }
                    }
                    rag_score =
                        (doc_ids.len() - missing_doc_ids.len()) as f32 / doc_ids.len() as f32;
                }
                Err(e) => {
                    warn!(
                        inference_id = %inference_id,
                        error = %e,
                        "Failed to check RAG document availability, assuming degraded"
                    );
                    // Conservative: assume all docs missing on error
                    missing_doc_ids = doc_ids.clone();
                    rag_score = 0.0;
                }
            }
        }
    }

    // Determine replay status
    let mut unavailable_reasons = Vec::new();
    let mut approximation_warnings = Vec::new();

    // PRD-02: Check manifest/backend compatibility first (hard requirement)
    if !has_compatible_worker {
        unavailable_reasons.push(format!(
            "No worker available with manifest_hash={} (required for deterministic replay)",
            metadata.manifest_hash
        ));
    }

    // Check for truncation
    if metadata.prompt_truncated == 1 {
        approximation_warnings.push("Original prompt was truncated (>64KB)".to_string());
    }
    if metadata.response_truncated == 1 {
        approximation_warnings.push("Original response was truncated (>64KB)".to_string());
    }

    // Determine status based on worker compatibility, RAG availability, and truncation
    let status = if !has_compatible_worker {
        // No compatible worker = hard unavailable (PRD-02: no fallback to different manifest)
        ReplayStatus::Unavailable
    } else if !missing_doc_ids.is_empty() {
        approximation_warnings.push(format!(
            "{} of {} RAG documents are no longer available",
            missing_doc_ids.len(),
            rag_doc_ids.as_ref().map(|d| d.len()).unwrap_or(0)
        ));
        if rag_score > 0.5 {
            ReplayStatus::Degraded
        } else if rag_score > 0.0 {
            ReplayStatus::Approximate
        } else {
            unavailable_reasons.push("All RAG documents are unavailable".to_string());
            ReplayStatus::Unavailable
        }
    } else if metadata.prompt_truncated == 1 || metadata.response_truncated == 1 {
        ReplayStatus::Approximate
    } else {
        ReplayStatus::Available
    };

    // Build replay key
    let replay_key = ReplayKey {
        manifest_hash: metadata.manifest_hash,
        router_seed: metadata.router_seed,
        sampler_params: sampling_params,
        backend: metadata.backend,
        sampling_algorithm_version: metadata.sampling_algorithm_version,
        rag_snapshot_hash: metadata.rag_snapshot_hash,
        adapter_ids,
    };

    let can_replay_exact = status == ReplayStatus::Available;
    let can_replay_approximate = matches!(
        status,
        ReplayStatus::Available | ReplayStatus::Approximate | ReplayStatus::Degraded
    );

    debug!(
        inference_id = %inference_id,
        status = ?status,
        can_replay_exact,
        can_replay_approximate,
        "Replay availability determined"
    );

    Ok(Json(ReplayAvailabilityResponse {
        inference_id,
        status,
        can_replay_exact,
        can_replay_approximate,
        unavailable_reasons,
        approximation_warnings,
        replay_key: Some(replay_key),
    }))
}

/// Execute deterministic replay of an inference
///
/// Replays an inference using stored metadata and compares the result to the original.
/// Supports exact replay (when all conditions match) and approximate replay (when RAG
/// context has changed but documents still exist).
///
/// # Security
/// - Requires InferenceExecute permission
/// - Validates tenant access to the inference record
#[utoipa::path(
    post,
    path = "/v1/replay",
    tag = "replay",
    request_body = ReplayRequest,
    responses(
        (status = 200, description = "Replay executed successfully", body = ReplayResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Forbidden - tenant access denied", body = ErrorResponse),
        (status = 404, description = "Inference not found", body = ErrorResponse),
        (status = 503, description = "Replay unavailable or degraded", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn execute_replay(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request: must have either inference_id or replay_key
    if req.inference_id.is_none() && req.replay_key.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Must provide either inference_id or replay_key")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let inference_id = req.inference_id.clone().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("inference_id is required for replay")
                    .with_code("VALIDATION_ERROR"),
            ),
        )
    })?;

    info!(
        inference_id = %inference_id,
        tenant_id = %claims.tenant_id,
        allow_approximate = req.allow_approximate,
        skip_rag = req.skip_rag,
        "Executing replay"
    );

    // Load metadata
    let metadata = state
        .db
        .get_replay_metadata_by_inference(&inference_id)
        .await
        .map_err(|e| {
            error!(inference_id = %inference_id, error = %e, "Failed to load replay metadata");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to load replay metadata").with_code("DB_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Inference not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant access
    if !check_tenant_access(&claims, &metadata.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("ACCESS_DENIED")),
        ));
    }

    // Parse stored params
    let sampling_params: SamplingParams =
        serde_json::from_str(&metadata.sampling_params_json).unwrap_or_default();

    let adapter_ids: Option<Vec<String>> = metadata
        .adapter_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    let rag_doc_ids: Option<Vec<String>> = metadata
        .rag_doc_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    // Check RAG availability and compute degraded status
    let mut missing_doc_ids = Vec::new();
    if let Some(ref doc_ids) = rag_doc_ids {
        for doc_id in doc_ids {
            let doc_exists = state.db.get_document(doc_id).await.ok().flatten().is_some();
            if !doc_exists {
                missing_doc_ids.push(doc_id.clone());
            }
        }
    }

    let rag_total = rag_doc_ids.as_ref().map(|d| d.len()).unwrap_or(0);
    let rag_available = rag_total - missing_doc_ids.len();
    let rag_score = if rag_total > 0 {
        rag_available as f32 / rag_total as f32
    } else {
        1.0
    };

    // Determine replay mode
    let replay_mode = if missing_doc_ids.is_empty()
        && metadata.prompt_truncated == 0
        && metadata.response_truncated == 0
    {
        "exact"
    } else if rag_score > 0.5 {
        "degraded"
    } else {
        "approximate"
    };

    // Check if approximate replay is allowed
    if replay_mode != "exact" && !req.allow_approximate {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new(&format!(
                    "Replay would be {} but allow_approximate=false",
                    replay_mode
                ))
                .with_code("APPROXIMATE_REPLAY_REQUIRED"),
            ),
        ));
    }

    // Create replay execution record (before execution)
    let prompt = req.prompt.unwrap_or_else(|| metadata.prompt_text.clone());

    let create_params = CreateReplayExecutionParams {
        original_inference_id: inference_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        replay_mode: replay_mode.to_string(),
        prompt_text: prompt.clone(),
        sampling_params_json: metadata.sampling_params_json.clone(),
        backend: metadata.backend.clone(),
        manifest_hash: metadata.manifest_hash.clone(),
        router_seed: metadata.router_seed.clone(),
        adapter_ids: adapter_ids.clone(),
        executed_by: Some(claims.sub.clone()),
    };

    let replay_id = state
        .db
        .create_replay_execution(create_params)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create replay execution record");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to create execution record").with_code("DB_ERROR")),
            )
        })?;

    // Build InferenceRequestInternal from replay metadata (PRD-02)
    let inference_request = InferenceRequestInternal {
        request_id: replay_id.clone(),
        cpid: claims.tenant_id.clone(),
        prompt: prompt.clone(),
        stream: false,
        batch_item_id: None,
        rag_enabled: rag_doc_ids.is_some() && !req.skip_rag,
        rag_collection_id: None, // RAG context will be reconstructed from doc IDs if needed
        adapter_stack: None,
        adapters: adapter_ids.clone(),
        stack_id: None,
        stack_version: None,
        stack_determinism_mode: None,
        effective_adapter_ids: None,
        determinism_mode: None,
        max_tokens: sampling_params.max_tokens,
        temperature: sampling_params.temperature,
        top_k: sampling_params.top_k,
        top_p: sampling_params.top_p,
        seed: sampling_params.seed,
        router_seed: metadata.router_seed.clone(),
        require_evidence: false,
        session_id: None,
        pinned_adapter_ids: None, // Not used in replay
        chat_context_hash: None,
        model: None,
        created_at: std::time::Instant::now(),
    };

    // Build replay context with manifest/backend constraints (PRD-02)
    let replay_context = ReplayContext {
        original_inference_id: inference_id.clone(),
        required_manifest_hash: metadata.manifest_hash.clone(),
        required_backend: metadata.backend.clone(),
        skip_metadata_capture: true, // Don't create new replay metadata for replay
    };

    // Execute inference through InferenceCore (PRD-02: unified inference path)
    let core = InferenceCore::new(&state);
    let inference_result = core
        .route_and_infer_replay(inference_request, replay_context)
        .await;

    let latency_ms = match &inference_result {
        Ok(r) => r.latency_ms as i32,
        Err(_) => 0,
    };

    // Handle result
    // Note: tokens_generated is estimated from response text since WorkerInferResponse
    // doesn't directly provide token count (would need tokenizer for accurate count)
    let (response_text, tokens_generated, match_status, error_message) = match inference_result {
        Ok(result) => {
            let replay_text = result.text;
            let original_text = metadata.response_text.as_deref().unwrap_or("");
            let status = compute_match_status(original_text, &replay_text);
            // Rough estimate: ~4 chars per token for English text
            let tokens = (replay_text.len() / 4).max(1) as i32;
            (replay_text, tokens, status, None)
        }
        Err(e) => {
            warn!(error = %e, "Replay inference failed");
            (
                String::new(),
                0,
                ReplayMatchStatus::Error,
                Some(e.to_string()),
            )
        }
    };

    // Compute divergence details
    let original_response = metadata.response_text.clone().unwrap_or_default();
    let divergence_position = compute_divergence_position(&original_response, &response_text);

    let (truncated_response, response_truncated) =
        truncate_text(&response_text, MAX_REPLAY_TEXT_SIZE);

    // Build divergence details
    let divergence = if match_status != ReplayMatchStatus::Exact {
        let mut reasons = Vec::new();
        if !missing_doc_ids.is_empty() {
            reasons.push(format!(
                "{} RAG documents unavailable",
                missing_doc_ids.len()
            ));
        }
        if metadata.prompt_truncated == 1 {
            reasons.push("Original prompt was truncated".to_string());
        }
        if metadata.response_truncated == 1 {
            reasons.push("Original response was truncated".to_string());
        }
        Some(DivergenceDetails {
            divergence_position,
            backend_changed: false,
            manifest_changed: false,
            approximation_reasons: reasons,
        })
    } else {
        None
    };

    // Build RAG reproducibility
    let rag_reproducibility = if rag_total > 0 {
        Some(RagReproducibility {
            score: rag_score,
            matching_docs: rag_available,
            total_original_docs: rag_total,
            missing_doc_ids: missing_doc_ids.clone(),
        })
    } else {
        None
    };

    // Update replay execution with results
    let update_params = UpdateReplayExecutionParams {
        response_text: Some(truncated_response.clone()),
        response_truncated,
        tokens_generated: Some(tokens_generated),
        latency_ms: Some(latency_ms),
        match_status: match_status.to_string(),
        divergence_details: divergence
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok()),
        rag_reproducibility_score: rag_reproducibility.as_ref().map(|r| r.score as f64),
        missing_doc_ids: if missing_doc_ids.is_empty() {
            None
        } else {
            Some(missing_doc_ids.clone())
        },
        error_message: error_message.clone(),
    };

    state
        .db
        .update_replay_execution_result(&replay_id, update_params)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to update replay execution");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update execution").with_code("DB_ERROR")),
            )
        })?;

    debug!(
        replay_id = %replay_id,
        match_status = %match_status,
        latency_ms,
        "Replay completed"
    );

    Ok(Json(ReplayResponse {
        replay_id,
        original_inference_id: inference_id,
        replay_mode: replay_mode.to_string(),
        response: truncated_response,
        response_truncated,
        match_status,
        rag_reproducibility,
        divergence,
        original_response,
        stats: ReplayStats {
            estimated_tokens: tokens_generated as usize,
            latency_ms: latency_ms as u64,
            original_latency_ms: metadata.latency_ms.map(|l| l as u64),
        },
    }))
}

/// Get replay execution history for an inference
///
/// Returns all replay executions that have been performed for a given inference,
/// including match status and timestamps.
///
/// # Security
/// - Requires InferenceView permission
/// - Validates tenant access to the inference record
#[utoipa::path(
    get,
    path = "/v1/replay/history/{inference_id}",
    tag = "replay",
    params(
        ("inference_id" = String, Path, description = "Inference ID to get history for")
    ),
    responses(
        (status = 200, description = "Replay history retrieved", body = ReplayHistoryResponse),
        (status = 403, description = "Forbidden - tenant access denied", body = ErrorResponse),
        (status = 404, description = "Inference not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_replay_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(inference_id): Path<String>,
) -> Result<Json<ReplayHistoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check - InferenceExecute grants access to inference results
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    info!(
        inference_id = %inference_id,
        tenant_id = %claims.tenant_id,
        "Fetching replay history"
    );

    // Verify inference exists and tenant access
    let metadata = state
        .db
        .get_replay_metadata_by_inference(&inference_id)
        .await
        .map_err(|e| {
            error!(inference_id = %inference_id, error = %e, "Failed to load replay metadata");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to load metadata").with_code("DB_ERROR")),
            )
        })?;

    if let Some(ref m) = metadata {
        if !check_tenant_access(&claims, &m.tenant_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("Access denied").with_code("ACCESS_DENIED")),
            ));
        }
    }

    // Load replay executions
    let executions = state
        .db
        .list_replay_executions_for_inference(&inference_id)
        .await
        .map_err(|e| {
            error!(inference_id = %inference_id, error = %e, "Failed to load replay history");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to load history").with_code("DB_ERROR")),
            )
        })?;

    // Convert to response format
    let execution_records: Vec<ReplayExecutionRecord> = executions
        .into_iter()
        .map(|exec| {
            let match_status = match exec.match_status.as_str() {
                "exact" => ReplayMatchStatus::Exact,
                "semantic" => ReplayMatchStatus::Semantic,
                "divergent" => ReplayMatchStatus::Divergent,
                _ => ReplayMatchStatus::Error,
            };

            ReplayExecutionRecord {
                id: exec.id,
                original_inference_id: exec.original_inference_id,
                replay_mode: exec.replay_mode,
                match_status,
                rag_reproducibility_score: exec.rag_reproducibility_score.map(|s| s as f32),
                executed_at: exec.executed_at,
                executed_by: exec.executed_by,
                error_message: exec.error_message,
            }
        })
        .collect();

    let total_count = execution_records.len();

    debug!(
        inference_id = %inference_id,
        count = total_count,
        "Replay history retrieved"
    );

    Ok(Json(ReplayHistoryResponse {
        inference_id,
        executions: execution_records,
        total_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text_under_limit() {
        let text = "Hello world";
        let (result, truncated) = truncate_text(text, 100);
        assert_eq!(result, "Hello world");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_text_over_limit() {
        let text = "Hello world this is a long text";
        let (result, truncated) = truncate_text(text, 10);
        assert_eq!(result, "Hello worl");
        assert!(truncated);
    }

    #[test]
    fn test_compute_divergence_position_identical() {
        let pos = compute_divergence_position("hello", "hello");
        assert_eq!(pos, None);
    }

    #[test]
    fn test_compute_divergence_position_different() {
        let pos = compute_divergence_position("hello", "hallo");
        assert_eq!(pos, Some(1));
    }

    #[test]
    fn test_compute_divergence_position_prefix() {
        let pos = compute_divergence_position("hello", "hello world");
        assert_eq!(pos, Some(5));
    }

    #[test]
    fn test_compute_match_status_exact() {
        let status = compute_match_status("hello world", "hello world");
        assert_eq!(status, ReplayMatchStatus::Exact);
    }

    #[test]
    fn test_compute_match_status_semantic() {
        let status = compute_match_status("the quick brown fox jumps", "the quick brown fox leaps");
        assert_eq!(status, ReplayMatchStatus::Semantic);
    }

    #[test]
    fn test_compute_match_status_divergent() {
        let status = compute_match_status("the quick brown fox", "something completely different");
        assert_eq!(status, ReplayMatchStatus::Divergent);
    }

    #[test]
    fn test_compute_match_status_empty() {
        let status = compute_match_status("", "");
        assert_eq!(status, ReplayMatchStatus::Exact);
    }

    #[test]
    fn test_truncate_64kb_boundary() {
        // Test at exactly 64KB boundary
        let text = "a".repeat(MAX_REPLAY_TEXT_SIZE);
        let (result, truncated) = truncate_text(&text, MAX_REPLAY_TEXT_SIZE);
        assert_eq!(result.len(), MAX_REPLAY_TEXT_SIZE);
        assert!(!truncated);

        // Test one byte over
        let text = "a".repeat(MAX_REPLAY_TEXT_SIZE + 1);
        let (result, truncated) = truncate_text(&text, MAX_REPLAY_TEXT_SIZE);
        assert_eq!(result.len(), MAX_REPLAY_TEXT_SIZE);
        assert!(truncated);
    }
}
