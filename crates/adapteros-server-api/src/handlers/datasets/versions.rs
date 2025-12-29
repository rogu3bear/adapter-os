//! Dataset version handlers.

use super::types::{CreateDatasetVersionRequest, CreateDatasetVersionResponse};
use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, forbidden, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{DatasetVersionSummary, DatasetVersionsResponse, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

/// List all versions for a dataset (ordered latest-first) with effective trust_state.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Dataset versions", body = DatasetVersionsResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_dataset_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Ensure dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let versions = state
        .db
        .list_dataset_versions_for_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to list dataset versions: {}", e)))?;

    let summaries: Vec<DatasetVersionSummary> = versions
        .into_iter()
        .map(|(version, trust_state)| DatasetVersionSummary {
            dataset_version_id: version.id,
            version_number: version.version_number,
            version_label: version.version_label,
            hash_b3: Some(version.hash_b3),
            storage_path: Some(version.storage_path),
            trust_state: Some(trust_state),
            created_at: version.created_at,
        })
        .collect();

    Ok(Json(DatasetVersionsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        dataset_id,
        versions: summaries,
    }))
}

/// Create a dataset version explicitly (e.g., to pin a manifest before training).
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = CreateDatasetVersionRequest,
    responses(
        (status = 200, description = "Dataset version created", body = CreateDatasetVersionResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<CreateDatasetVersionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let manifest_json = if let Some(v) = body.manifest_json {
        Some(
            serde_json::to_string(&v)
                .map_err(|e| bad_request(format!("invalid manifest_json: {}", e)))?,
        )
    } else {
        None
    };

    let version_id = state
        .db
        .create_training_dataset_version(
            &dataset_id,
            dataset.tenant_id.as_deref(),
            body.version_label.as_deref(),
            &dataset.storage_path,
            &dataset.hash_b3,
            body.manifest_path.as_deref(),
            manifest_json.as_deref(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| db_error(format!("Failed to create dataset version: {}", e)))?;

    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to fetch created dataset version: {}", e)))?
        .ok_or_else(|| {
            crate::error_helpers::internal_error("Dataset version was created but not found")
        })?;

    Ok(Json(CreateDatasetVersionResponse {
        dataset_id,
        dataset_version_id: version_id,
        version_number: version.version_number,
        trust_state: version.trust_state,
        created_at: version.created_at,
    }))
}

use axum::extract::Query;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query parameters for listing dataset versions
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct ListVersionsQuery {
    /// Maximum number of versions to return (default: 50, max: 100)
    pub limit: Option<i64>,
    /// Number of versions to skip for pagination
    pub offset: Option<i64>,
    /// Filter by trust state (e.g., "allowed", "blocked", "needs_approval")
    pub trust_state: Option<String>,
}

/// Response for a single dataset version with full details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionDetailResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub version_number: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,
    pub hash_b3: String,
    pub storage_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub validation_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    pub pii_status: String,
    pub toxicity_status: String,
    pub leak_status: String,
    pub anomaly_status: String,
    pub overall_safety_status: String,
    pub trust_state: String,
    pub overall_trust_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_at: Option<String>,
}

fn schema_version() -> String {
    adapteros_api_types::API_SCHEMA_VERSION.to_string()
}

/// Response for listing versions by codebase
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CodebaseVersionsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The codebase source location (e.g., repo path)
    pub codebase_id: String,
    /// Dataset ID if a codebase dataset exists
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// List of versions for this codebase
    pub versions: Vec<DatasetVersionSummary>,
    /// Total count of versions (for pagination)
    pub total_count: i64,
}

/// Get a specific dataset version by ID or revision number.
///
/// The `revision` parameter can be:
/// - A version ID (UUID string)
/// - A version number (integer, e.g., "1", "2", "latest")
/// - "latest" to get the most recent version
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions/{revision}",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("revision" = String, Path, description = "Version ID, version number, or 'latest'")
    ),
    responses(
        (status = 200, description = "Dataset version details", body = DatasetVersionDetailResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, revision)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Ensure dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Resolve the revision to a version
    let version = if revision.to_lowercase() == "latest" {
        // Get the latest version
        state
            .db
            .get_latest_dataset_version_for_dataset(&dataset_id)
            .await
            .map_err(|e| db_error(format!("Failed to load latest version: {}", e)))?
            .ok_or_else(|| not_found("Dataset version"))?
    } else if let Ok(version_number) = revision.parse::<i64>() {
        // Lookup by version number
        let versions = state
            .db
            .list_dataset_versions_for_dataset(&dataset_id)
            .await
            .map_err(|e| db_error(format!("Failed to list versions: {}", e)))?;

        versions
            .into_iter()
            .find(|(v, _)| v.version_number == version_number)
            .map(|(v, _)| v)
            .ok_or_else(|| not_found("Dataset version"))?
    } else {
        // Assume it's a version ID
        let version = state
            .db
            .get_training_dataset_version(&revision)
            .await
            .map_err(|e| db_error(format!("Failed to load version: {}", e)))?
            .ok_or_else(|| not_found("Dataset version"))?;

        // Verify the version belongs to this dataset
        if version.dataset_id != dataset_id {
            return Err(not_found("Dataset version"));
        }
        version
    };

    // Parse validation errors if present
    let validation_errors = version
        .validation_errors_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok());

    Ok(Json(DatasetVersionDetailResponse {
        schema_version: schema_version(),
        dataset_id: version.dataset_id,
        dataset_version_id: version.id,
        version_number: version.version_number,
        version_label: version.version_label,
        hash_b3: version.hash_b3,
        storage_path: version.storage_path,
        manifest_path: version.manifest_path,
        validation_status: version.validation_status,
        validation_errors,
        pii_status: version.pii_status,
        toxicity_status: version.toxicity_status,
        leak_status: version.leak_status,
        anomaly_status: version.anomaly_status,
        overall_safety_status: version.overall_safety_status,
        trust_state: version.trust_state,
        overall_trust_status: version.overall_trust_status,
        sensitivity: version.sensitivity,
        created_at: version.created_at,
        created_by: version.created_by,
        locked_at: version.locked_at,
    }))
}

/// List dataset versions by codebase source location.
///
/// This endpoint finds the dataset associated with a codebase (by source_location)
/// and returns all its versions. Useful for codebase adapter workflows.
#[utoipa::path(
    get,
    path = "/v1/datasets/by-codebase/{codebase_id}/versions",
    params(
        ("codebase_id" = String, Path, description = "Codebase identifier (URL-encoded source location, e.g., repo path)"),
        ListVersionsQuery
    ),
    responses(
        (status = 200, description = "Codebase dataset versions", body = CodebaseVersionsResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "No dataset found for codebase"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_versions_by_codebase(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(codebase_id): Path<String>,
    Query(params): Query<ListVersionsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    // URL-decode the codebase_id (it may contain slashes, etc.)
    let source_location = urlencoding::decode(&codebase_id)
        .map_err(|e| bad_request(format!("Invalid codebase_id encoding: {}", e)))?
        .into_owned();

    // Find the codebase dataset by source_location
    let dataset = state
        .db
        .get_codebase_dataset_by_repo(&source_location, None)
        .await
        .map_err(|e| db_error(format!("Failed to find codebase dataset: {}", e)))?;

    let (dataset_id, versions) = if let Some(dataset) = dataset {
        // Enforce tenant isolation
        if let Some(ref dataset_tenant_id) = dataset.tenant_id {
            validate_tenant_isolation(&claims, dataset_tenant_id)?;
        } else if claims.role != "admin" {
            return Err(forbidden(
                "Access denied: dataset has no tenant association",
            ));
        }

        let all_versions = state
            .db
            .list_dataset_versions_for_dataset(&dataset.id)
            .await
            .map_err(|e| db_error(format!("Failed to list dataset versions: {}", e)))?;

        (Some(dataset.id), all_versions)
    } else {
        // No dataset found for this codebase - return empty list
        (None, Vec::new())
    };

    // Apply trust_state filter if provided
    let filtered_versions: Vec<_> = versions
        .into_iter()
        .filter(|(_, trust_state)| {
            if let Some(ref filter) = params.trust_state {
                trust_state.to_lowercase() == filter.to_lowercase()
            } else {
                true
            }
        })
        .collect();

    let total_count = filtered_versions.len() as i64;

    // Apply pagination
    let paginated: Vec<DatasetVersionSummary> = filtered_versions
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|(version, trust_state)| DatasetVersionSummary {
            dataset_version_id: version.id,
            version_number: version.version_number,
            version_label: version.version_label,
            hash_b3: Some(version.hash_b3),
            storage_path: Some(version.storage_path),
            trust_state: Some(trust_state),
            created_at: version.created_at,
        })
        .collect();

    Ok(Json(CodebaseVersionsResponse {
        schema_version: schema_version(),
        codebase_id: source_location,
        dataset_id,
        versions: paginated,
        total_count,
    }))
}
