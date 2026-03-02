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
    extract::{Extension, Path, Query, State},
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

/// Paging parameters for repository listings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct ListReposPaging {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Adapter version response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterVersionResponse {
    pub id: String,
    pub repo_id: String,
    pub tenant_id: String,
    pub version: String,
    pub branch: String,
    pub branch_classification: String,
    pub aos_path: Option<String>,
    pub aos_hash: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub parent_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
    pub training_backend: Option<String>,
    pub coreml_used: bool,
    pub coreml_device_type: Option<String>,
    pub adapter_trust_state: String,
    pub release_state: String,
    pub metrics_snapshot_id: Option<String>,
    pub evaluation_summary: Option<String>,
    pub created_at: String,
    pub attach_mode: String,
    pub required_scope_dataset_version_id: Option<String>,
    pub is_archived: bool,
    pub published_at: Option<String>,
    pub short_description: Option<String>,
    /// Human-readable display name derived from the version's typed ID word alias.
    /// Populated when the ID uses the TypedId format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Promote version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PromoteVersionRequest {
    pub actor: Option<String>,
    pub reason: Option<String>,
}

/// Rollback version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RollbackVersionRequest {
    pub target_version_id: String,
    pub actor: Option<String>,
    pub reason: Option<String>,
}

/// Tag version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TagVersionRequest {
    pub tag_name: String,
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
    Query(paging): Query<ListReposPaging>,
) -> Result<Json<Vec<RepoSummaryResponse>>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;

    let limit = paging.limit.unwrap_or(100).min(500);
    let offset = paging.offset.unwrap_or(0);
    let start = std::time::Instant::now();

    let repos = state
        .db
        .list_adapter_repositories_paged(&claims.tenant_id, None, None, Some(limit), Some(offset))
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

    let elapsed_ms = start.elapsed().as_millis() as f64;
    let _ = state
        .metrics_registry
        .record_metric("http.list_repos.duration_ms".to_string(), elapsed_ms)
        .await;
    if elapsed_ms > 200.0 {
        tracing::warn!(
            tenant_id = %claims.tenant_id,
            elapsed_ms,
            limit,
            offset,
            "list_repos exceeded latency budget (200ms)"
        );
    }

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
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

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

    tracing::info!(
        target: "audit.repos",
        repo_id = %repo_id,
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        name = %req.name,
        action = "create",
        "Repository created"
    );

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
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

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

        let pool = state.db.pool_result()?;
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

    tracing::info!(
        target: "audit.repos",
        repo_id = %repo_id,
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        action = "update",
        "Repository updated"
    );

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

// Version management handlers

/// List all versions for a repository
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos/{repo_id}/versions",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "List of adapter versions", body = Vec<AdapterVersionResponse>),
        (status = 404, description = "Repository not found")
    )
)]
pub async fn list_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<AdapterVersionResponse>>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

    // Verify repository exists and belongs to tenant
    let _repo = state
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

    // List all versions for this repository
    let versions = state
        .db
        .list_adapter_versions_for_repo(&claims.tenant_id, &repo_id, None, None)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to list versions: {}", e),
            )
        })?;

    info!(
        repo_id = %repo_id,
        tenant_id = %claims.tenant_id,
        count = versions.len(),
        "Listed adapter versions"
    );

    let response: Vec<AdapterVersionResponse> = versions
        .into_iter()
        .map(|v| {
            let display_name = adapteros_id::display_name_for(&v.id);
            AdapterVersionResponse {
                id: v.id,
                repo_id: v.repo_id,
                tenant_id: v.tenant_id,
                version: v.version,
                branch: v.branch,
                branch_classification: v.branch_classification,
                aos_path: v.aos_path,
                aos_hash: v.aos_hash,
                manifest_schema_version: v.manifest_schema_version,
                parent_version_id: v.parent_version_id,
                code_commit_sha: v.code_commit_sha,
                data_spec_hash: v.data_spec_hash,
                training_backend: v.training_backend,
                coreml_used: v.coreml_used,
                coreml_device_type: v.coreml_device_type,
                adapter_trust_state: v.adapter_trust_state,
                release_state: v.release_state,
                metrics_snapshot_id: v.metrics_snapshot_id,
                evaluation_summary: v.evaluation_summary,
                created_at: v.created_at,
                attach_mode: v.attach_mode,
                required_scope_dataset_version_id: v.required_scope_dataset_version_id,
                is_archived: v.is_archived,
                published_at: v.published_at,
                short_description: v.short_description,
                display_name,
            }
        })
        .collect();

    Ok(Json(response))
}

/// Get a specific adapter version
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos/{repo_id}/versions/{version_id}",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Version ID")
    ),
    responses(
        (status = 200, description = "Adapter version details", body = AdapterVersionResponse),
        (status = 404, description = "Version not found")
    )
)]
pub async fn get_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
) -> Result<Json<AdapterVersionResponse>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id).await?;

    // Verify repository exists and belongs to tenant
    let _repo = state
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

    // Get the specific version
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch version: {}", e),
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "VERSION_NOT_FOUND",
                format!("Version {} not found", version_id),
            )
        })?;

    // Verify version belongs to the requested repository
    if version.repo_id != repo_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "VERSION_REPO_MISMATCH",
            format!(
                "Version {} does not belong to repository {}",
                version_id, repo_id
            ),
        ));
    }

    info!(
        version_id = %version_id,
        repo_id = %repo_id,
        tenant_id = %claims.tenant_id,
        "Retrieved adapter version"
    );

    let display_name = adapteros_id::display_name_for(&version.id);
    Ok(Json(AdapterVersionResponse {
        id: version.id,
        repo_id: version.repo_id,
        tenant_id: version.tenant_id,
        version: version.version,
        branch: version.branch,
        branch_classification: version.branch_classification,
        aos_path: version.aos_path,
        aos_hash: version.aos_hash,
        manifest_schema_version: version.manifest_schema_version,
        parent_version_id: version.parent_version_id,
        code_commit_sha: version.code_commit_sha,
        data_spec_hash: version.data_spec_hash,
        training_backend: version.training_backend,
        coreml_used: version.coreml_used,
        coreml_device_type: version.coreml_device_type,
        adapter_trust_state: version.adapter_trust_state,
        release_state: version.release_state,
        metrics_snapshot_id: version.metrics_snapshot_id,
        evaluation_summary: version.evaluation_summary,
        created_at: version.created_at,
        attach_mode: version.attach_mode,
        required_scope_dataset_version_id: version.required_scope_dataset_version_id,
        is_archived: version.is_archived,
        published_at: version.published_at,
        short_description: version.short_description,
        display_name,
    }))
}

/// Promote an adapter version to active
#[utoipa::path(
    tag = "repositories",
    post,
    path = "/v1/repos/{repo_id}/versions/{version_id}/promote",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Version ID to promote")
    ),
    request_body = PromoteVersionRequest,
    responses(
        (status = 200, description = "Version promoted successfully"),
        (status = 404, description = "Version not found"),
        (status = 400, description = "Version cannot be promoted")
    )
)]
pub async fn promote_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
    Json(req): Json<PromoteVersionRequest>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id).await?;

    // Verify repository exists and belongs to tenant
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

    // Check if repository is archived
    if repo.archived != 0 {
        return Err(ApiError::repo_archived(&repo_id));
    }

    // Promote the version
    state
        .db
        .promote_adapter_version(
            &claims.tenant_id,
            &repo_id,
            &version_id,
            req.actor.as_deref().or(Some(&claims.sub)),
            req.reason.as_deref(),
        )
        .await
        .map_err(|e| {
            let error_str = e.to_string();

            // Map specific errors to appropriate HTTP errors
            if error_str.contains("not found") || error_str.contains("NotFound") {
                return ApiError::new(
                    StatusCode::NOT_FOUND,
                    "VERSION_NOT_FOUND",
                    format!("Version {} not found", version_id),
                );
            }

            if error_str.contains("Validation") || error_str.contains("requires") {
                return ApiError::new(StatusCode::BAD_REQUEST, "PROMOTION_FAILED", error_str);
            }

            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to promote version: {}", error_str),
            )
        })?;

    tracing::info!(
        target: "audit.repos",
        version_id = %version_id,
        repo_id = %repo_id,
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        actor = req.actor.as_deref().unwrap_or(&claims.sub),
        action = "promote_version",
        "Promoted adapter version"
    );

    Ok(StatusCode::OK)
}

/// Rollback a branch to a previous version
#[utoipa::path(
    tag = "repositories",
    post,
    path = "/v1/repos/{repo_id}/branches/{branch}/rollback",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("branch" = String, Path, description = "Branch name")
    ),
    request_body = RollbackVersionRequest,
    responses(
        (status = 200, description = "Branch rolled back successfully"),
        (status = 404, description = "Repository or version not found"),
        (status = 400, description = "Rollback failed")
    )
)]
pub async fn rollback_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, branch)): Path<(String, String)>,
    Json(req): Json<RollbackVersionRequest>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

    // Verify repository exists and belongs to tenant
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

    // Check if repository is archived
    if repo.archived != 0 {
        return Err(ApiError::repo_archived(&repo_id));
    }

    // Rollback the branch
    state
        .db
        .rollback_adapter_branch(
            &claims.tenant_id,
            &repo_id,
            &branch,
            &req.target_version_id,
            req.actor.as_deref().or(Some(&claims.sub)),
            req.reason.as_deref(),
        )
        .await
        .map_err(|e| {
            let error_str = e.to_string();

            // Map specific errors to appropriate HTTP errors
            if error_str.contains("not found") || error_str.contains("NotFound") {
                return ApiError::new(
                    StatusCode::NOT_FOUND,
                    "VERSION_NOT_FOUND",
                    format!("Target version {} not found", req.target_version_id),
                );
            }

            if error_str.contains("Validation") || error_str.contains("must") {
                return ApiError::new(StatusCode::BAD_REQUEST, "ROLLBACK_FAILED", error_str);
            }

            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to rollback branch: {}", error_str),
            )
        })?;

    tracing::info!(
        target: "audit.repos",
        target_version_id = %req.target_version_id,
        branch = %branch,
        repo_id = %repo_id,
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        actor = req.actor.as_deref().unwrap_or(&claims.sub),
        action = "rollback_version",
        "Rolled back adapter branch"
    );

    Ok(StatusCode::OK)
}

/// Get version history timeline for a repository
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos/{repo_id}/timeline",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Version history timeline", body = Vec<RepoTimelineEventResponse>),
        (status = 404, description = "Repository not found")
    )
)]
pub async fn get_timeline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<RepoTimelineEventResponse>>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

    // Verify repository exists and belongs to tenant
    let _repo = state
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

    // Query version history from database
    let history = state
        .db
        .list_version_history_for_repo(&claims.tenant_id, &repo_id, Some(100))
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch version history: {}", e),
            )
        })?;

    // Convert to timeline events
    let timeline_events: Vec<RepoTimelineEventResponse> = history
        .into_iter()
        .map(|record| {
            let event_type = format!("state_change:{}", record.new_state);
            let description = match (&record.old_state, &record.reason) {
                (Some(old), Some(reason)) => {
                    format!("{} → {} ({})", old, record.new_state, reason)
                }
                (Some(old), None) => format!("{} → {}", old, record.new_state),
                (None, Some(reason)) => format!("Created as {} ({})", record.new_state, reason),
                (None, None) => format!("Created as {}", record.new_state),
            };

            RepoTimelineEventResponse {
                id: record.id,
                timestamp: record.created_at,
                event_type,
                description,
            }
        })
        .collect();

    info!(
        repo_id = %repo_id,
        tenant_id = %claims.tenant_id,
        event_count = timeline_events.len(),
        "Retrieved repository timeline"
    );

    Ok(Json(timeline_events))
}

/// List training jobs associated with a repository
#[utoipa::path(
    tag = "repositories",
    get,
    path = "/v1/repos/{repo_id}/training-jobs",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "List of training jobs", body = Vec<RepoTrainingJobLinkResponse>),
        (status = 404, description = "Repository not found")
    )
)]
pub async fn list_training_jobs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<RepoTrainingJobLinkResponse>>, ApiError> {
    require_permission(&claims, Permission::AdapterList)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;

    // Verify repository exists and belongs to tenant
    let _repo = state
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

    // List training jobs for this repository
    let jobs = state.db.list_training_jobs(&repo_id).await.map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            format!("failed to list training jobs: {}", e),
        )
    })?;

    info!(
        repo_id = %repo_id,
        tenant_id = %claims.tenant_id,
        count = jobs.len(),
        "Listed training jobs for repository"
    );

    let response: Vec<RepoTrainingJobLinkResponse> = jobs
        .into_iter()
        .map(|job| RepoTrainingJobLinkResponse {
            job_id: job.id,
            status: job.status,
            created_at: job.started_at,
        })
        .collect();

    Ok(Json(response))
}

/// Tag an adapter version
#[utoipa::path(
    tag = "repositories",
    post,
    path = "/v1/repos/{repo_id}/versions/{version_id}/tag",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Version ID to tag")
    ),
    request_body = TagVersionRequest,
    responses(
        (status = 200, description = "Version tagged successfully"),
        (status = 404, description = "Version not found"),
        (status = 400, description = "Invalid tag name")
    )
)]
pub async fn tag_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
    Json(req): Json<TagVersionRequest>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id).await?;

    // Verify repository exists and belongs to tenant
    let _repo = state
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

    // Validate tag name
    if req.tag_name.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "tag_name cannot be empty",
        ));
    }

    // Tag the version
    state
        .db
        .upsert_adapter_version_tag(&claims.tenant_id, &version_id, &req.tag_name)
        .await
        .map_err(|e| {
            let error_str = e.to_string();

            // Map specific errors to appropriate HTTP errors
            if error_str.contains("not found") || error_str.contains("NotFound") {
                return ApiError::new(
                    StatusCode::NOT_FOUND,
                    "VERSION_NOT_FOUND",
                    format!("Version {} not found", version_id),
                );
            }

            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to tag version: {}", error_str),
            )
        })?;

    tracing::info!(
        target: "audit.repos",
        version_id = %version_id,
        repo_id = %repo_id,
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        tag_name = %req.tag_name,
        action = "tag_version",
        "Tagged adapter version"
    );

    Ok(StatusCode::OK)
}

/// Start training from a specific adapter version
#[utoipa::path(
    tag = "repositories",
    post,
    path = "/v1/repos/{repo_id}/versions/{version_id}/train",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Version ID to use as base")
    ),
    responses(
        (status = 200, description = "Training job started successfully"),
        (status = 404, description = "Repository or version not found"),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn start_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::TrainingStart)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id).await?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id).await?;

    // Verify repository exists and belongs to tenant
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

    // Check if repository is archived
    if repo.archived != 0 {
        return Err(ApiError::repo_archived(&repo_id));
    }

    // Verify version exists and belongs to this repository
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                format!("failed to fetch version: {}", e),
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "VERSION_NOT_FOUND",
                format!("Version {} not found", version_id),
            )
        })?;

    // Verify version belongs to this repository
    if version.repo_id != repo_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "VERSION_REPO_MISMATCH",
            format!(
                "Version {} does not belong to repository {}",
                version_id, repo_id
            ),
        ));
    }

    // Version-based training workflow:
    // 1. Create a draft version based on the specified version
    // 2. Return success with the draft version ID
    // The user should then use POST /v1/training/jobs with base_version_id to start training

    // Use the version's branch
    let target_branch = version.branch.clone();

    // Create a draft version based on the specified version
    let draft_version_id: String = state
        .db
        .create_adapter_draft_version(adapteros_db::CreateDraftVersionParams {
            repo_id: &repo_id,
            tenant_id: &claims.tenant_id,
            branch: &target_branch,
            branch_classification: &version.branch_classification,
            parent_version_id: Some(&version_id),
            code_commit_sha: version.code_commit_sha.as_deref(),
            data_spec_hash: version.data_spec_hash.as_deref(),
            training_backend: version.training_backend.as_deref(),
            dataset_version_ids: None,
            actor: Some(&claims.sub),
            reason: Some(&format!("Created from version {}", version.version)),
        })
        .await
        .map_err(|e| {
            tracing::error!(
                version_id = %version_id,
                repo_id = %repo_id,
                error = %e,
                "Failed to create draft version for training"
            );
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DRAFT_VERSION_FAILED",
                format!("Failed to create draft version: {}", e),
            )
        })?;

    info!(
        draft_version_id = %draft_version_id,
        base_version_id = %version_id,
        repo_id = %repo_id,
        tenant_id = %claims.tenant_id,
        "Created draft version for training from version"
    );

    Ok(StatusCode::OK)
}
