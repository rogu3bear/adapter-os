use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use adapteros_db::Db;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_telemetry::MetricsCollector;

/// Build a minimal AppState with in-memory DB, metrics, and training service.
///
/// Creates all required dependencies for integration testing:
/// - In-memory SQLite database with migrations applied
/// - Default tenant created for test isolation
/// - Metrics infrastructure (collector, registry, exporter)
/// - UMA pressure monitor for memory management
#[allow(dead_code)]
pub async fn setup_state(_uds_path: Option<&PathBuf>) -> anyhow::Result<AppState> {
    // 1. Create in-memory database with migrations
    let db = Db::new_in_memory()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create in-memory DB: {}", e))?;

    // 2. Create test tenants
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('default', 'Default Tenant')",
    )
    .execute(db.pool())
    .await?;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant 1')",
    )
    .execute(db.pool())
    .await?;

    // 3. Create test JWT secret
    let jwt_secret = b"test-jwt-secret-for-integration-tests-32bytes!".to_vec();

    // 4. Create API config
    let config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test-bearer-token".to_string(),
        },
        directory_analysis_timeout_secs: 120,
    }));

    // 5. Create metrics exporter with standard histogram buckets
    let histogram_buckets = vec![
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];
    let metrics_exporter = Arc::new(
        MetricsExporter::new(histogram_buckets)
            .map_err(|e| anyhow::anyhow!("Failed to create metrics exporter: {}", e))?,
    );

    // 6. Create metrics collector and registry
    let metrics_collector = Arc::new(MetricsCollector::default());
    let metrics_registry = Arc::new(MetricsRegistry::new());

    // 7. Create UMA pressure monitor (15% min headroom, no telemetry for tests)
    let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

    // 8. Build AppState
    Ok(AppState::new(
        db,
        jwt_secret,
        config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        uma_monitor,
    ))
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
