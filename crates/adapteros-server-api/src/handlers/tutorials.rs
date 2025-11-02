//! Tutorial handlers
//!
//! Provides API endpoints for tutorial status management (completion and dismissal).
//!
//! Note: Tutorial definitions are currently hardcoded. In a production system,
//! these should be loaded from a configuration file or database table.
//! See: ui/src/data/tutorial-content.ts for the UI-side definitions.

use crate::handlers::{AppState, Claims, ErrorResponse};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use tracing::{error, warn};

#[derive(Debug, Serialize)]
pub struct TutorialStep {
    pub id: String,
    pub title: String,
    pub content: String,
    pub target_selector: Option<String>,
    pub position: Option<String>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct TutorialStatusResponse {
    pub tutorial_id: String,
    pub completed: bool,
    pub dismissed: bool,
    pub completed_at: Option<String>,
    pub dismissed_at: Option<String>,
}

// Known tutorial IDs for validation
// TODO: Load from shared config file to avoid duplication with ui/src/data/tutorial-content.ts
const VALID_TUTORIAL_IDS: &[&str] = &[
    "training-tutorial",
    "adapter-management-tutorial",
    "policy-management-tutorial",
    "dashboard-tutorial",
];

fn is_valid_tutorial_id(id: &str) -> bool {
    VALID_TUTORIAL_IDS.contains(&id)
}

/// List all tutorials with their status for the current user
pub async fn list_tutorials(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TutorialResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Load canonical tutorial definitions
    // TODO: Load from shared config file or database to avoid duplication
    // Current source of truth: ui/src/data/tutorial-content.ts
    let canonical_tutorials = vec![
        TutorialResponse {
            id: "training-tutorial".to_string(),
            title: "Training Adapters Tutorial".to_string(),
            description: "Learn how to train adapters step by step".to_string(),
            steps: vec![
                TutorialStep {
                    id: "intro".to_string(),
                    title: "Welcome to Training".to_string(),
                    content: "This tutorial will guide you through training an adapter. Training creates specialized AI models for your domain.".to_string(),
                    target_selector: None,
                    position: Some("center".to_string()),
                },
                TutorialStep {
                    id: "select-template".to_string(),
                    title: "Choose a Template".to_string(),
                    content: "Start by selecting a training template. Templates provide pre-configured settings for common use cases.".to_string(),
                    target_selector: Some("[data-tutorial=\"training-template\"]".to_string()),
                    position: Some("bottom".to_string()),
                },
                TutorialStep {
                    id: "configure-params".to_string(),
                    title: "Configure Parameters".to_string(),
                    content: "Adjust training parameters like learning rate, batch size, and epochs to match your requirements.".to_string(),
                    target_selector: Some("[data-tutorial=\"training-params\"]".to_string()),
                    position: Some("right".to_string()),
                },
                TutorialStep {
                    id: "start-training".to_string(),
                    title: "Start Training".to_string(),
                    content: "Click the Start Training button to launch your training job. Monitor progress in real-time.".to_string(),
                    target_selector: Some("[data-tutorial=\"start-training\"]".to_string()),
                    position: Some("top".to_string()),
                },
            ],
            trigger: None,
            dismissible: true,
            completed: false,
            dismissed: false,
            completed_at: None,
            dismissed_at: None,
        },
        TutorialResponse {
            id: "adapter-management-tutorial".to_string(),
            title: "Managing Adapters".to_string(),
            description: "Learn how to deploy and manage adapters".to_string(),
            steps: vec![
                TutorialStep {
                    id: "intro".to_string(),
                    title: "Adapter Management".to_string(),
                    content: "Adapters are specialized AI models trained for specific domains. Learn how to deploy and manage them.".to_string(),
                    target_selector: None,
                    position: Some("center".to_string()),
                },
                TutorialStep {
                    id: "deploy-adapter".to_string(),
                    title: "Deploy an Adapter".to_string(),
                    content: "Click Deploy to make a trained adapter available for inference. Ensure it has passed all tests first.".to_string(),
                    target_selector: Some("[data-tutorial=\"deploy-adapter\"]".to_string()),
                    position: Some("bottom".to_string()),
                },
                TutorialStep {
                    id: "monitor-health".to_string(),
                    title: "Monitor Health".to_string(),
                    content: "Check adapter health metrics regularly. Look for latency, error rates, and resource usage.".to_string(),
                    target_selector: Some("[data-tutorial=\"adapter-health\"]".to_string()),
                    position: Some("right".to_string()),
                },
            ],
            trigger: None,
            dismissible: true,
            completed: false,
            dismissed: false,
            completed_at: None,
            dismissed_at: None,
        },
        TutorialResponse {
            id: "policy-management-tutorial".to_string(),
            title: "Policy Management".to_string(),
            description: "Learn how to manage policies and ensure compliance".to_string(),
            steps: vec![
                TutorialStep {
                    id: "intro".to_string(),
                    title: "Policy Management".to_string(),
                    content: "Policies enforce security and compliance rules. This tutorial shows you how to review and manage them.".to_string(),
                    target_selector: None,
                    position: Some("center".to_string()),
                },
                TutorialStep {
                    id: "review-policies".to_string(),
                    title: "Review Policy Packs".to_string(),
                    content: "All 20 policy packs should be reviewed regularly. Click on a policy to view its details.".to_string(),
                    target_selector: Some("[data-tutorial=\"policy-list\"]".to_string()),
                    position: Some("bottom".to_string()),
                },
                TutorialStep {
                    id: "sign-policies".to_string(),
                    title: "Sign Policies".to_string(),
                    content: "After reviewing, sign policies to indicate compliance. Unsigned policies may block operations.".to_string(),
                    target_selector: Some("[data-tutorial=\"sign-policy\"]".to_string()),
                    position: Some("right".to_string()),
                },
            ],
            trigger: None,
            dismissible: true,
            completed: false,
            dismissed: false,
            completed_at: None,
            dismissed_at: None,
        },
        TutorialResponse {
            id: "dashboard-tutorial".to_string(),
            title: "Dashboard Overview".to_string(),
            description: "Learn how to navigate and use the dashboard".to_string(),
            steps: vec![
                TutorialStep {
                    id: "intro".to_string(),
                    title: "Welcome to the Dashboard".to_string(),
                    content: "The dashboard provides a system overview with health metrics, adapter counts, and performance indicators.".to_string(),
                    target_selector: None,
                    position: Some("center".to_string()),
                },
                TutorialStep {
                    id: "system-health".to_string(),
                    title: "System Health".to_string(),
                    content: "Monitor system health metrics here. Green indicates healthy, yellow means attention needed, red requires immediate action.".to_string(),
                    target_selector: Some("[data-tutorial=\"system-health\"]".to_string()),
                    position: Some("bottom".to_string()),
                },
                TutorialStep {
                    id: "recent-activity".to_string(),
                    title: "Recent Activity".to_string(),
                    content: "View recent system events and activities. This helps you stay informed about what's happening.".to_string(),
                    target_selector: Some("[data-tutorial=\"recent-activity\"]".to_string()),
                    position: Some("right".to_string()),
                },
            ],
            trigger: None,
            dismissible: true,
            completed: false,
            dismissed: false,
            completed_at: None,
            dismissed_at: None,
        },
    ];

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
