//! Discrepancy Case API handlers
//!
//! Provides REST endpoints for managing discrepancy cases linked to inference runs.
//! Discrepancy cases capture instances where model output diverged from ground truth,
//! enabling targeted training feedback collection.

use crate::api_error::ApiError;
use crate::audit_helper::{actions, log_failure_or_warn, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use utoipa::{IntoParams, ToSchema};

// ============================================================================
// Constants
// ============================================================================

/// Valid discrepancy types
const VALID_DISCREPANCY_TYPES: &[&str] = &[
    "incorrect_answer",
    "incomplete_answer",
    "hallucination",
    "wrong_format",
    "outdated_info",
    "other",
];

/// Valid resolution statuses
const VALID_RESOLUTION_STATUSES: &[&str] = &[
    "pending",
    "confirmed_error",
    "not_an_error",
    "model_limitation",
    "needs_review",
];

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a discrepancy case
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDiscrepancyRequest {
    /// Inference run ID that produced the discrepancy
    pub inference_id: String,
    /// Type of discrepancy (incorrect_answer, incomplete_answer, hallucination, etc.)
    pub discrepancy_type: String,

    // Document reference (optional)
    /// Document ID referenced during inference
    pub document_id: Option<String>,
    /// Page number in source document
    pub page_number: Option<u32>,
    /// BLAKE3 hash of the chunk used
    pub chunk_hash_b3: Option<String>,

    // Content (explicit opt-in for plaintext storage)
    /// Whether to store plaintext content (requires explicit opt-in)
    #[serde(default)]
    pub store_content: bool,
    /// The user's original question
    pub user_question: Option<String>,
    /// The model's answer
    pub model_answer: Option<String>,
    /// The correct/expected answer
    pub ground_truth: Option<String>,

    /// Additional notes
    pub notes: Option<String>,
}

/// Response containing a discrepancy case
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscrepancyResponse {
    pub id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub discrepancy_type: String,
    pub resolution_status: String,
    pub document_id: Option<String>,
    pub page_number: Option<u32>,
    pub chunk_hash_b3: Option<String>,
    pub user_question: Option<String>,
    pub model_answer: Option<String>,
    pub ground_truth: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
}

/// Query parameters for listing discrepancies
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListDiscrepanciesQuery {
    /// Filter by resolution status
    pub status: Option<String>,
    /// Filter by discrepancy type
    pub discrepancy_type: Option<String>,
    /// Filter by inference ID
    pub inference_id: Option<String>,
    /// Filter by document ID
    pub document_id: Option<String>,
    /// Maximum number of results (default: 50)
    pub limit: Option<i64>,
    /// Offset for pagination
    pub offset: Option<i64>,
}

/// Request to resolve/update a discrepancy case
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResolveDiscrepancyRequest {
    /// New resolution status (confirmed_error, not_an_error, model_limitation, needs_review)
    pub resolution_status: String,
    /// Additional notes about the resolution
    pub notes: Option<String>,
}

/// Export format for training data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscrepancyExportRow {
    pub id: String,
    pub inference_id: String,
    pub discrepancy_type: String,
    pub user_question: Option<String>,
    pub model_answer: Option<String>,
    pub ground_truth: Option<String>,
    pub document_id: Option<String>,
    pub chunk_hash_b3: Option<String>,
    pub confirmed_at: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Create a new discrepancy case
#[utoipa::path(
    post,
    path = "/v1/discrepancies",
    request_body = CreateDiscrepancyRequest,
    responses(
        (status = 201, description = "Discrepancy case created", body = DiscrepancyResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "discrepancies",
    security(("bearer_auth" = []))
)]
pub async fn create_discrepancy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateDiscrepancyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to manage inference
    require_permission(&claims, Permission::InferenceExecute)?;

    // Validate discrepancy type
    if !VALID_DISCREPANCY_TYPES.contains(&request.discrepancy_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Invalid discrepancy_type. Must be one of: {}",
                    VALID_DISCREPANCY_TYPES.join(", ")
                ))
                .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate inference_id exists and belongs to tenant
    // Note: We don't enforce tenant isolation here since the inference may not have
    // tenant_id directly accessible. The claims.tenant_id is used for the new case.

    // Generate ID
    let id = crate::id_generator::readable_id(adapteros_core::ids::IdKind::Event, "discrepancy");
    let now = chrono::Utc::now().to_rfc3339();

    // Clear content if store_content is false
    let (user_question, model_answer, ground_truth) = if request.store_content {
        (
            request.user_question.clone(),
            request.model_answer.clone(),
            request.ground_truth.clone(),
        )
    } else {
        (None, None, None)
    };

    // Insert the discrepancy case
    let insert_result = sqlx::query(
        r#"
        INSERT INTO discrepancy_cases (
            id, tenant_id, inference_id, discrepancy_type, resolution_status,
            document_id, page_number, chunk_hash_b3,
            user_question, model_answer, ground_truth, notes,
            created_at, updated_at, created_by
        ) VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&claims.tenant_id)
    .bind(&request.inference_id)
    .bind(&request.discrepancy_type)
    .bind(&request.document_id)
    .bind(request.page_number.map(|p| p as i64))
    .bind(&request.chunk_hash_b3)
    .bind(&user_question)
    .bind(&model_answer)
    .bind(&ground_truth)
    .bind(&request.notes)
    .bind(&now)
    .bind(&now)
    .bind(&claims.sub)
    .execute(state.db.pool())
    .await;

    if let Err(e) = &insert_result {
        error!(error = %e, "Failed to create discrepancy case");
        let message = format!("Failed to create discrepancy case: {}", e);
        log_failure_or_warn(
            &state.db,
            &claims,
            actions::INFERENCE_EXECUTE,
            resources::INFERENCE,
            None,
            &message,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create discrepancy case")
                    .with_code("INTERNAL_ERROR"),
            ),
        ));
    }

    log_success_or_warn(
        &state.db,
        &claims,
        actions::INFERENCE_EXECUTE,
        resources::INFERENCE,
        Some(&id),
    )
    .await;

    info!(
        discrepancy_id = %id,
        inference_id = %request.inference_id,
        discrepancy_type = %request.discrepancy_type,
        user = %claims.sub,
        "Discrepancy case created"
    );

    let response = DiscrepancyResponse {
        id,
        tenant_id: claims.tenant_id.clone(),
        inference_id: request.inference_id,
        discrepancy_type: request.discrepancy_type,
        resolution_status: "pending".to_string(),
        document_id: request.document_id,
        page_number: request.page_number,
        chunk_hash_b3: request.chunk_hash_b3,
        user_question,
        model_answer,
        ground_truth,
        notes: request.notes,
        created_at: now.clone(),
        updated_at: now,
        created_by: Some(claims.sub),
        resolved_by: None,
        resolved_at: None,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// Get a single discrepancy case by ID
#[utoipa::path(
    get,
    path = "/v1/discrepancies/{id}",
    params(
        ("id" = String, Path, description = "Discrepancy case ID")
    ),
    responses(
        (status = 200, description = "Discrepancy case details", body = DiscrepancyResponse),
        (status = 404, description = "Discrepancy case not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "discrepancies",
    security(("bearer_auth" = []))
)]
pub async fn get_discrepancy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view inference
    require_permission(&claims, Permission::InferenceExecute)?;
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let row = sqlx::query_as::<_, DiscrepancyRow>(
        r#"
        SELECT id, tenant_id, inference_id, discrepancy_type, resolution_status,
               document_id, page_number, chunk_hash_b3,
               user_question, model_answer, ground_truth, notes,
               created_at, updated_at, created_by, resolved_by, resolved_at
        FROM discrepancy_cases
        WHERE id = ?
        "#,
    )
    .bind(&id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(ApiError::db_error)?
    .ok_or_else(|| ApiError::not_found("Discrepancy case"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &row.tenant_id)?;

    Ok(Json(row.into_response()))
}

/// List discrepancy cases with optional filters
#[utoipa::path(
    get,
    path = "/v1/discrepancies",
    params(ListDiscrepanciesQuery),
    responses(
        (status = 200, description = "List of discrepancy cases", body = Vec<DiscrepancyResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "discrepancies",
    security(("bearer_auth" = []))
)]
pub async fn list_discrepancies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListDiscrepanciesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view inference
    require_permission(&claims, Permission::InferenceExecute)?;

    let limit = query.limit.unwrap_or(50).min(500);
    let offset = query.offset.unwrap_or(0);

    // Build dynamic query with filters
    let mut sql = String::from(
        r#"
        SELECT id, tenant_id, inference_id, discrepancy_type, resolution_status,
               document_id, page_number, chunk_hash_b3,
               user_question, model_answer, ground_truth, notes,
               created_at, updated_at, created_by, resolved_by, resolved_at
        FROM discrepancy_cases
        WHERE tenant_id = ?
        "#,
    );

    let mut params: Vec<String> = vec![claims.tenant_id.clone()];

    if let Some(ref status) = query.status {
        sql.push_str(" AND resolution_status = ?");
        params.push(status.clone());
    }

    if let Some(ref discrepancy_type) = query.discrepancy_type {
        sql.push_str(" AND discrepancy_type = ?");
        params.push(discrepancy_type.clone());
    }

    if let Some(ref inference_id) = query.inference_id {
        sql.push_str(" AND inference_id = ?");
        params.push(inference_id.clone());
    }

    if let Some(ref document_id) = query.document_id {
        sql.push_str(" AND document_id = ?");
        params.push(document_id.clone());
    }

    sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

    // Execute with dynamic binding
    let mut query_builder = sqlx::query_as::<_, DiscrepancyRow>(&sql);
    for param in &params {
        query_builder = query_builder.bind(param);
    }
    query_builder = query_builder.bind(limit).bind(offset);

    let rows = query_builder
        .fetch_all(state.db.pool())
        .await
        .map_err(ApiError::db_error)?;

    let responses: Vec<DiscrepancyResponse> = rows.into_iter().map(|r| r.into_response()).collect();

    debug!(
        tenant_id = %claims.tenant_id,
        count = responses.len(),
        "Listed discrepancy cases"
    );

    Ok(Json(responses))
}

/// Update resolution status of a discrepancy case
#[utoipa::path(
    patch,
    path = "/v1/discrepancies/{id}/resolve",
    params(
        ("id" = String, Path, description = "Discrepancy case ID")
    ),
    request_body = ResolveDiscrepancyRequest,
    responses(
        (status = 200, description = "Discrepancy case resolved", body = DiscrepancyResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Discrepancy case not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "discrepancies",
    security(("bearer_auth" = []))
)]
pub async fn resolve_discrepancy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<ResolveDiscrepancyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require manage permission for resolution
    require_permission(&claims, Permission::TrainingStart)?;
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Validate resolution status
    if !VALID_RESOLUTION_STATUSES.contains(&request.resolution_status.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Invalid resolution_status. Must be one of: {}",
                    VALID_RESOLUTION_STATUSES.join(", ")
                ))
                .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Fetch existing case
    let existing = sqlx::query_as::<_, DiscrepancyRow>(
        r#"
        SELECT id, tenant_id, inference_id, discrepancy_type, resolution_status,
               document_id, page_number, chunk_hash_b3,
               user_question, model_answer, ground_truth, notes,
               created_at, updated_at, created_by, resolved_by, resolved_at
        FROM discrepancy_cases
        WHERE id = ?
        "#,
    )
    .bind(&id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(ApiError::db_error)?
    .ok_or_else(|| ApiError::not_found("Discrepancy case"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    let now = chrono::Utc::now().to_rfc3339();

    // Update with new notes if provided, otherwise keep existing
    let updated_notes = request.notes.or(existing.notes.clone());

    // Update the case
    let update_result = sqlx::query(
        r#"
        UPDATE discrepancy_cases
        SET resolution_status = ?,
            notes = ?,
            resolved_by = ?,
            resolved_at = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&request.resolution_status)
    .bind(&updated_notes)
    .bind(&claims.sub)
    .bind(&now)
    .bind(&now)
    .bind(&id)
    .execute(state.db.pool())
    .await;

    if let Err(e) = &update_result {
        error!(error = %e, id = %id, "Failed to resolve discrepancy case");
        let message = format!("Failed to resolve discrepancy case: {}", e);
        log_failure_or_warn(
            &state.db,
            &claims,
            actions::TRAINING_START,
            resources::TRAINING,
            Some(&id),
            &message,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to resolve discrepancy case")
                    .with_code("INTERNAL_ERROR"),
            ),
        ));
    }

    log_success_or_warn(
        &state.db,
        &claims,
        actions::TRAINING_START,
        resources::TRAINING,
        Some(&id),
    )
    .await;

    info!(
        discrepancy_id = %id,
        resolution_status = %request.resolution_status,
        user = %claims.sub,
        "Discrepancy case resolved"
    );

    let response = DiscrepancyResponse {
        id,
        tenant_id: existing.tenant_id,
        inference_id: existing.inference_id,
        discrepancy_type: existing.discrepancy_type,
        resolution_status: request.resolution_status,
        document_id: existing.document_id,
        page_number: existing.page_number.map(|p| p as u32),
        chunk_hash_b3: existing.chunk_hash_b3,
        user_question: existing.user_question,
        model_answer: existing.model_answer,
        ground_truth: existing.ground_truth,
        notes: updated_notes,
        created_at: existing.created_at,
        updated_at: now.clone(),
        created_by: existing.created_by,
        resolved_by: Some(claims.sub),
        resolved_at: Some(now),
    };

    Ok(Json(response))
}

/// Export confirmed errors as JSONL for training
#[utoipa::path(
    get,
    path = "/v1/discrepancies/export",
    responses(
        (status = 200, description = "JSONL export of confirmed errors", content_type = "application/x-ndjson"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "discrepancies",
    security(("bearer_auth" = []))
)]
pub async fn export_discrepancies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require training permission for export
    require_permission(&claims, Permission::TrainingStart)?;

    // Fetch all confirmed_error cases for the tenant
    let rows = sqlx::query_as::<_, DiscrepancyRow>(
        r#"
        SELECT id, tenant_id, inference_id, discrepancy_type, resolution_status,
               document_id, page_number, chunk_hash_b3,
               user_question, model_answer, ground_truth, notes,
               created_at, updated_at, created_by, resolved_by, resolved_at
        FROM discrepancy_cases
        WHERE tenant_id = ? AND resolution_status = 'confirmed_error'
        ORDER BY resolved_at DESC
        "#,
    )
    .bind(&claims.tenant_id)
    .fetch_all(state.db.pool())
    .await
    .map_err(ApiError::db_error)?;

    // Convert to JSONL format
    let mut jsonl_output = String::new();
    for row in rows {
        let export_row = DiscrepancyExportRow {
            id: row.id,
            inference_id: row.inference_id,
            discrepancy_type: row.discrepancy_type,
            user_question: row.user_question,
            model_answer: row.model_answer,
            ground_truth: row.ground_truth,
            document_id: row.document_id,
            chunk_hash_b3: row.chunk_hash_b3,
            confirmed_at: row.resolved_at.unwrap_or_else(|| row.updated_at),
        };

        if let Ok(line) = serde_json::to_string(&export_row) {
            jsonl_output.push_str(&line);
            jsonl_output.push('\n');
        }
    }

    info!(
        tenant_id = %claims.tenant_id,
        user = %claims.sub,
        "Exported discrepancy cases for training"
    );

    Ok((
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/x-ndjson; charset=utf-8",
        )],
        jsonl_output,
    ))
}

// ============================================================================
// Database Row Type
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct DiscrepancyRow {
    id: String,
    tenant_id: String,
    inference_id: String,
    discrepancy_type: String,
    resolution_status: String,
    document_id: Option<String>,
    page_number: Option<i64>,
    chunk_hash_b3: Option<String>,
    user_question: Option<String>,
    model_answer: Option<String>,
    ground_truth: Option<String>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
    created_by: Option<String>,
    resolved_by: Option<String>,
    resolved_at: Option<String>,
}

impl DiscrepancyRow {
    fn into_response(self) -> DiscrepancyResponse {
        DiscrepancyResponse {
            id: self.id,
            tenant_id: self.tenant_id,
            inference_id: self.inference_id,
            discrepancy_type: self.discrepancy_type,
            resolution_status: self.resolution_status,
            document_id: self.document_id,
            page_number: self.page_number.map(|p| p as u32),
            chunk_hash_b3: self.chunk_hash_b3,
            user_question: self.user_question,
            model_answer: self.model_answer,
            ground_truth: self.ground_truth,
            notes: self.notes,
            created_at: self.created_at,
            updated_at: self.updated_at,
            created_by: self.created_by,
            resolved_by: self.resolved_by,
            resolved_at: self.resolved_at,
        }
    }
}
