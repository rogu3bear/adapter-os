use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::Db;
use adapteros_manifest::ManifestV3;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::config::PathsConfig;
use adapteros_server_api::handlers::worker_manifests::{
    fetch_manifest_by_hash, WorkerManifestPath,
};
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};
use adapteros_server_api::telemetry::MetricsRegistry;
use axum::{extract::Path, extract::State, Json};
use std::sync::{Arc, RwLock};

#[tokio::test]
async fn fetch_manifest_by_hash_returns_yaml_and_hash() {
    use adapteros_lora_worker::memory::UmaPressureMonitor;

    let manifest_yaml = include_str!("../../../manifests/qwen32b-coder-mlx.yaml");
    let manifest: ManifestV3 = serde_yaml::from_str(manifest_yaml).expect("parse manifest fixture");
    let manifest_hash = manifest
        .compute_hash()
        .expect("compute manifest hash")
        .to_hex();
    let manifest_json = manifest.to_json().expect("serialize manifest to json");
    println!("manifest_hash={}", manifest_hash);

    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('default','Default',0)")
        .execute(db.pool())
        .await
        .unwrap();

    let api_config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: false,
        capacity_limits: Default::default(),
        general: None,
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
        timeouts: Default::default(),
        rate_limit: None,
    }));

    let metrics_exporter = Arc::new(MetricsExporter::new(vec![0.1]).unwrap());
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
        Default::default(),
    ));
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

    let state = AppState::new(
        db,
        b"test-jwt-secret-key-32-bytes-long".to_vec(),
        api_config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        uma_monitor,
    )
    .with_manifest_info(manifest_hash.clone(), "mlx".to_string());

    state
        .db
        .create_manifest("default", &manifest_hash, &manifest_json)
        .await
        .expect("insert manifest");

    let response = fetch_manifest_by_hash(
        State(state),
        Path(WorkerManifestPath {
            tenant_id: "default".to_string(),
            manifest_hash: manifest_hash.clone(),
        }),
    )
    .await
    .expect("fetch manifest");

    let Json(body) = response;
    assert_eq!(body.manifest_hash, manifest_hash);
    assert!(body.manifest_yaml.contains("schema: adapteros.manifest.v3"));
}
