use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::{http::StatusCode, Json};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GitStatusResponse {
    pub enabled: bool,
    pub active_sessions: u32,
    pub repositories_tracked: u32,
    pub last_scan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StartGitSessionRequest {
    pub repository_path: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StartGitSessionResponse {
    pub session_id: String,
    pub repository_path: String,
    pub branch: String,
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
    State(_state): State<AppState>,
) -> Result<Json<GitStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(GitStatusResponse {
        enabled: true,
        active_sessions: 0,
        repositories_tracked: 0,
        last_scan: None,
    }))
}

pub async fn start_git_session(
    State(_state): State<AppState>,
    Json(req): Json<StartGitSessionRequest>,
) -> Result<Json<StartGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    Ok(Json(StartGitSessionResponse {
        session_id,
        repository_path: req.repository_path,
        branch: req.branch.unwrap_or_else(|| "main".to_string()),
        started_at: chrono::Utc::now().to_rfc3339(),
    }))
}

pub async fn end_git_session(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<EndGitSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(EndGitSessionResponse {
        session_id,
        status: "ended".to_string(),
    }))
}

pub async fn list_git_branches(
    State(_state): State<AppState>,
) -> Result<Json<Vec<GitBranchInfo>>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(vec![GitBranchInfo {
        name: "main".to_string(),
        is_current: true,
        last_commit: "HEAD".to_string(),
        ahead: 0,
        behind: 0,
    }]))
}

pub async fn file_changes_stream(
    State(_state): State<AppState>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let stream = stream::unfold(0, |i| async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if i > 5 {
            return None;
        }
        let ev = FileChangeEvent {
            file_path: format!("src/file{}.rs", i),
            change_type: "modified".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_id: "demo".to_string(),
        };
        let event = Event::default()
            .event("file_change")
            .json_data(&ev)
            .unwrap_or_else(|_| Event::default());
        Some((Ok(event), i + 1))
    });
    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new()))
}
