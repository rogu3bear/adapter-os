//! Evidence Explorer API handlers (PRD-DATA-01 Phase 2)
//!
//! Provides REST endpoints for managing evidence entries linked to datasets and adapters.

use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::error_helpers::{db_error, internal_error, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view evidence
    require_permission(&claims, Permission::AdapterView)?;

    // CRITICAL: Validate tenant isolation for filtered resources
    // If dataset_id filter is provided, validate tenant owns the dataset
    if let Some(ref dataset_id) = query.dataset_id {
        let dataset = state
            .db
            .get_training_dataset(dataset_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Dataset"))?;

        if let Some(ref tenant_id) = dataset.tenant_id {
            validate_tenant_isolation(&claims, tenant_id)?;
        }
    }

    // If adapter_id filter is provided, validate tenant owns the adapter
    if let Some(ref adapter_id) = query.adapter_id {
        // get_adapter_by_id enforces tenant isolation by requiring tenant_id
        let _adapter = state
            .db
            .get_adapter_by_id(&claims.tenant_id, adapter_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Adapter"))?;
    }

    // NOTE: If no dataset_id or adapter_id filter is provided, this query could potentially
    // return evidence entries from other tenants. The database layer should be enhanced
    // to filter by tenant_id via JOINs with datasets/adapters tables.
    // For now, callers should always provide dataset_id or adapter_id filters.

    let filter = EvidenceFilter {
        dataset_id: query.dataset_id,
        adapter_id: query.adapter_id,
        evidence_type: query.evidence_type,
        confidence: query.confidence,
        limit: query.limit,
    };

    let entries = state
        .db
        .list_evidence_entries(&filter)
        .await
        .map_err(db_error)?;

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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to register adapters/datasets
    require_permission(&claims, Permission::AdapterRegister)?;

    // Validate at least one ID is provided
    if request.dataset_id.is_none() && request.adapter_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Either dataset_id or adapter_id must be provided")
                    .with_code("BAD_REQUEST"),
            ),
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
            Json(
                ErrorResponse::new(&format!(
                    "Invalid evidence_type. Must be one of: {}",
                    valid_types.join(", ")
                ))
                .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate confidence level
    let valid_confidence = ["high", "medium", "low"];
    if !valid_confidence.contains(&request.confidence.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(&format!(
                    "Invalid confidence. Must be one of: {}",
                    valid_confidence.join(", ")
                ))
                .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // CRITICAL: Validate tenant isolation for dataset or adapter
    if let Some(ref dataset_id) = request.dataset_id {
        let dataset = state
            .db
            .get_training_dataset(dataset_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Dataset"))?;

        if let Some(ref tenant_id) = dataset.tenant_id {
            validate_tenant_isolation(&claims, tenant_id)?;
        }
    }

    if let Some(ref adapter_id) = request.adapter_id {
        // get_adapter_by_id enforces tenant isolation by requiring tenant_id
        let _adapter = state
            .db
            .get_adapter_by_id(&claims.tenant_id, adapter_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Adapter"))?;
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
            let _ = log_failure(
                &state.db,
                &claims,
                actions::ADAPTER_REGISTER,
                resources::ADAPTER,
                None,
                &format!("Failed to create evidence: {}", e),
            );
            internal_error(e)
        })?;

    // Retrieve the created entry
    let entry = state
        .db
        .get_evidence_entry(&entry_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| internal_error("Evidence entry not found after creation"))?;

    let _ = log_success(
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view evidence
    require_permission(&claims, Permission::AdapterView)?;

    let entry = state
        .db
        .get_evidence_entry(&id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Evidence entry"))?;

    // CRITICAL: Validate tenant isolation via linked dataset or adapter
    if let Some(ref dataset_id) = entry.dataset_id {
        let dataset = state
            .db
            .get_training_dataset(dataset_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Dataset"))?;

        if let Some(ref tenant_id) = dataset.tenant_id {
            validate_tenant_isolation(&claims, tenant_id)?;
        }
    } else if let Some(ref adapter_id) = entry.adapter_id {
        // get_adapter_by_id enforces tenant isolation by requiring tenant_id
        let _adapter = state
            .db
            .get_adapter_by_id(&claims.tenant_id, adapter_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Adapter"))?;
    }

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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require admin permission to delete evidence
    require_permission(&claims, Permission::AdapterDelete)?;

    // Verify entry exists first
    let entry = state
        .db
        .get_evidence_entry(&id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Evidence entry"))?;

    // CRITICAL: Validate tenant isolation via linked dataset or adapter
    if let Some(ref dataset_id) = entry.dataset_id {
        let dataset = state
            .db
            .get_training_dataset(dataset_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Dataset"))?;

        if let Some(ref tenant_id) = dataset.tenant_id {
            validate_tenant_isolation(&claims, tenant_id)?;
        }
    } else if let Some(ref adapter_id) = entry.adapter_id {
        // get_adapter_by_id enforces tenant isolation by requiring tenant_id
        let _adapter = state
            .db
            .get_adapter_by_id(&claims.tenant_id, adapter_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| not_found("Adapter"))?;
    }

    state.db.delete_evidence_entry(&id).await.map_err(|e| {
        error!(error = %e, id = %id, "Failed to delete evidence entry");
        let _ = log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_DELETE,
            resources::ADAPTER,
            Some(&id),
            &format!("Failed to delete evidence: {}", e),
        );
        internal_error(e)
    })?;

    let _ = log_success(
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view datasets
    require_permission(&claims, Permission::TrainingView)?;

    // CRITICAL: Validate tenant isolation - verify dataset belongs to tenant
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, tenant_id)?;
    }

    let entries = state
        .db
        .get_dataset_evidence(&dataset_id)
        .await
        .map_err(db_error)?;

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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require permission to view adapters
    require_permission(&claims, Permission::AdapterView)?;

    // CRITICAL: Validate tenant isolation - verify adapter belongs to tenant
    // get_adapter_by_id enforces tenant isolation by requiring tenant_id
    let _adapter = state
        .db
        .get_adapter_by_id(&claims.tenant_id, &adapter_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found("Adapter"))?;

    let entries = state
        .db
        .get_adapter_evidence(&adapter_id)
        .await
        .map_err(db_error)?;

    let responses: Vec<EvidenceResponse> = entries.into_iter().map(Into::into).collect();
    debug!(
        adapter_id = %adapter_id,
        count = responses.len(),
        "Retrieved adapter evidence"
    );

    Ok(Json(responses))
}
