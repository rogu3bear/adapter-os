//! Git integration API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Extension, Json,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use utoipa::ToSchema;

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::git::{
    GitCheckoutRequest, GitCheckoutResponse, GitCommitRequest, GitCommitResponse, GitLogEntry,
    WorkingTreeDiffResponse, WorkingTreeFileOperationRequest, WorkingTreeOperationResponse,
    WorkingTreeStatusResponse,
};

/// Start Git session request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartGitSessionRequest {
    pub adapter_id: String,
    pub repo_id: String,
    pub base_branch: Option<String>,
}

/// Start Git session response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartGitSessionResponse {
    pub session_id: String,
    pub branch_name: String,
}

/// End Git session request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EndGitSessionRequest {
    pub action: SessionAction,
}

/// Session action
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionAction {
    Merge,
    Abandon,
}

/// End Git session response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EndGitSessionResponse {
    pub status: String,
    pub merge_commit_sha: Option<String>,
}

/// Git branch info
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitBranchInfo {
    pub adapter_id: String,
    pub branch_name: String,
    pub created_at: String,
    pub commit_count: i64,
}

/// File change event for SSE
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileChangeEvent {
    pub file_path: String,
    pub change_type: String,
    pub adapter_id: Option<String>,
    pub timestamp: String,
}

/// Query parameters for file change stream
#[derive(Debug, Deserialize)]
pub struct FileChangeStreamQuery {
    pub repo_id: Option<String>,
}

/// Query parameters for working-tree endpoints
#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkingTreeQuery {
    pub repo_id: Option<String>,
}

/// Query parameters for working-tree diff endpoint
#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkingTreeDiffQuery {
    pub repo_id: Option<String>,
    pub path: Option<String>,
}

/// Query parameters for git log endpoint
#[derive(Debug, Deserialize, ToSchema)]
pub struct GitLogQuery {
    pub repo_id: Option<String>,
    pub branch: Option<String>,
    pub limit: Option<usize>,
}

/// Get Git status
#[utoipa::path(
    get,
    path = "/v1/git/status",
    responses(
        (status = 200, description = "Git status", body = WorkingTreeStatusResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
) -> Result<Json<WorkingTreeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    git_working_status(State(state), Extension(claims), Query(query)).await
}

/// Get working-tree status
#[utoipa::path(
    get,
    path = "/v1/git/working-status",
    responses(
        (status = 200, description = "Working-tree status", body = WorkingTreeStatusResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_working_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
) -> Result<Json<WorkingTreeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitView)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let status = git_subsystem
        .get_working_tree_status(query.repo_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get git status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get git status")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(WorkingTreeStatusResponse {
        schema_version: adapteros_api_types::schema_version(),
        branch: status.branch,
        modified_files: status.modified_files,
        staged_files: status.staged_files,
        untracked_files: status.untracked_files,
    }))
}

/// Get working-tree diff (optionally filtered by repository-relative path)
#[utoipa::path(
    get,
    path = "/v1/git/working-diff",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository"),
        ("path" = Option<String>, Query, description = "Repository-relative path to filter diff")
    ),
    responses(
        (status = 200, description = "Working-tree diff", body = WorkingTreeDiffResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_working_diff(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeDiffQuery>,
) -> Result<Json<WorkingTreeDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitView)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let diff = git_subsystem
        .get_working_diff(query.repo_id.as_deref(), query.path.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get working diff")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(WorkingTreeDiffResponse {
        schema_version: adapteros_api_types::schema_version(),
        diff,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/git/working-tree/stage",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = WorkingTreeFileOperationRequest,
    responses(
        (status = 200, description = "File staged", body = WorkingTreeOperationResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn stage_working_tree_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<WorkingTreeFileOperationRequest>,
) -> Result<Json<WorkingTreeOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    git_subsystem
        .stage_file(query.repo_id.as_deref(), &request.file_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to stage file")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(WorkingTreeOperationResponse {
        schema_version: adapteros_api_types::schema_version(),
        success: true,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/git/stage",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = WorkingTreeFileOperationRequest,
    responses(
        (status = 200, description = "File staged", body = WorkingTreeOperationResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn stage_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<WorkingTreeFileOperationRequest>,
) -> Result<Json<WorkingTreeOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    stage_working_tree_file(State(state), Extension(claims), Query(query), Json(request)).await
}

#[utoipa::path(
    post,
    path = "/v1/git/working-tree/unstage",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = WorkingTreeFileOperationRequest,
    responses(
        (status = 200, description = "File unstaged", body = WorkingTreeOperationResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn unstage_working_tree_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<WorkingTreeFileOperationRequest>,
) -> Result<Json<WorkingTreeOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    git_subsystem
        .unstage_file(query.repo_id.as_deref(), &request.file_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unstage file")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(WorkingTreeOperationResponse {
        schema_version: adapteros_api_types::schema_version(),
        success: true,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/git/unstage",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = WorkingTreeFileOperationRequest,
    responses(
        (status = 200, description = "File unstaged", body = WorkingTreeOperationResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn unstage_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<WorkingTreeFileOperationRequest>,
) -> Result<Json<WorkingTreeOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    unstage_working_tree_file(State(state), Extension(claims), Query(query), Json(request)).await
}

/// Commit currently staged changes
#[utoipa::path(
    post,
    path = "/v1/git/commit",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = GitCommitRequest,
    responses(
        (status = 200, description = "Commit created", body = GitCommitResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_commit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<GitCommitRequest>,
) -> Result<Json<GitCommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let commit_sha = git_subsystem
        .create_commit(query.repo_id.as_deref(), &request.message)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create commit")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(GitCommitResponse {
        schema_version: adapteros_api_types::schema_version(),
        success: true,
        commit_sha,
    }))
}

/// Get git commit log for repository/branch
#[utoipa::path(
    get,
    path = "/v1/git/log",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository"),
        ("branch" = Option<String>, Query, description = "Branch name; defaults to repo default branch"),
        ("limit" = Option<usize>, Query, description = "Maximum commits (default 50, max 200)")
    ),
    responses(
        (status = 200, description = "Git log", body = Vec<GitLogEntry>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_log(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<GitLogQuery>,
) -> Result<Json<Vec<GitLogEntry>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitView)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let commits = git_subsystem
        .list_commits(
            query.repo_id.as_deref(),
            query.branch.as_deref(),
            query.limit.unwrap_or(50),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get git log")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let entries = commits
        .into_iter()
        .map(|commit| GitLogEntry {
            sha: commit.sha,
            message: commit.message,
            author: commit.author,
            date: commit.date.to_rfc3339(),
        })
        .collect();

    Ok(Json(entries))
}

/// Checkout a branch
#[utoipa::path(
    post,
    path = "/v1/git/checkout",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = GitCheckoutRequest,
    responses(
        (status = 200, description = "Checked out branch", body = GitCheckoutResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_checkout(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<GitCheckoutRequest>,
) -> Result<Json<GitCheckoutResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    git_subsystem
        .checkout_branch(query.repo_id.as_deref(), &request.branch)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to checkout branch")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(GitCheckoutResponse {
        schema_version: adapteros_api_types::schema_version(),
        success: true,
        branch: request.branch,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/git/working-tree/discard",
    params(
        ("repo_id" = Option<String>, Query, description = "Repository ID; defaults to first registered repository")
    ),
    request_body = WorkingTreeFileOperationRequest,
    responses(
        (status = 200, description = "File changes discarded", body = WorkingTreeOperationResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn discard_working_tree_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<WorkingTreeQuery>,
    Json(request): Json<WorkingTreeFileOperationRequest>,
) -> Result<Json<WorkingTreeOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    git_subsystem
        .discard_file(query.repo_id.as_deref(), &request.file_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to discard file changes")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(WorkingTreeOperationResponse {
        schema_version: adapteros_api_types::schema_version(),
        success: true,
    }))
}

/// Start a new Git session for an adapter
#[utoipa::path(
    post,
    path = "/v1/git/sessions/start",
    request_body = StartGitSessionRequest,
    responses(
        (status = 200, description = "Session started", body = StartGitSessionResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
#[axum::debug_handler]
pub async fn start_git_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<StartGitSessionRequest>,
) -> Result<Json<StartGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;

    // Get Git subsystem from state
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Start session
    let branch_manager = git_subsystem.branch_manager();
    let session = branch_manager
        .read()
        .await
        .start_session(request.adapter_id, request.repo_id, request.base_branch)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to start Git session: {}", e))
                        .with_code("INTERNAL_SERVER_ERROR"),
                ),
            )
        })?;

    Ok(Json(StartGitSessionResponse {
        session_id: session.id,
        branch_name: session.branch_name,
    }))
}

/// End a Git session
#[utoipa::path(
    post,
    path = "/v1/git/sessions/{session_id}/end",
    params(
        ("session_id" = String, Path, description = "Git session ID")
    ),
    request_body = EndGitSessionRequest,
    responses(
        (status = 200, description = "Session ended", body = EndGitSessionResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn end_git_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(request): Json<EndGitSessionRequest>,
) -> Result<Json<EndGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitManage)?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let merge = matches!(request.action, SessionAction::Merge);

    let branch_manager = git_subsystem.branch_manager();
    let merge_commit_sha = branch_manager
        .read()
        .await
        .end_session(&session_id, merge)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to end Git session: {}", e))
                        .with_code("INTERNAL_SERVER_ERROR"),
                ),
            )
        })?;

    let status = if merge { "merged" } else { "abandoned" };

    Ok(Json(EndGitSessionResponse {
        status: status.to_string(),
        merge_commit_sha,
    }))
}

/// List adapter Git branches
#[utoipa::path(
    get,
    path = "/v1/git/branches",
    responses(
        (status = 200, description = "List of adapter branches", body = Vec<GitBranchInfo>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn list_git_branches(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<GitBranchInfo>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitView)?;

    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Git subsystem not available").with_code("INTERNAL_ERROR")),
        )
    })?;

    let branch_manager = git_subsystem.branch_manager();
    let sessions = branch_manager.read().await.list_active_sessions().await;

    let branches: Vec<GitBranchInfo> = sessions
        .into_iter()
        .map(|session| GitBranchInfo {
            adapter_id: session.adapter_id,
            branch_name: session.branch_name,
            created_at: session.started_at.to_rfc3339(),
            commit_count: 0, // Placeholder - would get actual commit count
        })
        .collect();

    Ok(Json(branches))
}

/// Stream file changes via SSE
#[utoipa::path(
    get,
    path = "/v1/stream/file-changes",
    params(
        ("repo_id" = Option<String>, Query, description = "Filter by repository ID")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of file changes"),
        (status = 500, description = "Internal server error")
    ),
    tag = "git"
)]
pub async fn file_changes_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<FileChangeStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::GitView)?;

    // Get file change broadcast channel from state
    let rx = state
        .file_change_tx
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("File change streaming not available")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(event) => {
                // Filter by repo_id if specified
                if let Some(ref repo_id) = query.repo_id {
                    if event.repo_id != *repo_id {
                        return None;
                    }
                }

                // Convert to SSE event
                let data = FileChangeEvent {
                    file_path: event.file_path.clone(),
                    change_type: event.change_type.clone(),
                    adapter_id: event.adapter_id.clone(),
                    timestamp: event.timestamp.clone(),
                };

                match serde_json::to_string(&data) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
