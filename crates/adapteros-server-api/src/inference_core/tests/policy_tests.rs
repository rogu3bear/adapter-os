//! Policy resolution tests for inference_core.

use crate::config::PathsConfig;
use crate::inference_core::resolve_tenant_execution_policy;
use crate::state::{ApiConfig, AppState, GeneralConfig, MetricsConfig};
use crate::telemetry::MetricsRegistry;
use adapteros_api_types::{CreateExecutionPolicyRequest, DeterminismPolicy};
use adapteros_core::{determinism_mode::DeterminismMode, BackendKind, SeedMode};
use adapteros_db::Db;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_telemetry::MetricsCollector;
use std::fs;
use std::sync::{Arc, RwLock};
use tempfile::Builder as TempDirBuilder;

async fn build_test_state_with_general(
    use_session_stack: bool,
    general_determinism_mode: Option<DeterminismMode>,
) -> AppState {
    let base = std::path::Path::new("var/test-dbs");
    fs::create_dir_all(base).unwrap();
    let dir = TempDirBuilder::new()
        .prefix("aos-inference-core-")
        .tempdir_in(base)
        .unwrap();
    let db_path = dir.path().join("db.sqlite3");
    let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();
    // Keep the tempdir alive for the lifetime of the test database
    let _db_dir = dir.keep();
    // Seed tenant
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let general = general_determinism_mode.map(|mode| GeneralConfig {
        system_name: None,
        environment: None,
        api_base_url: None,
        determinism_mode: Some(mode),
    });

    let config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: use_session_stack,
        capacity_limits: Default::default(),
        general,
        server: Default::default(),
        security: Default::default(),
        auth: Default::default(),
        self_hosting: Default::default(),
        performance: Default::default(),
        streaming: Default::default(),
        paths: PathsConfig {
            artifacts_root: "var/artifacts".into(),
            bundles_root: "var/bundles".into(),
            adapters_root: "var/adapters/repo".into(),
            plan_dir: "var/plan".into(),
            datasets_root: "var/datasets".into(),
            documents_root: "var/documents".into(),
            synthesis_model_path: None,
        },
        chat_context: Default::default(),
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Auto,
        worker_id: 0,
        rate_limit: None,
    }));

    let metrics_exporter = Arc::new(MetricsExporter::new(vec![0.1]).unwrap());
    let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let uma_monitor = Arc::new(adapteros_lora_worker::memory::UmaPressureMonitor::new(
        15, None,
    ));

    AppState::new(
        db,
        b"test-jwt-secret-for-effective-adapters".to_vec(),
        config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        uma_monitor,
    )
    .with_manifest_info("test-manifest-hash".to_string(), "mlx".to_string())
}

#[tokio::test]
async fn test_implicit_policy_uses_global_strict() {
    let state = build_test_state_with_general(false, Some(DeterminismMode::Strict)).await;

    let policy = resolve_tenant_execution_policy(
        &state.db,
        &state.config.read().unwrap(),
        "tenant-1",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(policy.effective_determinism_mode, DeterminismMode::Strict);
}

#[tokio::test]
async fn test_explicit_policy_overrides_global_strict() {
    let state = build_test_state_with_general(false, Some(DeterminismMode::Strict)).await;

    let determinism = DeterminismPolicy {
        allowed_modes: vec![
            "strict".to_string(),
            "besteffort".to_string(),
            "relaxed".to_string(),
        ],
        default_mode: "relaxed".to_string(),
        require_seed: false,
        allow_fallback: true,
        replay_mode: "approximate".to_string(),
        allowed_backends: Some(Vec::new()),
        denied_backends: Some(Vec::new()),
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    state
        .db
        .create_execution_policy("tenant-1", request, Some("test"))
        .await
        .unwrap();

    let policy = resolve_tenant_execution_policy(
        &state.db,
        &state.config.read().unwrap(),
        "tenant-1",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(policy.effective_determinism_mode, DeterminismMode::Relaxed);
}
