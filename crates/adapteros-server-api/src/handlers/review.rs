//! Review Protocol API Handlers
//!
//! Endpoints for the human-in-the-loop review protocol:
//! - GET /v1/infer/{id}/state - Check if inference is paused
//! - POST /v1/infer/{id}/review - Submit review to resume
//! - GET /v1/infer/paused - List all paused inferences

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::{info, warn};

use adapteros_api_types::review::{
    InferenceState, InferenceStateResponse, ListPausedResponse,
    PausedInferenceInfo as ApiPausedInfo, ReviewContextExport, SubmitReviewRequest,
    SubmitReviewResponse,
};
use adapteros_api_types::{schema_version, ErrorResponse};

use crate::pause_tracker::ServerPauseTracker;
use crate::state::AppState;
use adapteros_core::AosError;

/// Map AosError to appropriate HTTP status code and error response
fn map_aos_error(e: AosError, default_code: &str) -> (StatusCode, Json<ErrorResponse>) {
    match &e {
        AosError::NotFound(msg) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(msg).with_code("NOT_FOUND")),
        ),
        AosError::Validation(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(msg).with_code("VALIDATION_ERROR")),
        ),
        AosError::Internal(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(msg).with_code("INTERNAL_ERROR")),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string()).with_code(default_code)),
        ),
    }
}

// =============================================================================
// GET /v1/infer/{id}/state
// =============================================================================

/// Get the state of an inference request
#[utoipa::path(
    tag = "inference",
    get,
    path = "/v1/infer/{inference_id}/state",
    params(
        ("inference_id" = String, Path, description = "Inference request ID")
    ),
    responses(
        (status = 200, description = "Inference state retrieved", body = InferenceStateResponse),
        (status = 404, description = "Inference not found", body = ErrorResponse)
    )
)]
pub async fn get_inference_state(
    State(state): State<AppState>,
    Path(inference_id): Path<String>,
) -> Result<Json<InferenceStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &inference_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let tracker = get_pause_tracker(&state)?;

    // Check if this inference is paused
    if let Some(info) = tracker.get_state_by_inference(&inference_id) {
        let paused_at_str = info.created_at.to_rfc3339();
        let response = InferenceStateResponse {
            schema_version: schema_version(),
            inference_id: info.inference_id,
            state: InferenceState::Paused(adapteros_api_types::review::PauseReason {
                kind: info.kind,
                pause_id: info.pause_id,
                context: info.context,
                created_at: Some(paused_at_str.clone()),
            }),
            paused_at: Some(paused_at_str),
            paused_duration_secs: Some(info.duration_secs),
        };
        return Ok(Json(response));
    }

    // Not paused - check inference state tracker for more accurate state
    if let Some(ref state_tracker) = state.inference_state_tracker {
        if let Some(tracked_state) = state_tracker.get_state(&inference_id) {
            return Ok(Json(InferenceStateResponse {
                schema_version: schema_version(),
                inference_id,
                state: tracked_state,
                paused_at: None,
                paused_duration_secs: None,
            }));
        }
    }

    // Not tracked - return running as default
    Ok(Json(InferenceStateResponse {
        schema_version: schema_version(),
        inference_id,
        state: InferenceState::Running,
        paused_at: None,
        paused_duration_secs: None,
    }))
}

// =============================================================================
// POST /v1/infer/{id}/review
// =============================================================================

/// Submit a review to resume a paused inference
#[utoipa::path(
    tag = "inference",
    post,
    path = "/v1/infer/{inference_id}/review",
    params(
        ("inference_id" = String, Path, description = "Inference request ID")
    ),
    request_body = SubmitReviewRequest,
    responses(
        (status = 200, description = "Review submitted, inference resumed", body = SubmitReviewResponse),
        (status = 404, description = "No paused inference with this ID", body = ErrorResponse),
        (status = 400, description = "Invalid review", body = ErrorResponse)
    )
)]
pub async fn submit_review(
    State(state): State<AppState>,
    Path(inference_id): Path<String>,
    Json(request): Json<SubmitReviewRequest>,
) -> Result<Json<SubmitReviewResponse>, (StatusCode, Json<ErrorResponse>)> {
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &inference_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let tracker = get_pause_tracker(&state)?;

    info!(
        inference_id = %inference_id,
        pause_id = %request.pause_id,
        reviewer = %request.reviewer,
        assessment = ?request.review.assessment,
        "Submitting review for paused inference"
    );

    match tracker.submit_review(request).await {
        Ok(new_state) => {
            info!(inference_id = %inference_id, "Inference resumed with review");

            // Update inference state tracker
            if let Some(ref state_tracker) = state.inference_state_tracker {
                state_tracker.mark_resumed(&inference_id);
            }

            Ok(Json(SubmitReviewResponse {
                schema_version: schema_version(),
                accepted: true,
                new_state,
                message: Some("Review accepted, inference resumed".to_string()),
            }))
        }
        Err(e) => {
            warn!(inference_id = %inference_id, error = %e, "Failed to submit review");
            Err(map_aos_error(e, "REVIEW_ERROR"))
        }
    }
}

// =============================================================================
// GET /v1/infer/paused
// =============================================================================

/// List all paused inferences
#[utoipa::path(
    tag = "inference",
    get,
    path = "/v1/infer/paused",
    responses(
        (status = 200, description = "List of paused inferences", body = ListPausedResponse)
    )
)]
pub async fn list_paused(
    State(state): State<AppState>,
) -> Result<Json<ListPausedResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tracker = get_pause_tracker(&state)?;

    let paused_list = tracker.list_paused();
    let total = paused_list.len();

    let paused: Vec<ApiPausedInfo> = paused_list
        .into_iter()
        .map(|info| ApiPausedInfo {
            inference_id: info.inference_id,
            pause_id: info.pause_id,
            kind: info.kind,
            paused_at: info.created_at.to_rfc3339(),
            duration_secs: info.duration_secs,
            context_preview: info.context.question.clone().map(|q| {
                if q.len() > 100 {
                    format!("{}...", &q[..97])
                } else {
                    q
                }
            }),
        })
        .collect();

    Ok(Json(ListPausedResponse {
        schema_version: schema_version(),
        paused,
        total,
    }))
}

// =============================================================================
// GET /v1/reviews/paused - Alias for CLI compatibility
// =============================================================================

/// List all paused inferences (CLI-compatible alias)
#[utoipa::path(
    tag = "reviews",
    get,
    path = "/v1/reviews/paused",
    responses(
        (status = 200, description = "List of paused inferences", body = ListPausedResponse)
    )
)]
pub async fn list_paused_reviews(
    State(state): State<AppState>,
) -> Result<Json<ListPausedResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Delegate to existing handler
    list_paused(State(state)).await
}

// =============================================================================
// GET /v1/reviews/{pause_id} - Get pause details by pause ID
// =============================================================================

/// Get details for a specific paused inference by pause ID
#[utoipa::path(
    tag = "reviews",
    get,
    path = "/v1/reviews/{pause_id}",
    params(
        ("pause_id" = String, Path, description = "Pause ID")
    ),
    responses(
        (status = 200, description = "Pause details", body = InferenceStateResponse),
        (status = 404, description = "Pause not found", body = ErrorResponse)
    )
)]
pub async fn get_pause_details(
    State(state): State<AppState>,
    Path(pause_id): Path<String>,
) -> Result<Json<InferenceStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pause_id = crate::id_resolver::resolve_any_id(&state.db, &pause_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let tracker = get_pause_tracker(&state)?;

    if let Some(info) = tracker.get_state_by_pause_id(&pause_id) {
        let paused_at_str = info.created_at.to_rfc3339();
        let response = InferenceStateResponse {
            schema_version: schema_version(),
            inference_id: info.inference_id,
            state: InferenceState::Paused(adapteros_api_types::review::PauseReason {
                kind: info.kind,
                pause_id: info.pause_id,
                context: info.context,
                created_at: Some(paused_at_str.clone()),
            }),
            paused_at: Some(paused_at_str),
            paused_duration_secs: Some(info.duration_secs),
        };
        return Ok(Json(response));
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new(format!("Pause ID not found: {}", pause_id))
                .with_code("PAUSE_NOT_FOUND"),
        ),
    ))
}

// =============================================================================
// GET /v1/reviews/{pause_id}/context - Export review context
// =============================================================================

/// Export review context for external reviewers (e.g., Claude Code)
#[utoipa::path(
    tag = "reviews",
    get,
    path = "/v1/reviews/{pause_id}/context",
    params(
        ("pause_id" = String, Path, description = "Pause ID")
    ),
    responses(
        (status = 200, description = "Review context for export", body = ReviewContextExport),
        (status = 404, description = "Pause not found", body = ErrorResponse)
    )
)]
pub async fn export_review_context(
    State(state): State<AppState>,
    Path(pause_id): Path<String>,
) -> Result<Json<ReviewContextExport>, (StatusCode, Json<ErrorResponse>)> {
    let pause_id = crate::id_resolver::resolve_any_id(&state.db, &pause_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let tracker = get_pause_tracker(&state)?;

    if let Some(info) = tracker.get_state_by_pause_id(&pause_id) {
        let response = ReviewContextExport {
            pause_id: pause_id.clone(),
            inference_id: info.inference_id,
            kind: format!("{:?}", info.kind),
            paused_at: info.created_at.to_rfc3339(),
            duration_secs: info.duration_secs,
            code: info.context.code.clone(),
            question: info.context.question.clone(),
            scope: info
                .context
                .scope
                .iter()
                .map(|s| format!("{:?}", s))
                .collect(),
            metadata: info.context.metadata.clone(),
            instructions: format!(
                "Review this item and respond with a JSON file containing:\n\
                 - assessment: Approved | ApprovedWithSuggestions | NeedsChanges | Rejected\n\
                 - issues: [{{severity, description, suggested_fix}}]\n\
                 - suggestions: [string]\n\
                 - comments: string\n\n\
                 Then import with: aosctl review import {} -f response.json",
                pause_id
            ),
        };
        return Ok(Json(response));
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new(format!("Pause ID not found: {}", pause_id))
                .with_code("PAUSE_NOT_FOUND"),
        ),
    ))
}

// =============================================================================
// POST /v1/reviews/submit - Submit review (CLI-compatible)
// =============================================================================

/// Submit a review to resume a paused inference (CLI-compatible)
#[utoipa::path(
    tag = "reviews",
    post,
    path = "/v1/reviews/submit",
    request_body = SubmitReviewRequest,
    responses(
        (status = 200, description = "Review submitted", body = SubmitReviewResponse),
        (status = 404, description = "Pause not found", body = ErrorResponse),
        (status = 400, description = "Invalid review", body = ErrorResponse)
    )
)]
pub async fn submit_review_response(
    State(state): State<AppState>,
    Json(request): Json<SubmitReviewRequest>,
) -> Result<Json<SubmitReviewResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tracker = get_pause_tracker(&state)?;

    info!(
        pause_id = %request.pause_id,
        reviewer = %request.reviewer,
        assessment = ?request.review.assessment,
        "Submitting review via /v1/reviews/submit"
    );

    // Get inference_id before submit (for state tracker update)
    let inference_id = tracker
        .get_state_by_pause_id(&request.pause_id)
        .map(|info| info.inference_id.clone());

    match tracker.submit_review(request).await {
        Ok(new_state) => {
            info!("Review submitted successfully");

            // Update inference state tracker if we have the inference_id
            if let (Some(ref state_tracker), Some(ref infer_id)) =
                (&state.inference_state_tracker, &inference_id)
            {
                state_tracker.mark_resumed(infer_id);
            }

            Ok(Json(SubmitReviewResponse {
                schema_version: schema_version(),
                accepted: true,
                new_state,
                message: Some("Review accepted, inference resumed".to_string()),
            }))
        }
        Err(e) => {
            warn!(error = %e, "Failed to submit review");
            Err(map_aos_error(e, "REVIEW_ERROR"))
        }
    }
}

// =============================================================================
// Helper
// =============================================================================

/// Get the pause tracker from app state
fn get_pause_tracker(
    state: &AppState,
) -> Result<Arc<ServerPauseTracker>, (StatusCode, Json<ErrorResponse>)> {
    state.pause_tracker.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Server pause tracker not initialized")
                    .with_code("TRACKER_NOT_AVAILABLE"),
            ),
        )
    })
}
