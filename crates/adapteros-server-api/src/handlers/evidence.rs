//! Evidence Explorer API handlers (PRD-DATA-01 Phase 2)
//!
//! Provides REST endpoints for managing evidence entries linked to datasets and adapters.

use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use adapteros_db::training_datasets::{CreateEvidenceParams, EvidenceEntry, EvidenceFilter};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};
use utoipa::{IntoParams, ToSchema};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create an evidence entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateEvidenceRequest {
    /// Dataset ID (optional, must have dataset_id or adapter_id)
    pub dataset_id: Option<String>,
    /// Adapter ID (optional, must have dataset_id or adapter_id)
    pub adapter_id: Option<String>,
    /// Evidence type (doc, ticket, commit, policy_approval, data_agreement, review, audit, other)
    pub evidence_type: String,
    /// Reference (URL, commit SHA, ticket ID, document path)
    pub reference: String,
    /// Description of evidence
    pub description: Option<String>,
    /// Confidence level (high, medium, low)
    #[serde(default = "default_confidence")]
    pub confidence: String,
    /// Additional metadata as JSON string
    pub metadata_json: Option<String>,
}

fn default_confidence() -> String {
    "medium".to_string()
}

/// Response containing evidence entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EvidenceResponse {
    pub id: String,
    pub dataset_id: Option<String>,
    pub adapter_id: Option<String>,
    pub evidence_type: String,
    pub reference: String,
    pub description: Option<String>,
    pub confidence: String,
    pub created_by: Option<String>,
    pub created_at: String,
    pub metadata_json: Option<String>,
}

impl From<EvidenceEntry> for EvidenceResponse {
    fn from(entry: EvidenceEntry) -> Self {
        Self {
            id: entry.id,
            dataset_id: entry.dataset_id,
            adapter_id: entry.adapter_id,
            evidence_type: entry.evidence_type,
            reference: entry.reference,
            description: entry.description,
            confidence: entry.confidence,
            created_by: entry.created_by,
            created_at: entry.created_at,
            metadata_json: entry.metadata_json,
        }
    }
}

/// Query parameters for listing evidence
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListEvidenceQuery {
    /// Filter by dataset ID
    pub dataset_id: Option<String>,
    /// Filter by adapter ID
    pub adapter_id: Option<String>,
    /// Filter by evidence type
    pub evidence_type: Option<String>,
    /// Filter by confidence level
    pub confidence: Option<String>,
    /// Maximum number of results
    pub limit: Option<i64>,
}

// ============================================================================
// Handlers
// ============================================================================

/// List evidence entries with optional filters
#[utoipa::path(
    get,
    path = "/v1/evidence",
    params(ListEvidenceQuery),
    responses(
        (status = 200, description = "List of evidence entries", body = Vec<EvidenceResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn list_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListEvidenceQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require permission to view evidence
    require_permission(&claims, Permission::AdapterView)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    let filter = EvidenceFilter {
        dataset_id: query.dataset_id,
        adapter_id: query.adapter_id,
        evidence_type: query.evidence_type,
        confidence: query.confidence,
        limit: query.limit,
    };

    let entries = state.db.list_evidence_entries(&filter).await.map_err(|e| {
        error!(error = %e, "Failed to list evidence entries");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list evidence entries: {}", e),
        )
    })?;

    let responses: Vec<EvidenceResponse> = entries.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Create a new evidence entry
#[utoipa::path(
    post,
    path = "/v1/evidence",
    request_body = CreateEvidenceRequest,
    responses(
        (status = 201, description = "Evidence entry created", body = EvidenceResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn create_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateEvidenceRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require permission to register adapters/datasets
    require_permission(&claims, Permission::AdapterRegister)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Validate at least one ID is provided
    if request.dataset_id.is_none() && request.adapter_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Either dataset_id or adapter_id must be provided".to_string(),
        ));
    }

    // Validate evidence type
    let valid_types = [
        "doc",
        "ticket",
        "commit",
        "policy_approval",
        "data_agreement",
        "review",
        "audit",
        "other",
    ];
    if !valid_types.contains(&request.evidence_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid evidence_type. Must be one of: {}",
                valid_types.join(", ")
            ),
        ));
    }

    // Validate confidence level
    let valid_confidence = ["high", "medium", "low"];
    if !valid_confidence.contains(&request.confidence.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid confidence. Must be one of: {}",
                valid_confidence.join(", ")
            ),
        ));
    }

    let params = CreateEvidenceParams {
        dataset_id: request.dataset_id.clone(),
        adapter_id: request.adapter_id.clone(),
        evidence_type: request.evidence_type.clone(),
        reference: request.reference.clone(),
        description: request.description.clone(),
        confidence: request.confidence.clone(),
        created_by: Some(claims.sub.clone()),
        metadata_json: request.metadata_json.clone(),
    };

    let entry_id = state
        .db
        .create_evidence_entry_with_params(&params)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create evidence entry");
            log_failure(
                &state.db,
                &claims,
                actions::ADAPTER_REGISTER,
                resources::ADAPTER,
                None,
                &format!("Failed to create evidence: {}", e),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create evidence entry: {}", e),
            )
        })?;

    // Retrieve the created entry
    let entry = state
        .db
        .get_evidence_entry(&entry_id)
        .await
        .map_err(|e| {
            error!(error = %e, entry_id = %entry_id, "Failed to retrieve evidence entry");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve evidence entry: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Evidence entry not found after creation".to_string(),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_REGISTER,
        resources::ADAPTER,
        Some(&entry_id),
    )
    .await;

    info!(
        entry_id = %entry_id,
        dataset_id = ?request.dataset_id,
        adapter_id = ?request.adapter_id,
        evidence_type = %request.evidence_type,
        user = %claims.sub,
        "Evidence entry created"
    );

    Ok((StatusCode::CREATED, Json(EvidenceResponse::from(entry))))
}

/// Get a single evidence entry by ID
#[utoipa::path(
    get,
    path = "/v1/evidence/{id}",
    params(
        ("id" = String, Path, description = "Evidence entry ID")
    ),
    responses(
        (status = 200, description = "Evidence entry details", body = EvidenceResponse),
        (status = 404, description = "Evidence entry not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn get_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require permission to view evidence
    require_permission(&claims, Permission::AdapterView)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    let entry = state
        .db
        .get_evidence_entry(&id)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id, "Failed to get evidence entry");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get evidence entry: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Evidence entry not found: {}", id),
            )
        })?;

    Ok(Json(EvidenceResponse::from(entry)))
}

/// Delete an evidence entry
#[utoipa::path(
    delete,
    path = "/v1/evidence/{id}",
    params(
        ("id" = String, Path, description = "Evidence entry ID")
    ),
    responses(
        (status = 204, description = "Evidence entry deleted"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Evidence entry not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn delete_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require admin permission to delete evidence
    require_permission(&claims, Permission::AdapterDelete)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Verify entry exists first
    let _entry = state
        .db
        .get_evidence_entry(&id)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id, "Failed to get evidence entry");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get evidence entry: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Evidence entry not found: {}", id),
            )
        })?;

    state.db.delete_evidence_entry(&id).await.map_err(|e| {
        error!(error = %e, id = %id, "Failed to delete evidence entry");
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_DELETE,
            resources::ADAPTER,
            Some(&id),
            &format!("Failed to delete evidence: {}", e),
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete evidence entry: {}", e),
        )
    })?;

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_DELETE,
        resources::ADAPTER,
        Some(&id),
    )
    .await;

    info!(
        entry_id = %id,
        user = %claims.sub,
        "Evidence entry deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// Get evidence entries for a specific dataset
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/evidence",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "List of evidence entries for dataset", body = Vec<EvidenceResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn get_dataset_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require permission to view datasets
    require_permission(&claims, Permission::TrainingView)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    let entries = state
        .db
        .get_dataset_evidence(&dataset_id)
        .await
        .map_err(|e| {
            error!(error = %e, dataset_id = %dataset_id, "Failed to get dataset evidence");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset evidence: {}", e),
            )
        })?;

    let responses: Vec<EvidenceResponse> = entries.into_iter().map(Into::into).collect();
    debug!(
        dataset_id = %dataset_id,
        count = responses.len(),
        "Retrieved dataset evidence"
    );

    Ok(Json(responses))
}

/// Get evidence entries for a specific adapter
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/evidence",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "List of evidence entries for adapter", body = Vec<EvidenceResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "evidence",
    security(("bearer_auth" = []))
)]
pub async fn get_adapter_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Require permission to view adapters
    require_permission(&claims, Permission::AdapterView)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    let entries = state
        .db
        .get_adapter_evidence(&adapter_id)
        .await
        .map_err(|e| {
            error!(error = %e, adapter_id = %adapter_id, "Failed to get adapter evidence");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get adapter evidence: {}", e),
            )
        })?;

    let responses: Vec<EvidenceResponse> = entries.into_iter().map(Into::into).collect();
    debug!(
        adapter_id = %adapter_id,
        count = responses.len(),
        "Retrieved adapter evidence"
    );

    Ok(Json(responses))
}
