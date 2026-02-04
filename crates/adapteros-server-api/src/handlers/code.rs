use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::code_repositories::{
    CommitDeltaRequest, CommitDeltaResponse, ListRepositoriesQuery, Pagination,
    RegisterRepositoryRequest, RegisterRepositoryResponse, RepositoryDetailResponse,
    RepositoryInfo, RepositoryListResponse, ScanJobProgress, ScanJobResponse, ScanJobResult,
    ScanJobStatusResponse, ScanRepositoryRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};

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
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterRepositoryRequest>,
) -> Result<Json<RegisterRepositoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeScan)?;

    // Enforce tenant isolation: caller must belong to repo tenant
    validate_tenant_isolation(&claims, &req.tenant_id)?;

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
    Extension(claims): Extension<Claims>,
    Json(req): Json<ScanRepositoryRequest>,
) -> Result<Json<ScanJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeScan)?;

    // Enforce tenant isolation: caller must belong to repo tenant
    validate_tenant_isolation(&claims, &req.tenant_id)?;

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
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<ScanJobStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeView)?;
    let job_id = crate::id_resolver::resolve_any_id(&state.db, &job_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

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

    // Validate tenant isolation by looking up the repository
    let repo = state.db.get_repository(&job.repo_id).await.map_err(|e| {
        // get_repository returns AosError::NotFound if not found
        let status = if matches!(e, adapteros_core::AosError::NotFound(_)) {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(ErrorResponse::new(e.to_string())))
    })?;

    validate_tenant_isolation(&claims, &repo.tenant_id)?;

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
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListRepositoriesQuery>,
) -> Result<Json<RepositoryListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeView)?;

    // Tenant-scoped listing
    let tenant_id = claims.tenant_id.as_str();
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

    // Filter by status if provided
    let filtered_repos: Vec<_> = if let Some(ref status_filter) = query.status {
        repos
            .into_iter()
            .filter(|r| r.status == *status_filter)
            .collect()
    } else {
        repos
    };

    // Count is for total (unfiltered) - for accurate pagination we'd need a filtered count
    // For now, return total count (pagination will need adjustment for filtered results)
    let total = state.db.count_repositories(tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    let repo_infos: Vec<RepositoryInfo> = filtered_repos
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
                status: r.status,
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
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepositoryDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeView)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let tenant_id = claims.tenant_id.as_str();

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
    Extension(claims): Extension<Claims>,
    Json(req): Json<CommitDeltaRequest>,
) -> Result<Json<CommitDeltaResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::CodeScan)?;

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
    let job_id = crate::id_generator::readable_id(adapteros_core::ids::IdKind::Job, "code");

    // Spawn background task
    if let Some(ref code_job_manager) = state.code_job_manager {
        let manager = code_job_manager.clone();
        let repo_id = req.repo_id.clone();
        let base = req.base_commit.clone();
        let head = req.head_commit.clone();

        tokio::spawn(async move {
            let job = adapteros_orchestrator::CommitDeltaJob {
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

// --- Imports for propose_patch ---
use crate::middleware::require_any_role;
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::{validate_description, validate_file_paths, validate_repo_id};
use adapteros_db::sqlx;
use adapteros_db::users::Role;

// --- Moved from handlers.rs ---
/// Propose code patch
#[utoipa::path(
    post,
    path = "/v1/patch/propose",
    request_body = ProposePatchRequest,
    responses(
        (status = 200, description = "Patch proposal response", body = ProposePatchResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn propose_patch(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ProposePatchRequest>,
) -> Result<Json<ProposePatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Validate inputs
    validate_repo_id(&req.repo_id)?;
    validate_description(&req.description)?;
    validate_file_paths(&req.target_files)?;

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no workers available")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("No active workers found for patch proposal"),
            ),
        ));
    }

    // Select first available worker (simple selection for now)
    let worker = &workers[0];
    let uds_path = std::path::Path::new(&worker.uds_path);

    // Create UDS client and send patch proposal request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(60)); // Longer timeout for patch generation

    let worker_request = PatchProposalInferRequest {
        cpid: "patch-proposal".to_string(),
        prompt: req.description.clone(),
        max_tokens: 2000,
        require_evidence: true,
        request_type: PatchProposalRequestType {
            repo_id: req.repo_id.clone(),
            commit_sha: Some(req.commit_sha.clone()),
            target_files: req.target_files.clone(),
            description: req.description.clone(),
        },
    };

    match uds_client.propose_patch(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Extract proposal ID and status
            let proposal_id = worker_response
                .patch_proposal
                .as_ref()
                .map(|p| p.proposal_id.clone())
                .unwrap_or_else(|| {
                    crate::id_generator::readable_id(adapteros_core::ids::IdKind::Job, "code")
                });

            let status = if worker_response.patch_proposal.is_some() {
                "completed"
            } else if worker_response.refusal.is_some() {
                "refused"
            } else {
                "failed"
            };

            let message = if let Some(ref proposal) = worker_response.patch_proposal {
                format!(
                    "Patch proposal generated successfully with {} files and {} citations",
                    proposal.patches.len(),
                    proposal.citations.len()
                )
            } else if let Some(ref refusal) = worker_response.refusal {
                format!("Patch proposal refused: {}", refusal.message)
            } else {
                "Patch proposal generation failed".to_string()
            };

            // Store proposal in database
            if let Some(ref proposal) = worker_response.patch_proposal {
                let proposal_json = serde_json::to_string(proposal).unwrap_or_else(|e| {
                    tracing::warn!("Failed to serialize patch proposal: {}", e);
                    "{}".to_string()
                });

                match sqlx::query(
                    "INSERT INTO patch_proposals 
                     (id, repo_id, commit_sha, status, proposal_json, created_at, created_by) 
                     VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
                )
                .bind(&proposal_id)
                .bind(&req.repo_id)
                .bind(&req.commit_sha)
                .bind(status)
                .bind(&proposal_json)
                .bind(&claims.email)
                .execute(state.db.pool())
                .await
                {
                    Ok(_) => {
                        tracing::info!("Stored patch proposal {} in database", proposal_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to store patch proposal in database: {}", e);
                        // Don't fail the request if storage fails
                    }
                }
            }

            Ok(Json(ProposePatchResponse {
                proposal_id,
                status: status.to_string(),
                message,
            }))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("patch generation timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("patch generation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}
