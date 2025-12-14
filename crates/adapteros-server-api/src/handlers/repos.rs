//! Repository handlers for the /v1/repos API
//!
//! Provides REST endpoints for managing adapter repositories, versions,
//! and associated training jobs.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

/// Branch summary with version count
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BranchSummary {
    pub branch: String,
    pub version_count: usize,
    pub latest_version: Option<String>,
    pub latest_release_state: Option<String>,
}

/// Repository summary response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RepoSummaryResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub default_branch: String,
    pub archived: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub description: Option<String>,
}

/// Repository detail response with full metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RepoDetailResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub default_branch: String,
    pub archived: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub description: Option<String>,
}

/// Create repository request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateRepoRequest {
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub description: Option<String>,
    pub default_branch: Option<String>,
}

/// Create repository response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateRepoResponse {
    pub repo_id: String,
}

/// Update repository request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRepoRequest {
    pub description: Option<String>,
    pub default_branch: Option<String>,
}

/// Timeline event response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RepoTimelineEventResponse {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub description: String,
}

/// Training job link response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RepoTrainingJobLinkResponse {
    pub job_id: String,
    pub status: String,
    pub created_at: String,
}

/// List all repositories for the authenticated tenant
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos",
    responses(
        (status = 200, description = "List of repositories", body = Vec<RepoSummaryResponse>),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_repos(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<RepoSummaryResponse>>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;

    let repos = state
        .db
        .list_adapter_repositories(&claims.tenant_id, None, None)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to list repositories: {}", e),
            )
        })?;

    info!(tenant_id = %claims.tenant_id, count = repos.len(), "Listed repositories");

    let response: Vec<RepoSummaryResponse> = repos
        .into_iter()
        .map(|repo| RepoSummaryResponse {
            id: repo.id,
            tenant_id: repo.tenant_id,
            name: repo.name,
            base_model_id: repo.base_model_id,
            default_branch: repo.default_branch,
            archived: repo.archived != 0,
            created_by: repo.created_by,
            created_at: repo.created_at,
            description: repo.description,
        })
        .collect();

    Ok(Json(response))
}

/// Get repository details
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos/{repo_id}",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Repository details", body = RepoDetailResponse),
        (status = 404, description = "Repository not found")
    )
)]
pub async fn get_repo(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch repository: {}", e),
            )
        })?
        .ok_or_else(|| ApiError::repo_not_found(&repo_id))?;

    info!(repo_id = %repo_id, tenant_id = %claims.tenant_id, "Retrieved repository");

    Ok(Json(RepoDetailResponse {
        id: repo.id,
        tenant_id: repo.tenant_id,
        name: repo.name,
        base_model_id: repo.base_model_id,
        default_branch: repo.default_branch,
        archived: repo.archived != 0,
        created_by: repo.created_by,
        created_at: repo.created_at,
        description: repo.description,
    }))
}

/// Create a new repository
#[utoipa::path(
    tag = "repositories",
    post,
    path = "/v1/repos",
    request_body = CreateRepoRequest,
    responses(
        (status = 201, description = "Repository created", body = CreateRepoResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Repository already exists")
    )
)]
pub async fn create_repo(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<(StatusCode, Json<CreateRepoResponse>), ApiError> {
    // Check permission
    require_permission(&claims, Permission::AdapterRegister)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &req.tenant_id)?;

    // Validate request fields
    if req.name.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "repository name cannot be empty",
        ));
    }

    let base_model = req.base_model_id.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "base_model_id is required",
        )
    })?;

    if base_model.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "base_model_id cannot be empty",
        ));
    }

    // Create repository
    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: &req.name,
            base_model_id: Some(base_model.as_str()),
            default_branch: req.default_branch.as_deref(),
            created_by: Some(&claims.sub),
            description: req.description.as_deref(),
        })
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            // Check for duplicate name error
            if error_str.contains("UNIQUE constraint failed") {
                return ApiError::repo_already_exists(&req.name);
            }
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "DATABASE_ERROR",
                format!("failed to create repository: {}", error_str),
            )
        })?;

    info!(repo_id = %repo_id, name = %req.name, tenant_id = %claims.tenant_id, "Repository created");

    Ok((StatusCode::CREATED, Json(CreateRepoResponse { repo_id })))
}

/// Update repository metadata
#[utoipa::path(
    tag = "repositories",
    patch,
    path = "/v1/repos/{repo_id}",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    request_body = UpdateRepoRequest,
    responses(
        (status = 200, description = "Repository updated", body = RepoDetailResponse),
        (status = 404, description = "Repository not found"),
        (status = 403, description = "Repository is archived")
    )
)]
pub async fn update_repo(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Json(req): Json<UpdateRepoRequest>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    // Check permission
    require_permission(&claims, Permission::AdapterRegister)?;

    // First get the repo to verify it exists and belongs to tenant
    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch repository: {}", e),
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("Repository {} not found", repo_id),
            )
        })?;

    // Check if archived
    if repo.archived == 1 {
        return Err(ApiError::repo_archived(&repo_id));
    }

    // Build UPDATE query with only provided fields
    let mut updates = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(description) = &req.description {
        updates.push("description = ?");
        values.push(description.clone());
    }

    if let Some(default_branch) = &req.default_branch {
        updates.push("default_branch = ?");
        values.push(default_branch.clone());
    }

    // Only execute update if there are fields to update
    if !updates.is_empty() {
        let update_clause = updates.join(", ");
        let query_str = format!(
            "UPDATE adapter_repositories SET {} WHERE id = ? AND tenant_id = ?",
            update_clause
        );

        let pool = state.db.pool();
        let mut query = sqlx::query(&query_str);

        // Bind values in order
        for value in &values {
            query = query.bind(value);
        }

        // Bind WHERE clause parameters
        query = query.bind(&repo_id).bind(&claims.tenant_id);

        query.execute(pool).await.map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to update repository: {}", e),
            )
        })?;
    }

    // Fetch updated repository
    let updated_repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch updated repository: {}", e),
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "repository not found after update",
            )
        })?;

    info!(repo_id = %repo_id, tenant_id = %claims.tenant_id, "Repository updated");

    Ok(Json(RepoDetailResponse {
        id: updated_repo.id,
        tenant_id: updated_repo.tenant_id,
        name: updated_repo.name,
        base_model_id: updated_repo.base_model_id,
        default_branch: updated_repo.default_branch,
        archived: updated_repo.archived != 0,
        created_by: updated_repo.created_by,
        created_at: updated_repo.created_at,
        description: updated_repo.description,
    }))
}

// Stub handlers for other endpoints required by routes

pub async fn list_versions(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_repo_id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "list_versions not yet implemented",
    ))
}

pub async fn get_version(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_repo_id, _version_id)): Path<(String, String)>,
) -> Result<Json<String>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "get_version not yet implemented",
    ))
}

pub async fn promote_version(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_repo_id, _version_id)): Path<(String, String)>,
) -> Result<Json<String>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "promote_version not yet implemented",
    ))
}

pub async fn rollback_version(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_repo_id, _branch)): Path<(String, String)>,
) -> Result<Json<String>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "rollback_version not yet implemented",
    ))
}

pub async fn get_timeline(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_repo_id): Path<String>,
) -> Result<Json<Vec<RepoTimelineEventResponse>>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "get_timeline not yet implemented",
    ))
}

pub async fn list_training_jobs(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_repo_id): Path<String>,
) -> Result<Json<Vec<RepoTrainingJobLinkResponse>>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "list_training_jobs not yet implemented",
    ))
}

pub async fn tag_version(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_repo_id, _version_id)): Path<(String, String)>,
) -> Result<Json<String>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "tag_version not yet implemented",
    ))
}

pub async fn start_training(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_repo_id, _version_id)): Path<(String, String)>,
) -> Result<Json<String>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "start_training not yet implemented",
    ))
}
