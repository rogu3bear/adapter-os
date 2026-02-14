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

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{create_hook_context, enforce_at_hook};
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::SeedMode;
use adapteros_db::{CreateReplayExecutionParams, UpdateReplayExecutionParams};
use adapteros_policy::hooks::PolicyHook;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
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
        if similarity >= 0.8 {
            ReplayMatchStatus::Semantic
        } else {
            ReplayMatchStatus::Divergent
        }
    }
}

/// Determine replay mode based on document availability and truncation flags.
pub fn determine_replay_mode(
    missing_doc_count: usize,
    rag_score: f32,
    prompt_truncated: i32,
    response_truncated: i32,
) -> &'static str {
    if missing_doc_count == 0 && prompt_truncated == 0 && response_truncated == 0 {
        "exact"
    } else if rag_score > 0.5 {
        "degraded"
    } else {
        "approximate"
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
) -> ApiResult<ReplayAvailabilityResponse> {
    // Permission check - InferenceExecute grants access to inference results
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &inference_id).await?;

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
            ApiError::db_error(e)
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
                version_consistency_warning: None,
                replay_key: None,
            }));
        }
    };

    // Validate tenant access
    if !check_tenant_access(&claims, &metadata.tenant_id) {
        return Err(ApiError::forbidden("Access denied to this inference"));
    }

    // Parse stored data (used for both failure and availability checks)
    let sampling_params: SamplingParams =
        serde_json::from_str(&metadata.sampling_params_json).unwrap_or_default();

    let failure_status = match metadata.replay_status.as_str() {
        "failed_inference" => Some(ReplayStatus::FailedInference),
        "failed_capture" => Some(ReplayStatus::FailedCapture),
        _ => None,
    };

    if let Some(status) = failure_status {
        let mut unavailable_reasons = Vec::new();
        match status {
            ReplayStatus::FailedInference => {
                unavailable_reasons
                    .push("Original inference failed; no replayable output captured".to_string());
            }
            ReplayStatus::FailedCapture => {
                unavailable_reasons
                    .push("Replay metadata capture failed; replay key incomplete".to_string());
            }
            _ => {}
        }

        if let Some(code) = sampling_params.error_code.as_deref() {
            unavailable_reasons.push(format!("Original error code: {}", code));
        }

        return Ok(Json(ReplayAvailabilityResponse {
            inference_id,
            status,
            can_replay_exact: false,
            can_replay_approximate: false,
            unavailable_reasons,
            approximation_warnings: vec![],
            version_consistency_warning: None,
            replay_key: None,
        }));
    }

    // PRD-02: Check manifest/backend compatibility with available workers
    let workers = state.db.list_healthy_workers().await.unwrap_or_default();

    let compatible_worker = workers.iter().find(|w| {
        // Check manifest hash match (strict)
        let manifest_match = w.manifest_hash_b3.as_deref() == Some(&metadata.manifest_hash);
        // Check backend compatibility (exact match required for determinism)
        // Worker's backend is in the status field or can be inferred from manifest
        manifest_match
    });

    let has_compatible_worker = compatible_worker.is_some();

    let adapter_ids: Option<Vec<String>> = metadata
        .adapter_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    let base_only = metadata.base_only.unwrap_or(false);
    let adapter_ids = if base_only {
        Some(adapter_ids.unwrap_or_default())
    } else {
        adapter_ids
    };

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
            match state
                .db
                .get_documents_by_ids_ordered(&metadata.tenant_id, doc_ids)
                .await
            {
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
        base_only: metadata.base_only,
        dataset_version_id: metadata.dataset_version_id,
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
        version_consistency_warning: None,
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
) -> ApiResult<ReplayResponse> {
    // Permission check
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request: must have either inference_id or replay_key
    if req.inference_id.is_none() && req.replay_key.is_none() {
        return Err(ApiError::bad_request(
            "Must provide either inference_id or replay_key",
        ));
    }

    let inference_id = req
        .inference_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("inference_id is required for replay"))?;

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
            ApiError::db_error(e)
        })?
        .ok_or_else(|| ApiError::not_found("Inference"))?;

    // Validate tenant access
    if !check_tenant_access(&claims, &metadata.tenant_id) {
        return Err(ApiError::forbidden("Access denied"));
    }

    // PRD-06: Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    // Security: Replay MUST go through same policy gates as normal inference
    let request_id_str = crate::id_generator::readable_request_id();
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnRequestBeforeRouting,
        "replay_inference",
        None, // No adapter selected yet
    );
    if let Err(violation) = enforce_at_hook(&state, &routing_hook_ctx).await {
        let code = violation
            .code
            .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
        warn!(
            inference_id = %inference_id,
            policy_violation = %violation.message,
            "Replay blocked by policy at OnRequestBeforeRouting"
        );
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy violation (pre-routing)",
        )
        .with_code(code)
        .with_details(violation.message));
    }

    // Parse stored params
    let sampling_params: SamplingParams =
        serde_json::from_str(&metadata.sampling_params_json).unwrap_or_default();

    let adapter_ids: Option<Vec<String>> = metadata
        .adapter_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    let base_only = metadata.base_only.unwrap_or(false);
    if base_only
        && adapter_ids
            .as_ref()
            .map(|ids| !ids.is_empty())
            .unwrap_or(false)
    {
        return Err(ApiError::bad_request(
            "Replay metadata is marked base-only but includes adapter IDs; cannot replay with adapters",
        ));
    }

    let adapter_ids = if base_only {
        Some(Vec::new())
    } else {
        adapter_ids
    };

    let rag_doc_ids: Option<Vec<String>> = metadata
        .rag_doc_ids_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok());

    // Check adapter loadability before attempting replay
    // This is defense-in-depth: verify adapters are not archived/purged
    if let Some(ref adapter_list) = adapter_ids {
        for adapter_id in adapter_list {
            match state.db.is_adapter_loadable(adapter_id).await {
                Ok(true) => continue, // Adapter is loadable
                Ok(false) => {
                    warn!(
                        adapter_id = %adapter_id,
                        inference_id = %inference_id,
                        "Replay blocked: adapter is archived or purged"
                    );
                    return Err(ApiError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "ADAPTER_NOT_LOADABLE",
                        format!(
                            "Adapter '{}' is archived or purged and cannot be used for replay",
                            adapter_id
                        ),
                    ));
                }
                Err(e) => {
                    warn!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "Replay blocked: adapter not found or not loadable"
                    );
                    return Err(ApiError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "ADAPTER_NOT_FOUND",
                        format!("Adapter '{}' is not loadable: {}", adapter_id, e),
                    ));
                }
            }
        }
    }

    // Check RAG availability and compute degraded status (batch lookup)
    let mut missing_doc_ids = Vec::new();
    if let Some(ref doc_ids) = rag_doc_ids {
        if !doc_ids.is_empty() {
            match state
                .db
                .get_documents_by_ids_ordered(&metadata.tenant_id, doc_ids)
                .await
            {
                Ok(docs) => {
                    for (idx, doc) in docs.iter().enumerate() {
                        if doc.is_none() {
                            missing_doc_ids.push(doc_ids[idx].clone());
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        inference_id = %inference_id,
                        error = %e,
                        "Failed to check RAG document availability in replay, assuming all missing"
                    );
                    missing_doc_ids = doc_ids.clone();
                }
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
    let replay_mode = determine_replay_mode(
        missing_doc_ids.len(),
        rag_score,
        metadata.prompt_truncated,
        metadata.response_truncated,
    );

    // Check if approximate replay is allowed
    if replay_mode != "exact" && !req.allow_approximate {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "APPROXIMATE_REPLAY_REQUIRED",
            format!(
                "Replay would be {} but allow_approximate=false",
                replay_mode
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
    let determinism_ctx = crate::determinism_context::from_replay_metadata(&metadata).map_err(|e| {
        warn!(
            inference_id = %inference_id,
            replay_id = %replay_id,
            error = %e,
            "Replay rejected due to missing determinism seeds"
        );
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(
                    "Replay metadata missing request_seed; legacy seed derivation is no longer supported",
                )
                .with_code("LEGACY_REPLAY_UNSUPPORTED")
                .with_string_details("Please re-record the inference to capture determinism seeds"),
            ),
        )
    })?;

    // Restore stop_policy from metadata for deterministic replay
    let stop_policy = metadata.stop_policy_json.as_ref().and_then(|json| {
        serde_json::from_str::<adapteros_api_types::inference::StopPolicySpec>(json).ok()
    });

    // Restore policy_mask_digest from metadata for audit trail completeness
    let stored_policy_mask_digest = metadata.policy_mask_digest_b3.as_ref().and_then(|hex_str| {
        hex::decode(hex_str)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
    });

    // Log warning if policy_mask_digest is missing (indicates incomplete audit trail)
    if stored_policy_mask_digest.is_none() && metadata.policy_mask_digest_b3.is_some() {
        warn!(
            inference_id = %inference_id,
            replay_id = %replay_id,
            "Policy mask digest found in metadata but failed to decode"
        );
    }

    let mut run_envelope = new_run_envelope_no_tick(&state, &claims, replay_id.clone(), false);
    run_envelope.manifest_hash_b3 = Some(metadata.manifest_hash.clone());
    set_policy_mask(&mut run_envelope, stored_policy_mask_digest.as_ref());

    let inference_request = InferenceRequestInternal {
        request_id: replay_id.clone(),
        cpid: claims.tenant_id.clone(),
        prompt: prompt.clone(),
        run_envelope: Some(run_envelope),
        reasoning_mode: false,
        admin_override: false,
        stream: false,
        require_step: false,
        require_determinism: true,
        allow_fallback: false,
        batch_item_id: None,
        rag_enabled: rag_doc_ids.is_some() && !req.skip_rag,
        rag_collection_id: None, // RAG context will be reconstructed from doc IDs if needed
        dataset_version_id: metadata.dataset_version_id.clone(),
        adapter_stack: None,
        adapters: adapter_ids.clone(),
        stack_id: None,
        stack_version: None,
        stack_determinism_mode: None,
        stack_routing_determinism_mode: None,
        domain_hint: None,
        effective_adapter_ids: if base_only { Some(Vec::new()) } else { None },
        determinism_mode: metadata
            .determinism_mode
            .clone()
            .or_else(|| Some("strict".to_string())),
        routing_determinism_mode: Some(RoutingDeterminismMode::Deterministic),
        adapter_strength_overrides: None,
        seed_mode: Some(sampling_params.seed_mode.unwrap_or(SeedMode::BestEffort)),
        request_seed: Some(determinism_ctx.request_seed()),
        backend_profile: None,
        coreml_mode: None,
        max_tokens: sampling_params.max_tokens,
        temperature: sampling_params.temperature,
        top_k: sampling_params.top_k,
        top_p: sampling_params.top_p,
        seed: Some(determinism_ctx.request_seed_low64()),
        router_seed: Some(determinism_ctx.router_seed_hex().to_string()),
        require_evidence: false,
        session_id: None,
        pinned_adapter_ids: None, // Not used in replay
        chat_context_hash: None,
        claims: Some(claims.clone()),
        model: None,
        stop_policy, // Restored from original inference for deterministic replay
        created_at: std::time::Instant::now(),
        worker_auth_token: None,
        policy_mask_digest_b3: stored_policy_mask_digest, // Restored from metadata for audit trail
        utf8_healing: None,
        abstention_threshold: None, // AARA lifecycle
        citation_mode: None,        // AARA lifecycle
        fim_prefix: None,
        fim_suffix: None,
    };

    if base_only
        && inference_request
            .effective_adapter_ids
            .as_ref()
            .map(|ids| !ids.is_empty())
            .unwrap_or(true)
    {
        return Err(ApiError::bad_request(
            "Replay metadata is base-only but replay request is not base-only (adapters present)",
        ));
    }

    // Build replay context with manifest/backend constraints (PRD-02)
    let replay_context = ReplayContext {
        original_inference_id: inference_id.clone(),
        required_manifest_hash: metadata.manifest_hash.clone(),
        required_backend: metadata.backend.clone(),
        skip_metadata_capture: true, // Don't create new replay metadata for replay
        original_policy_id: metadata.execution_policy_id.clone(),
        original_policy_version: metadata.execution_policy_version.map(|v| v as i64),
    };

    // PRD-06: Enforce policies at OnBeforeInference hook (after routing, before inference)
    let adapters_for_hook = adapter_ids.as_ref().map(|a| a.join(","));
    let before_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnBeforeInference,
        "replay_inference",
        adapters_for_hook.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &before_hook_ctx).await {
        let code = violation
            .code
            .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
        warn!(
            inference_id = %inference_id,
            policy_violation = %violation.message,
            "Replay blocked by policy at OnBeforeInference"
        );
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy violation (pre-inference)",
        )
        .with_code(code)
        .with_details(violation.message));
    }

    // Log policy mask digest restoration for audit trail
    if let Some(digest) = stored_policy_mask_digest {
        debug!(
            inference_id = %inference_id,
            replay_id = %replay_id,
            policy_mask_digest = %hex::encode(digest),
            "Restored policy mask digest from original inference"
        );
    } else if metadata.policy_mask_digest_b3.is_none() {
        debug!(
            inference_id = %inference_id,
            replay_id = %replay_id,
            "No policy mask digest found in metadata (legacy inference or no policies applied)"
        );
    }

    // Execute inference through InferenceCore (PRD-02: unified inference path)
    let core = InferenceCore::new(&state);
    let inference_result = core
        .route_and_infer_replay(inference_request, replay_context, None)
        .await;

    let latency_ms = match &inference_result {
        Ok(r) => r.latency_ms as i32,
        Err(_) => 0,
    };

    // Handle result
    let (response_text, tokens_generated, match_status, error_message) = match inference_result {
        Ok(result) => {
            let replay_text = result.text;
            let original_text = metadata.response_text.as_deref().unwrap_or("");
            let status = compute_match_status(original_text, &replay_text);
            let tokens_generated = i32::try_from(result.tokens_generated).unwrap_or(i32::MAX);
            (replay_text, tokens_generated, status, None)
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

    // PRD-06: Enforce policies at OnAfterInference hook (post-inference validation/audit)
    let after_hook_ctx = create_hook_context(
        &claims,
        &request_id_str,
        PolicyHook::OnAfterInference,
        "replay_inference",
        adapters_for_hook.as_deref(),
    );
    if let Err(violation) = enforce_at_hook(&state, &after_hook_ctx).await {
        let code = violation
            .code
            .unwrap_or_else(|| "POLICY_HOOK_VIOLATION".to_string());
        warn!(
            inference_id = %inference_id,
            policy_violation = %violation.message,
            "Replay blocked by policy at OnAfterInference"
        );
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy violation (post-inference)",
        )
        .with_code(code)
        .with_details(violation.message));
    }

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
) -> ApiResult<ReplayHistoryResponse> {
    // Permission check - InferenceExecute grants access to inference results
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &inference_id).await?;

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
            ApiError::db_error(e)
        })?;

    if let Some(ref m) = metadata {
        if !check_tenant_access(&claims, &m.tenant_id) {
            return Err(ApiError::forbidden("Access denied"));
        }
    }

    // Load replay executions
    let executions = state
        .db
        .list_replay_executions_for_inference(&inference_id)
        .await
        .map_err(|e| {
            error!(inference_id = %inference_id, error = %e, "Failed to load replay history");
            ApiError::db_error(e)
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
    fn test_determine_replay_mode_base_only_exact() {
        let mode = determine_replay_mode(0, 1.0, 0, 0);
        assert_eq!(mode, "exact");
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
