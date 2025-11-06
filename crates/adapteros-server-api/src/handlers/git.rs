use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::{http::StatusCode, Json};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GitStatusResponse {
    pub enabled: bool,
    pub active_sessions: u32,
    pub repositories_tracked: u32,
    pub last_scan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StartGitSessionRequest {
    pub adapter_id: String,
    pub repo_id: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StartGitSessionResponse {
    pub session_id: String,
    pub adapter_id: String,
    pub repo_id: String,
    pub branch_name: String,
    pub base_commit_sha: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EndGitSessionRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EndGitSessionResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GitBranchInfo {
    pub name: String,
    pub is_current: bool,
    pub last_commit: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub enum SessionAction {
    Start,
    End,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FileChangeEvent {
    pub file_path: String,
    pub change_type: String,
    pub timestamp: String,
    pub session_id: String,
}

pub async fn git_status(
    State(state): State<AppState>,
) -> Result<Json<GitStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    match git_subsystem.get_status().await {
        Ok(git_status) => {
            let status = GitStatusResponse {
                enabled: true,
                active_sessions: git_status.active_sessions,
                repositories_tracked: git_status.repositories_tracked,
                last_scan: git_status.last_scan,
            };
            Ok(Json(status))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to get git status: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        )),
    }
}

pub async fn start_git_session(
    State(state): State<AppState>,
    Json(req): Json<StartGitSessionRequest>,
) -> Result<Json<StartGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    match git_subsystem
        .branch_manager()
        .start_session(req.adapter_id.clone(), req.repo_id.clone(), req.base_branch)
        .await
    {
        Ok(session) => Ok(Json(StartGitSessionResponse {
            session_id: session.id,
            adapter_id: session.adapter_id,
            repo_id: session.repo_id,
            branch_name: session.branch_name,
            base_commit_sha: session.base_commit_sha,
            started_at: session.started_at.to_rfc3339(),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to start git session: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        )),
    }
}

pub async fn end_git_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<EndGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    match git_subsystem
        .branch_manager()
        .end_session(&session_id, false)
        .await
    {
        Ok(merge_commit_sha) => {
            let status = if merge_commit_sha.is_some() {
                "merged".to_string()
            } else {
                "ended".to_string()
            };
            Ok(Json(EndGitSessionResponse { session_id, status }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to end git session: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        )),
    }
}

pub async fn list_git_branches(
    State(state): State<AppState>,
) -> Result<Json<Vec<GitBranchInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let git_subsystem = state.git_subsystem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        )
    })?;

    match git_subsystem.list_branches(None).await {
        Ok(branches) => {
            let api_branches: Vec<GitBranchInfo> = branches
                .into_iter()
                .map(|b| GitBranchInfo {
                    name: b.name,
                    is_current: b.is_current,
                    last_commit: b.last_commit,
                    ahead: b.ahead,
                    behind: b.behind,
                })
                .collect();
            Ok(Json(api_branches))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to list branches: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        )),
    }
}

pub async fn file_changes_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let file_change_tx = state
        .file_change_tx
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .clone();

    let stream = stream::unfold((), move |_| {
        let mut rx = file_change_tx.subscribe();
        async move {
            match rx.recv().await {
                Ok(file_change_event) => {
                    let event = Event::default()
                        .event("file_change")
                        .json_data(&file_change_event)
                        .unwrap_or_else(|_| Event::default());
                    Some((Ok(event), ()))
                }
                Err(_) => {
                    // Channel closed, end stream
                    None
                }
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new()))
}
