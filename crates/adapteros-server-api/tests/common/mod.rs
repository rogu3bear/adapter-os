use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use adapteros_orchestrator::TrainingService;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};

/// Build a minimal AppState with in-memory DB, metrics, and training service.
pub async fn setup_state(uds_path: Option<&PathBuf>) -> anyhow::Result<AppState> {
    let db = adapteros_db::Db::connect(":memory:").await?;

    // Minimal workers table for routes that depend on it
    adapteros_db::sqlx::query(
        "CREATE TABLE workers (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            node_id TEXT NOT NULL,
            plan_id TEXT NOT NULL,
            uds_path TEXT NOT NULL,
            pid INTEGER,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            last_seen_at TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    if let Some(path) = uds_path {
        adapteros_db::sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at)
             VALUES (?, ?, ?, ?, ?, NULL, 'ready', '2024-01-01T00:00:00Z', NULL)",
        )
        .bind("worker-1")
        .bind("tenant-1")
        .bind("node-1")
        .bind("plan-1")
        .bind(path.to_string_lossy().to_string())
        .execute(db.pool())
        .await?;
    }

    // Minimal training jobs table to support pause/resume and listing
    adapteros_db::sqlx::query(
        "CREATE TABLE IF NOT EXISTS repository_training_jobs (
            id TEXT PRIMARY KEY,
            repo_id TEXT NOT NULL,
            training_config_json TEXT NOT NULL,
            status TEXT NOT NULL,
            progress_json TEXT NOT NULL,
            started_at TEXT DEFAULT CURRENT_TIMESTAMP,
            completed_at TEXT,
            created_by TEXT NOT NULL
        )",
    )
    .execute(db.pool())
    .await?;

    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: String::new(),
            system_metrics_interval_secs: 0,
        },
        golden_gate: None,
        bundles_root: "var/bundles".to_string(),
        rate_limits: None,
    };

    let metrics = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
        0.1, 0.5, 1.0,
    ])?);
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new()?);
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));
    // Pre-create the standard dashboard series so tests can record snapshots deterministically.
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }

    let training_service = Arc::new(TrainingService::new());

    Ok(AppState::with_sqlite(
        db,
        b"test-secret".to_vec(),
        Arc::new(RwLock::new(config)),
        metrics,
        metrics_collector,
        metrics_registry,
        training_service,
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
