//! Git integration API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Json,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use utoipa::ToSchema;

use crate::state::AppState;
use crate::types::ErrorResponse;

/// Git status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitStatusResponse {
    pub branch: String,
    pub modified_files: Vec<String>,
    pub untracked_files: Vec<String>,
    pub staged_files: Vec<String>,
}

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

/// Get Git status
#[utoipa::path(
    get,
    path = "/v1/git/status",
    responses(
        (status = 200, description = "Git status", body = GitStatusResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "git"
)]
pub async fn git_status(
    State(_state): State<AppState>,
) -> Result<Json<GitStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Implement actual Git status checking
    // For now, return placeholder data
    Ok(Json(GitStatusResponse {
        branch: "main".to_string(),
        modified_files: vec![],
        untracked_files: vec![],
        staged_files: vec![],
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
    Json(request): Json<StartGitSessionRequest>,
) -> Result<Json<StartGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get Git subsystem from state
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Git subsystem not available".to_string(),
                details: None,
            }),
        )
    })?;

    // Start session
    let session = git_subsystem
        .branch_manager()
        .start_session(request.adapter_id, request.repo_id, request.base_branch)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to start Git session: {}", e),
                    details: None,
                }),
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
    Path(session_id): Path<String>,
    Json(request): Json<EndGitSessionRequest>,
) -> Result<Json<EndGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Git subsystem not available".to_string(),
                details: None,
            }),
        )
    })?;

    let merge = matches!(request.action, SessionAction::Merge);

    let merge_commit_sha = git_subsystem
        .branch_manager()
        .end_session(&session_id, merge)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to end Git session: {}", e),
                    details: None,
                }),
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
) -> Result<Json<Vec<GitBranchInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Git subsystem not available".to_string(),
                details: None,
            }),
        )
    })?;

    let sessions = git_subsystem.branch_manager().list_active_sessions().await;

    let branches: Vec<GitBranchInfo> = sessions
        .into_iter()
        .map(|session| GitBranchInfo {
            adapter_id: session.adapter_id,
            branch_name: session.branch_name,
            created_at: session.started_at.to_rfc3339(),
            commit_count: 0, // TODO: Get actual commit count
        })
        .collect();

    Ok(Json(branches))
}

/// Stream file changes via SSE
#[utoipa::path(
    get,
    path = "/v1/streams/file-changes",
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
    Query(query): Query<FileChangeStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Get file change broadcast channel from state
    let rx = state
        .file_change_tx
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "File change streaming not available".to_string(),
                    details: None,
                }),
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
                    file_path: event.file_path.to_string_lossy().to_string(),
                    change_type: event.change_type.to_string(),
                    adapter_id: event.adapter_id,
                    timestamp: event.timestamp.to_rfc3339(),
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
