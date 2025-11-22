use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use adapteros_orchestrator::TrainingService;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};

// NOTE: This test setup is currently incomplete due to API changes.
// Tests using setup_state should be marked with #[ignore] until the
// AppState construction is updated to match the current API.

/// Build a minimal AppState with in-memory DB, metrics, and training service.
///
/// IMPORTANT: This function currently returns Err due to API changes.
/// Tests using this should be marked with #[ignore = "Pending API refactoring"]
#[allow(dead_code)]
pub async fn setup_state(_uds_path: Option<&PathBuf>) -> anyhow::Result<AppState> {
    // TODO: Refactor to match current AppState API
    // The AppState constructor has changed significantly and requires:
    // - Different config structure
    // - UmaPressureMonitor
    // - MetricsRegistry from adapteros_telemetry
    Err(anyhow::anyhow!("setup_state needs refactoring to match current AppState API"))
}

/// Standard admin claims for tests
pub fn test_admin_claims() -> Claims {
    Claims {
        sub: "tenant-1-user".to_string(),
        email: "user@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: "tenant-1".to_string(),
        exp: 0,
        iat: 0,
        jti: "test-token".to_string(),
        nbf: 0,
    }
}

/// Standard viewer claims for tests
pub fn test_viewer_claims() -> Claims {
    Claims {
        sub: "viewer-user-id".to_string(),
        email: "viewer@example.com".to_string(),
        role: "viewer".to_string(),
        tenant_id: "default".to_string(),
        exp: 9999999999,
        iat: 0,
        jti: "test-viewer-token".to_string(),
        nbf: 0,
    }
}

/// Standard operator claims for tests
pub fn test_operator_claims() -> Claims {
    Claims {
        sub: "operator-user-id".to_string(),
        email: "operator@example.com".to_string(),
        role: "operator".to_string(),
        tenant_id: "default".to_string(),
        exp: 9999999999,
        iat: 0,
        jti: "test-operator-token".to_string(),
        nbf: 0,
    }
}

/// Standard compliance claims for tests
pub fn test_compliance_claims() -> Claims {
    Claims {
        sub: "compliance-user-id".to_string(),
        email: "compliance@example.com".to_string(),
        role: "compliance".to_string(),
        tenant_id: "default".to_string(),
        exp: 9999999999,
        iat: 0,
        jti: "test-compliance-token".to_string(),
        nbf: 0,
    }
}

/// Insert a training job record into the in-memory DB.
pub async fn insert_training_job(
    state: &adapteros_server_api::state::AppState,
    id: &str,
    status: &str,
) -> anyhow::Result<()> {
    let progress_json = serde_json::json!({
        "progress_pct": 0.0,
        "current_epoch": 0,
        "total_epochs": 3,
        "current_loss": 0.0,
        "learning_rate": 0.001,
        "tokens_per_second": 0.0,
        "error_message": null
    })
    .to_string();

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs
            (id, repo_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind("repo-1")
    .bind("{\"rank\":16,\"alpha\":32,\"targets\":[\"q_proj\"],\"epochs\":1,\"learning_rate\":0.001,\"batch_size\":8}")
    .bind(status)
    .bind(progress_json)
    .bind("tester")
    .execute(state.db.pool())
    .await?;

    Ok(())
}

/// Create a test workspace in the database
pub async fn create_test_workspace(
    state: &AppState,
    name: &str,
    owner_id: &str,
) -> anyhow::Result<String> {
    let workspace_id = state
        .db
        .create_workspace(name, None, owner_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create workspace: {}", e))?;
    Ok(workspace_id)
}

/// Create a test notification in the database
pub async fn create_test_notification(
    state: &AppState,
    user_id: &str,
    title: &str,
) -> anyhow::Result<String> {
    let notification_id = state
        .db
        .create_notification(
            user_id,
            None, // workspace_id
            adapteros_db::notifications::NotificationType::System,
            None, // target_type
            None, // target_id
            title,
            None, // content
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create notification: {}", e))?;
    Ok(notification_id)
}
