#![allow(dead_code)]

pub mod test_failure_bundle;
#[allow(unused_imports)]
pub use test_failure_bundle::*;

use std::sync::{Arc, RwLock};
use std::{env, path::Path, path::PathBuf};

use adapteros_api_types::{workers::WorkerCapabilities, API_SCHEMA_VERSION};
use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_db::Db;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType};
use adapteros_server_api::config::PathsConfig;
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_telemetry::MetricsCollector;
use once_cell::sync::Lazy;
use tokio::sync::{Mutex, MutexGuard};

/// Global lock to serialize environment mutations across tests.
pub static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub async fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

/// Clears all testkit-related environment flags so routing is deterministic.
pub fn clear_testkit_env() {
    for key in [
        "E2E_MODE",
        "AOS_SKIP_MIGRATION_SIGNATURES",
        "AOS_DEV_NO_AUTH",
        "VITE_ENABLE_DEV_BYPASS",
    ] {
        std::env::remove_var(key);
    }
}

/// Enables testkit mode and optional dev-no-auth for fixtures.
pub fn set_testkit_env(dev_no_auth: bool) {
    clear_testkit_env();
    std::env::set_var("E2E_MODE", "1");
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    if dev_no_auth {
        std::env::set_var("AOS_DEV_NO_AUTH", "1");
    } else {
        std::env::remove_var("AOS_DEV_NO_AUTH");
    }
}

/// RAII guard that holds the environment lock and resets flags on drop.
pub struct TestkitEnvGuard {
    _guard: MutexGuard<'static, ()>,
}

impl TestkitEnvGuard {
    pub async fn disabled() -> Self {
        let guard = env_lock().await;
        clear_testkit_env();
        Self { _guard: guard }
    }

    pub async fn enabled(dev_no_auth: bool) -> Self {
        let guard = env_lock().await;
        set_testkit_env(dev_no_auth);
        Self { _guard: guard }
    }
}

impl Drop for TestkitEnvGuard {
    fn drop(&mut self) {
        clear_testkit_env();
    }
}

/// Build a minimal AppState with in-memory DB, metrics, and training service.
///
/// Creates all required dependencies for integration testing:
/// - In-memory SQLite database with migrations applied
/// - Default tenant created for test isolation
/// - Metrics infrastructure (collector, registry, exporter)
/// - UMA pressure monitor for memory management
#[allow(dead_code)]
pub async fn setup_state(_uds_path: Option<&PathBuf>) -> anyhow::Result<AppState> {
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    // 1. Create in-memory database with migrations
    let db = Db::new_in_memory()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create in-memory DB: {}", e))?;

    // 2. Create test tenants
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('default', 'Default Tenant')",
    )
    .execute(db.pool_result()?)
    .await?;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant 1')",
    )
    .execute(db.pool_result()?)
    .await?;

    // Seed users referenced by claims to satisfy FK constraints in chat sessions
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("tenant-1-user")
    .bind("user@example.com")
    .bind("Tenant One User")
    .bind("test-hash")
    .bind("admin")
    .execute(db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("viewer-user-id")
    .bind("viewer@example.com")
    .bind("Default Viewer")
    .bind("test-hash")
    .bind("viewer")
    .execute(db.pool_result()?)
    .await?;

    // Dev bypass user (used when AOS_DEV_NO_AUTH=1)
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("dev-no-auth")
    .bind("dev-no-auth@adapteros.local")
    .bind("Dev Bypass User")
    .bind("dev-hash")
    .bind("admin")
    .execute(db.pool_result()?)
    .await?;

    // Create workspace for default tenant (required for dev bypass auth)
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, created_by) VALUES (?, ?, ?)",
    )
    .bind("default")
    .bind("Default Workspace")
    .bind("dev-no-auth")
    .execute(db.pool_result()?)
    .await?;

    // Create workspace and membership for tenant-scoped auth tests.
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, created_by) VALUES (?, ?, ?)",
    )
    .bind("tenant-1")
    .bind("Tenant 1 Workspace")
    .bind("tenant-1-user")
    .execute(db.pool_result()?)
    .await?;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO workspace_members (id, workspace_id, tenant_id, user_id, role, permissions_json, added_by) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("wsm-tenant-1-user")
    .bind("tenant-1")
    .bind("tenant-1")
    .bind("tenant-1-user")
    .bind("owner")
    .bind::<Option<&str>>(None)
    .bind("tenant-1-user")
    .execute(db.pool_result()?)
    .await?;

    // 3. Create test JWT secret
    let jwt_secret = b"test-jwt-secret-for-integration-tests-32bytes!".to_vec();

    // 4. Create isolated filesystem roots for tests in OS temp directory
    // Leak the TempDir to prevent cleanup during test - OS will clean on reboot
    let base_tempdir = tempfile::TempDir::with_prefix("aos-test-server-api-")?;
    let base_dir = base_tempdir.keep();
    let artifacts_root = base_dir.join("artifacts");
    let bundles_root = base_dir.join("bundles");
    let adapters_root = base_dir.join("adapters");
    let plan_dir = base_dir.join("plan");
    let datasets_root = base_dir.join("datasets");
    let documents_root = base_dir.join("documents");
    for dir in [
        &artifacts_root,
        &bundles_root,
        &adapters_root,
        &plan_dir,
        &datasets_root,
        &documents_root,
    ] {
        std::fs::create_dir_all(dir)?;
    }

    // 5. Create API config with all required fields
    let config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test-bearer-token".to_string(),
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
            artifacts_root: artifacts_root.to_string_lossy().to_string(),
            bundles_root: bundles_root.to_string_lossy().to_string(),
            adapters_root: adapters_root.to_string_lossy().to_string(),
            plan_dir: plan_dir.to_string_lossy().to_string(),
            datasets_root: datasets_root.to_string_lossy().to_string(),
            documents_root: documents_root.to_string_lossy().to_string(),
            synthesis_model_path: None,
            training_worker_bin: None,
        },
        chat_context: Default::default(),
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Auto,
        worker_id: 0,
        timeouts: Default::default(),
        rate_limit: None,
        inference_cache: Default::default(),
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
    let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
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

pub async fn register_test_model(state: &AppState, model_path: &Path) -> anyhow::Result<String> {
    let model_name = format!("test-model-{}", uuid::Uuid::new_v4());
    let model_hash = adapteros_core::B3Hash::hash(model_name.as_bytes()).to_hex();
    let params = ModelRegistrationBuilder::new()
        .name(model_name)
        .hash_b3(model_hash)
        .config_hash_b3("config-hash")
        .tokenizer_hash_b3("tok-hash")
        .tokenizer_cfg_hash_b3("tok-cfg-hash")
        .build()?;
    let model_id = state.db.register_model(params).await?;
    // Tests often create tenant-scoped repositories; mark seeded models as shared
    // to satisfy base_model tenant guards consistently.
    adapteros_db::sqlx::query("UPDATE models SET tenant_id = 'system' WHERE id = ?")
        .bind(&model_id)
        .execute(state.db.pool_result()?)
        .await?;
    state
        .db
        .update_model_path(&model_id, model_path.to_str().unwrap_or_default())
        .await?;
    Ok(model_id)
}

pub async fn register_test_worker(
    state: &AppState,
    tenant_id: &str,
    caps: WorkerCapabilities,
) -> anyhow::Result<String> {
    let worker_id = format!("worker-{}", uuid::Uuid::new_v4());
    let node_id = format!("node-{}", worker_id);
    let plan_id = format!("plan-{}", worker_id);
    let manifest_hash = format!("manifest-{}", worker_id);
    let uds_path = format!("var/run/{}/worker.sock", worker_id);

    let uds_path_buf = PathBuf::from(&uds_path);
    if let Some(parent) = uds_path_buf.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&uds_path_buf);
    #[cfg(unix)]
    {
        // Create a real Unix socket node so selector availability checks pass in integration tests.
        let listener = std::os::unix::net::UnixListener::bind(&uds_path_buf)?;
        drop(listener);
    }
    #[cfg(not(unix))]
    {
        std::fs::File::create(&uds_path_buf)?;
    }

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind(&node_id)
    .bind("test-node")
    .bind("http://localhost:0")
    .execute(state.db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind(&manifest_hash)
    .bind(tenant_id)
    .bind(&manifest_hash)
    .bind("{}")
    .execute(state.db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&plan_id)
    .bind(tenant_id)
    .bind(format!("plan-b3:{}", worker_id))
    .bind(&manifest_hash)
    .bind("[]")
    .bind("layout-b3:test")
    .execute(state.db.pool_result()?)
    .await?;

    let params = WorkerRegistrationParams {
        worker_id: worker_id.clone(),
        tenant_id: tenant_id.to_string(),
        node_id: node_id.clone(),
        plan_id: plan_id.clone(),
        uds_path,
        pid: 1234,
        manifest_hash,
        backend: Some(caps.backend_kind.clone()),
        model_hash_b3: None,
        tokenizer_hash_b3: None,
        tokenizer_vocab_size: None,
        capabilities_json: Some(serde_json::to_string(&caps)?),
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    state.db.register_worker(params).await?;
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "test", None)
        .await?;

    Ok(worker_id)
}

/// Standard admin claims for tests
pub fn test_admin_claims() -> Claims {
    Claims {
        sub: "tenant-1-user".to_string(),
        email: "user@example.com".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "tenant-1".to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: 0,
        iat: 0,
        jti: "test-token".to_string(),
        nbf: 0,
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

/// Standard viewer claims for tests
pub fn test_viewer_claims() -> Claims {
    Claims {
        sub: "viewer-user-id".to_string(),
        email: "viewer@example.com".to_string(),
        role: "viewer".to_string(),
        roles: vec!["viewer".to_string()],
        tenant_id: "default".to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: 9999999999,
        iat: 0,
        jti: "test-viewer-token".to_string(),
        nbf: 0,
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

/// Standard operator claims for tests
pub fn test_operator_claims() -> Claims {
    Claims {
        sub: "operator-user-id".to_string(),
        email: "operator@example.com".to_string(),
        role: "operator".to_string(),
        roles: vec!["operator".to_string()],
        tenant_id: "default".to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: 9999999999,
        iat: 0,
        jti: "test-operator-token".to_string(),
        nbf: 0,
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

/// Standard compliance claims for tests
pub fn test_compliance_claims() -> Claims {
    Claims {
        sub: "compliance-user-id".to_string(),
        email: "compliance@example.com".to_string(),
        role: "compliance".to_string(),
        roles: vec!["compliance".to_string()],
        tenant_id: "default".to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: 9999999999,
        iat: 0,
        jti: "test-compliance-token".to_string(),
        nbf: 0,
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
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
    .execute(state.db.pool_result()?)
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

/// Create a test adapter in the database
pub async fn create_test_adapter(
    state: &AppState,
    adapter_id: &str,
    tenant_id: &str,
    tier: &str,
    rank: i32,
) -> anyhow::Result<()> {
    use adapteros_core::B3Hash;
    use adapteros_db::adapters::AdapterRegistrationBuilder;

    let hash = B3Hash::hash(adapter_id.as_bytes()).to_hex();
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(hash)
        .rank(rank)
        .tier(tier)
        .category("code")
        .scope("global")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build adapter params: {}", e))?;

    state.db.register_adapter(params).await?;

    Ok(())
}

/// Create test adapter with defaults (tier_1, rank=16)
pub async fn create_test_adapter_default(
    state: &AppState,
    adapter_id: &str,
    tenant_id: &str,
) -> anyhow::Result<()> {
    create_test_adapter(state, adapter_id, tenant_id, "persistent", 16).await
}

/// Create a test dataset in the database
pub async fn create_test_dataset(state: &AppState, dataset_id: &str) -> anyhow::Result<()> {
    use adapteros_core::B3Hash;

    let hash = B3Hash::hash(dataset_id.as_bytes()).to_hex();

    adapteros_db::sqlx::query(
        "INSERT INTO training_datasets (id, name, hash_b3, validation_status, format, storage_path)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(dataset_id)
    .bind(dataset_id)
    .bind(hash)
    .bind("valid")
    .bind("jsonl")
    .bind("var/test-datasets")
    .execute(state.db.pool_result()?)
    .await?;

    Ok(())
}

/// Create a test tenant in the database
pub async fn create_test_tenant(
    state: &AppState,
    tenant_id: &str,
    name: &str,
) -> anyhow::Result<()> {
    adapteros_db::sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(name)
        .execute(state.db.pool_result()?)
        .await?;

    Ok(())
}

/// Cleanup: delete a test adapter
pub async fn delete_test_adapter(state: &AppState, adapter_id: &str) -> anyhow::Result<()> {
    adapteros_db::sqlx::query("DELETE FROM adapters WHERE id = ?")
        .bind(adapter_id)
        .execute(state.db.pool_result()?)
        .await?;

    Ok(())
}

/// Cleanup: delete a test dataset
pub async fn delete_test_dataset(state: &AppState, dataset_id: &str) -> anyhow::Result<()> {
    adapteros_db::sqlx::query("DELETE FROM training_datasets WHERE id = ?")
        .bind(dataset_id)
        .execute(state.db.pool_result()?)
        .await?;

    Ok(())
}

/// Cleanup: delete a test training job
pub async fn delete_test_training_job(state: &AppState, job_id: &str) -> anyhow::Result<()> {
    adapteros_db::sqlx::query("DELETE FROM repository_training_jobs WHERE id = ?")
        .bind(job_id)
        .execute(state.db.pool_result()?)
        .await?;

    Ok(())
}
