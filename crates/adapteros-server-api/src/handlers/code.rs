use crate::state::AppState;
use crate::types::*;
use adapteros_deterministic_exec::spawn_deterministic;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Register repository request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRepositoryRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

/// Register repository response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRepositoryResponse {
    pub status: String,
    pub repo_id: String,
    pub message: String,
}

/// Scan repository request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanRepositoryRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub full_scan: bool,
}

/// Scan job response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanJobResponse {
    pub status: String,
    pub job_id: String,
    pub repo_id: String,
    pub commit: String,
    pub estimated_duration_seconds: Option<u32>,
}

/// Scan job status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanJobStatusResponse {
    pub job_id: String,
    pub status: String,
    pub progress: ScanJobProgress,
    pub result: Option<ScanJobResult>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanJobProgress {
    pub current_stage: Option<String>,
    pub percentage: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanJobResult {
    pub code_graph_hash: String,
    pub symbol_index_hash: Option<String>,
    pub vector_index_hash: Option<String>,
    pub test_map_hash: Option<String>,
    pub file_count: i32,
    pub symbol_count: i32,
    pub test_count: i32,
}

/// List repositories query
#[derive(Debug, Deserialize)]
pub struct ListRepositoriesQuery {
    pub page: Option<i32>,
    pub limit: Option<i32>,
}

/// Repository list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RepositoryListResponse {
    pub repos: Vec<RepositoryInfo>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RepositoryInfo {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Pagination {
    pub page: i32,
    pub limit: i32,
    pub total: i64,
}

/// Repository detail response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RepositoryDetailResponse {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub status: String,
    pub latest_graph_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Commit delta request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitDeltaRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub base_commit: String,
    pub head_commit: String,
}

/// Commit delta response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitDeltaResponse {
    pub status: String,
    pub job_id: String,
    pub message: String,
}

/// Register repository
#[utoipa::path(
    post,
    path = "/v1/code/register-repo",
    request_body = RegisterRepositoryRequest,
    responses(
        (status = 202, description = "Repository registered", body = RegisterRepositoryResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 409, description = "Repository already exists", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn register_repo(
    State(state): State<AppState>,
    Json(req): Json<RegisterRepositoryRequest>,
) -> Result<Json<RegisterRepositoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if repository already exists
    let existing = state
        .db
        .get_repository_by_repo_id(&req.tenant_id, &req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("Repository already registered")
                    .with_code("CONFLICT")
                    .with_string_details(&req.repo_id),
            ),
        ));
    }

    // Register repository
    let _repo = state
        .db
        .register_repository(
            &req.tenant_id,
            &req.repo_id,
            &req.path,
            &req.languages,
            &req.default_branch,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    Ok(Json(RegisterRepositoryResponse {
        status: "accepted".to_string(),
        repo_id: req.repo_id.clone(),
        message: "Repository registered successfully".to_string(),
    }))
}

/// Trigger repository scan
#[utoipa::path(
    post,
    path = "/v1/code/scan",
    request_body = ScanRepositoryRequest,
    responses(
        (status = 202, description = "Scan job created", body = ScanJobResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse),
        (status = 409, description = "Scan already in progress", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn scan_repo(
    State(state): State<AppState>,
    Json(req): Json<ScanRepositoryRequest>,
) -> Result<Json<ScanJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get repository
    let repo = state
        .db
        .get_repository_by_repo_id(&req.tenant_id, &req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Repository not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(&req.repo_id),
                ),
            )
        })?;

    // Check for existing running scan
    let existing_jobs = state.db.list_scan_jobs(&repo.id, 10).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    if existing_jobs
        .iter()
        .any(|j| j.status == "pending" || j.status == "running")
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("Scan already in progress").with_code("CONFLICT")),
        ));
    }

    // Create scan job
    let job_id = state
        .db
        .create_scan_job(&repo.id, &req.commit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Spawn background task to execute scan
    if let Some(ref code_job_manager) = state.code_job_manager {
        let manager = code_job_manager.clone();
        let repo_id = repo.id.clone();
        let commit = req.commit.clone();

        tokio::spawn(async move {
            let job = adapteros_orchestrator::ScanRepositoryJob {
                repo_id,
                commit_sha: commit,
                full_scan: true,
            };

            if let Err(e) = manager.execute_scan_job(job).await {
                tracing::error!("Scan job failed: {}", e);
            }
        });
    }

    Ok(Json(ScanJobResponse {
        status: "accepted".to_string(),
        job_id,
        repo_id: req.repo_id.clone(),
        commit: req.commit.clone(),
        estimated_duration_seconds: Some(120),
    }))
}

/// Get scan job status
#[utoipa::path(
    get,
    path = "/v1/code/scan/{job_id}",
    params(
        ("job_id" = String, Path, description = "Scan job ID")
    ),
    responses(
        (status = 200, description = "Job status", body = ScanJobStatusResponse),
        (status = 404, description = "Job not found", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn get_scan_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<ScanJobStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let job = state
        .db
        .get_scan_job(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Job not found").with_code("NOT_FOUND")),
            )
        })?;

    // Get result if completed
    let result = if job.status == "completed" {
        state
            .db
            .get_code_graph_metadata(&job.repo_id, &job.commit_sha)
            .await
            .ok()
            .flatten()
            .map(|meta| ScanJobResult {
                code_graph_hash: meta.hash_b3,
                symbol_index_hash: meta.symbol_index_hash,
                vector_index_hash: meta.vector_index_hash,
                test_map_hash: meta.test_map_hash,
                file_count: meta.file_count,
                symbol_count: meta.symbol_count,
                test_count: meta.test_count,
            })
    } else {
        None
    };

    Ok(Json(ScanJobStatusResponse {
        job_id: job.id,
        status: job.status,
        progress: ScanJobProgress {
            current_stage: job.current_stage,
            percentage: job.progress_pct,
        },
        result,
        started_at: job.started_at,
        completed_at: job.completed_at,
    }))
}

/// List repositories
#[utoipa::path(
    get,
    path = "/v1/code/repositories",
    params(
        ("tenant_id" = String, Query, description = "Tenant ID"),
        ("page" = Option<i32>, Query, description = "Page number"),
        ("limit" = Option<i32>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "Repository list", body = RepositoryListResponse),
        (status = 400, description = "Bad request", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn list_repositories(
    State(state): State<AppState>,
    Query(query): Query<ListRepositoriesQuery>,
) -> Result<Json<RepositoryListResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Default tenant for now
    let tenant_id = "default";
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = (page - 1) * limit;

    let repos = state
        .db
        .list_repositories(tenant_id, limit, offset)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let total = state.db.count_repositories(tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    let repo_infos: Vec<RepositoryInfo> = repos
        .into_iter()
        .map(|r| {
            let languages: Vec<String> = r
                .languages_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            RepositoryInfo {
                repo_id: r.repo_id,
                path: r.path,
                languages,
                default_branch: r.default_branch,
                latest_scan_commit: r.latest_scan_commit,
                latest_scan_at: r.latest_scan_at,
                created_at: r.created_at,
            }
        })
        .collect();

    Ok(Json(RepositoryListResponse {
        repos: repo_infos,
        pagination: Pagination { page, limit, total },
    }))
}

/// Get repository details
#[utoipa::path(
    get,
    path = "/v1/code/repositories/{repo_id}",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("tenant_id" = String, Query, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Repository details", body = RepositoryDetailResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn get_repository(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepositoryDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = "default";

    let repo = state
        .db
        .get_repository_by_repo_id(tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Repository not found").with_code("NOT_FOUND")),
            )
        })?;

    let languages: Vec<String> = repo
        .languages_json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    Ok(Json(RepositoryDetailResponse {
        repo_id: repo.repo_id,
        path: repo.path,
        languages,
        default_branch: repo.default_branch,
        latest_scan_commit: repo.latest_scan_commit,
        latest_scan_at: repo.latest_scan_at,
        status: repo.status,
        latest_graph_hash: repo.latest_graph_hash,
        created_at: repo.created_at,
        updated_at: repo.updated_at,
    }))
}

/// Create commit delta pack
#[utoipa::path(
    post,
    path = "/v1/code/commit-delta",
    request_body = CommitDeltaRequest,
    responses(
        (status = 202, description = "Delta job created", body = CommitDeltaResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    ),
    tag = "code"
)]
pub async fn create_commit_delta(
    State(state): State<AppState>,
    Json(req): Json<CommitDeltaRequest>,
) -> Result<Json<CommitDeltaResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify repository exists
    let _repo = state
        .db
        .get_repository_by_repo_id(&req.tenant_id, &req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Repository not found").with_code("NOT_FOUND")),
            )
        })?;

    // Create job ID
    let job_id = uuid::Uuid::new_v4().to_string();

    // Spawn background task
    if let Some(ref code_job_manager) = state.code_job_manager {
        let manager = code_job_manager.clone();
        let tenant_id = req.tenant_id.clone();
        let repo_id = req.repo_id.clone();
        let base = req.base_commit.clone();
        let head = req.head_commit.clone();

        let _ = spawn_deterministic("Commit Delta Job".to_string(), async move {
            let job = adapteros_orchestrator::CommitDeltaJob {
                tenant_id,
                repo_id,
                base_commit: base,
                head_commit: head,
            };

            if let Err(e) = manager.execute_commit_delta_job(job).await {
                tracing::error!("Commit delta job failed: {}", e);
            }
        });
    }

    Ok(Json(CommitDeltaResponse {
        status: "accepted".to_string(),
        job_id,
        message: "Commit delta pack creation started".to_string(),
    }))
}
