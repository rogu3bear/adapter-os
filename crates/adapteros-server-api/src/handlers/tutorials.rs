//! Tutorial handlers
//!
//! Provides API endpoints for tutorial status management (completion and dismissal).
//!
//! Tutorial definitions are loaded from shared/tutorials.json to avoid duplication
//! between Rust and TypeScript. See: ui/src/data/tutorial-content.ts for UI consumption.

use crate::handlers::{AppState, Claims, ErrorResponse};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TutorialStep {
    pub id: String,
    pub title: String,
    pub content: String,
    pub target_selector: Option<String>,
    pub position: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TutorialDefinition {
    pub id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<TutorialStep>,
    pub dismissible: bool,
}

#[derive(Debug, Deserialize)]
struct TutorialsConfig {
    pub tutorials: Vec<TutorialDefinition>,
}

// Load tutorials from shared JSON at compile time
static TUTORIALS_JSON: &str = include_str!("../../../../shared/tutorials.json");

static TUTORIAL_DEFINITIONS: Lazy<Vec<TutorialDefinition>> = Lazy::new(|| {
    let config: TutorialsConfig = serde_json::from_str(TUTORIALS_JSON).expect(
        "Failed to parse shared/tutorials.json at compile time: \
             expected valid JSON structure with 'tutorials' array field, \
             but deserialization failed. This indicates tutorials.json is malformed \
             or incompatible with TutorialsConfig schema.",
    );
    config.tutorials
});

static VALID_TUTORIAL_IDS: Lazy<Vec<String>> =
    Lazy::new(|| TUTORIAL_DEFINITIONS.iter().map(|t| t.id.clone()).collect());

#[derive(Debug, Serialize, ToSchema)]
pub struct TutorialResponse {
    pub id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<TutorialStep>,
    pub trigger: Option<String>,
    pub dismissible: bool,
    pub completed: bool,
    pub dismissed: bool,
    pub completed_at: Option<String>,
    pub dismissed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TutorialStatusResponse {
    pub tutorial_id: String,
    pub completed: bool,
    pub dismissed: bool,
    pub completed_at: Option<String>,
    pub dismissed_at: Option<String>,
}

fn is_valid_tutorial_id(id: &str) -> bool {
    VALID_TUTORIAL_IDS.iter().any(|valid_id| valid_id == id)
}

/// List all tutorials with their status for the current user
#[utoipa::path(
    get,
    path = "/v1/tutorials",
    responses(
        (status = 200, description = "List of tutorials", body = Vec<TutorialResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tutorials"
)]
pub async fn list_tutorials(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TutorialResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Load canonical tutorial definitions from shared JSON
    let canonical_tutorials: Vec<TutorialResponse> = TUTORIAL_DEFINITIONS
        .iter()
        .map(|def| TutorialResponse {
            id: def.id.clone(),
            title: def.title.clone(),
            description: def.description.clone(),
            steps: def.steps.clone(),
            trigger: None,
            dismissible: def.dismissible,
            completed: false,
            dismissed: false,
            completed_at: None,
            dismissed_at: None,
        })
        .collect();

    // Fetch user's tutorial statuses
    let statuses = state
        .db
        .list_user_tutorial_statuses(&claims.sub)
        .await
        .map_err(|e| {
            error!("Failed to list tutorial statuses: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list tutorial statuses")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Create a map of tutorial_id -> status
    let status_map: std::collections::HashMap<String, adapteros_db::tutorials::TutorialStatus> =
        statuses
            .into_iter()
            .map(|s| (s.tutorial_id.clone(), s))
            .collect();

    // Merge tutorials with their statuses
    let tutorials_with_status: Vec<TutorialResponse> = canonical_tutorials
        .into_iter()
        .map(|mut tutorial| {
            if let Some(status) = status_map.get(&tutorial.id) {
                tutorial.completed = status.completed_at.is_some();
                tutorial.dismissed = status.dismissed_at.is_some();
                tutorial.completed_at = status.completed_at.clone();
                tutorial.dismissed_at = status.dismissed_at.clone();
            }
            tutorial
        })
        .collect();

    Ok(Json(tutorials_with_status))
}

/// Mark tutorial as completed
#[utoipa::path(
    post,
    path = "/v1/tutorials/{tutorial_id}/complete",
    params(
        ("tutorial_id" = String, Path, description = "Tutorial ID")
    ),
    responses(
        (status = 200, description = "Tutorial marked as completed"),
        (status = 400, description = "Invalid tutorial ID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tutorials"
)]
pub async fn mark_tutorial_completed(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tutorial_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Validate tutorial ID
    if !is_valid_tutorial_id(&tutorial_id) {
        warn!(
            tutorial_id = %tutorial_id,
            user_id = %claims.sub,
            "Attempted to mark invalid tutorial as completed"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
        ));
    }

    state
        .db
        .mark_tutorial_completed(&claims.sub, &tutorial_id)
        .await
        .map_err(|e| {
            error!("Failed to mark tutorial as completed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to mark tutorial as completed")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(serde_json::json!({"status": "completed"})))
}

/// Unmark tutorial as completed
#[utoipa::path(
    post,
    path = "/v1/tutorials/{tutorial_id}/incomplete",
    params(
        ("tutorial_id" = String, Path, description = "Tutorial ID")
    ),
    responses(
        (status = 200, description = "Tutorial marked as incomplete"),
        (status = 400, description = "Invalid tutorial ID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tutorials"
)]
pub async fn unmark_tutorial_completed(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tutorial_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if !is_valid_tutorial_id(&tutorial_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
        ));
    }

    state
        .db
        .unmark_tutorial_completed(&claims.sub, &tutorial_id)
        .await
        .map_err(|e| {
            error!("Failed to unmark tutorial as completed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unmark tutorial as completed")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(serde_json::json!({"status": "not_completed"})))
}

/// Mark tutorial as dismissed
#[utoipa::path(
    post,
    path = "/v1/tutorials/{tutorial_id}/dismiss",
    params(
        ("tutorial_id" = String, Path, description = "Tutorial ID")
    ),
    responses(
        (status = 200, description = "Tutorial dismissed"),
        (status = 400, description = "Invalid tutorial ID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tutorials"
)]
pub async fn mark_tutorial_dismissed(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tutorial_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if !is_valid_tutorial_id(&tutorial_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
        ));
    }

    state
        .db
        .mark_tutorial_dismissed(&claims.sub, &tutorial_id)
        .await
        .map_err(|e| {
            error!("Failed to mark tutorial as dismissed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to mark tutorial as dismissed")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(serde_json::json!({"status": "dismissed"})))
}

/// Unmark tutorial as dismissed
#[utoipa::path(
    post,
    path = "/v1/tutorials/{tutorial_id}/undismiss",
    params(
        ("tutorial_id" = String, Path, description = "Tutorial ID")
    ),
    responses(
        (status = 200, description = "Tutorial undismissed"),
        (status = 400, description = "Invalid tutorial ID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tutorials"
)]
pub async fn unmark_tutorial_dismissed(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tutorial_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if !is_valid_tutorial_id(&tutorial_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid tutorial ID").with_code("INVALID_TUTORIAL_ID")),
        ));
    }

    state
        .db
        .unmark_tutorial_dismissed(&claims.sub, &tutorial_id)
        .await
        .map_err(|e| {
            error!("Failed to unmark tutorial as dismissed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unmark tutorial as dismissed")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(serde_json::json!({"status": "not_dismissed"})))
}
