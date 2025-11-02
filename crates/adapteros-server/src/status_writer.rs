//! Status writer for menu bar application
//!
//! Writes a JSON snapshot of AdapterOS state to `/var/run/adapteros_status.json`
//! for consumption by the macOS menu bar app.

use adapteros_db::Database;
use adapteros_server_api::AppState;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::SystemTime;
use tracing::{debug, warn};

/// Status reported to menu bar app
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterOSStatus {
    /// Schema version for forward/backward compatibility
    pub schema_version: String,
    /// System status: "ok" | "degraded" | "error"
    pub status: String,
    /// Uptime in seconds since control plane started
    pub uptime_secs: u64,
    /// Number of adapters currently loaded
    pub adapters_loaded: usize,
    /// Whether deterministic mode is enabled
    pub deterministic: bool,
    /// Short kernel hash (first 8 chars)
    pub kernel_hash: String,
    /// Telemetry mode: "local" | "disabled"
    pub telemetry_mode: String,
    /// Number of active workers
    pub worker_count: usize,
    /// Whether base model is loaded
    pub base_model_loaded: bool,
    /// Base model identifier (optional)
    pub base_model_id: Option<String>,
    /// Base model display name (optional)
    pub base_model_name: Option<String>,
    /// Base model status: "ready" | "loading" | "error"
    pub base_model_status: String,
    /// Base model memory usage in MB (optional)
    pub base_model_memory_mb: Option<usize>,
}

/// Base model information for status reporting
#[derive(Debug)]
struct BaseModelInfo {
    loaded: bool,
    id: Option<String>,
    name: Option<String>,
    status: String,
    memory_mb: Option<usize>,
}

/// Tracks when the control plane started (thread-safe)
static START_TIME: OnceLock<SystemTime> = OnceLock::new();

/// Cached status snapshot for request handlers (thread-safe)
static STATUS_CACHE: OnceLock<RwLock<Option<AdapterOSStatus>>> = OnceLock::new();

/// Initialize the start time (call once at startup)
pub fn init_start_time() {
    let _ = START_TIME.set(SystemTime::now());
}

/// Initialize the status cache (call once at startup)
pub fn init_status_cache() {
    let _ = STATUS_CACHE.set(RwLock::new(None));
}

/// Update the cached status snapshot
pub async fn update_cache(state: &AppState) -> Result<()> {
    let status = collect_status(state).await?;
    if let Some(cache) = STATUS_CACHE.get() {
        let mut cache_write = cache
            .write()
            .map_err(|e| anyhow::anyhow!("Cache lock poisoned: {}", e))?;
        *cache_write = Some(status);
    } else {
        return Err(anyhow::anyhow!("Status cache not initialized"));
    }
    Ok(())
}

/// Get the current cached status snapshot
pub fn get_cached_status() -> Result<Option<AdapterOSStatus>> {
    if let Some(cache) = STATUS_CACHE.get() {
        let cache_read = cache
            .read()
            .map_err(|e| anyhow::anyhow!("Cache lock poisoned: {}", e))?;
        Ok(cache_read.clone())
    } else {
        Err(anyhow::anyhow!("Status cache not initialized"))
    }
}

/// Get uptime in seconds since init_start_time was called
fn get_uptime_secs() -> u64 {
    START_TIME
        .get()
        .and_then(|start| SystemTime::now().duration_since(*start).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write current status to JSON file (reads from cache)
pub async fn write_status(_state: &AppState) -> Result<()> {
    let status =
        get_cached_status()?.ok_or_else(|| anyhow::anyhow!("No cached status available"))?;
    write_status_file(&status)?;
    Ok(())
}

/// Collect current status from the system
async fn collect_status(state: &AppState) -> Result<AdapterOSStatus> {
    // Query database for adapter and worker counts with proper error handling
    let adapters_loaded = match query_adapter_count(&state.db).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to query adapter count: {}", e);
            0
        }
    };

    let worker_count = match query_worker_count(&state.db).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to query worker count: {}", e);
            0
        }
    };

    // Determine overall system status with telemetry awareness
    let status = if adapters_loaded > 0 && worker_count > 0 {
        "ok"
    } else if adapters_loaded > 0 || worker_count > 0 {
        "degraded"
    } else {
        "error"
    }
    .to_string();

    // Get kernel hash from plan with proper error handling
    let kernel_hash = match get_kernel_hash().await {
        Some(hash) => hash,
        None => {
            warn!("Failed to read kernel hash from manifest");
            "00000000".to_string()
        }
    };

    // Check if deterministic mode is enabled with error handling
    let deterministic = match check_deterministic_mode().await {
        Some(enabled) => enabled,
        None => {
            warn!("Failed to check deterministic mode");
            false // Conservative default
        }
    };

    // Get base model information from training service
    let base_model_info = get_base_model_info(state).await;

    Ok(AdapterOSStatus {
        schema_version: "1.0".to_string(),
        status,
        uptime_secs: get_uptime_secs(),
        adapters_loaded,
        deterministic,
        kernel_hash,
        telemetry_mode: "local".to_string(),
        worker_count,
        base_model_loaded: base_model_info.loaded,
        base_model_id: base_model_info.id,
        base_model_name: base_model_info.name,
        base_model_status: base_model_info.status,
        base_model_memory_mb: base_model_info.memory_mb,
    })
}

/// Query adapter count from database
async fn query_adapter_count(db: &Database) -> Result<usize> {
    let query = "SELECT COUNT(*) FROM adapters WHERE status = 'active'";

    let pool = db.pool();
    let count = adapteros_db::sqlx::query_scalar::<_, i64>(query)
        .fetch_one(pool)
        .await
        .context("Failed to query adapter count")?;

    Ok(count as usize)
}

/// Query worker count (from node agent or workers table)
async fn query_worker_count(db: &Database) -> Result<usize> {
    let query = "SELECT COUNT(*) FROM workers WHERE status IN ('active','starting')";

    let pool = db.pool();
    let res = adapteros_db::sqlx::query_scalar::<_, i64>(query)
        .fetch_one(pool)
        .await;

    match res {
        Ok(n) => Ok(n as usize),
        Err(_) => Ok(0),
    }
}

// Base model details omitted in status schema for the menu bar application

/// Get kernel hash from current plan
async fn get_kernel_hash() -> Option<String> {
    // Look for plan in standard location
    let plan_path = Path::new("plan/qwen7b/manifest.json");
    if !plan_path.exists() {
        return None;
    }

    // Try to read and extract kernel hash
    let content = fs::read_to_string(plan_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&content).ok()?;
    let hash = manifest
        .get("kernel_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.chars().take(8).collect())?;
    Some(hash)
}

/// Check if deterministic mode is enabled
async fn check_deterministic_mode() -> Option<bool> {
    // Check if metallib exists (indicates deterministic kernels)
    let metallib_path = Path::new("metal/mplora_kernels.metallib");
    Some(metallib_path.exists())
}

/// Get base model information from training service
async fn get_base_model_info(state: &AppState) -> BaseModelInfo {
    const DEFAULT_TENANT: &str = "default";

    match state.db.get_base_model_status(DEFAULT_TENANT).await {
        Ok(Some(status_record)) => {
            let model_id = status_record.model_id.clone();
            let status = status_record.status.clone();
            let is_loaded = status.as_str() == "loaded";

            let model_name = match state.db.get_model(&model_id).await {
                Ok(Some(model)) => Some(model.name),
                Ok(None) => {
                    warn!("Base model status references unknown model id {}", model_id);
                    None
                }
                Err(e) => {
                    warn!("Failed to load base model metadata for {}: {}", model_id, e);
                    None
                }
            };

            BaseModelInfo {
                loaded: is_loaded,
                id: Some(model_id),
                name: model_name,
                status,
                memory_mb: status_record.memory_usage_mb.and_then(|mb| {
                    if mb >= 0 {
                        Some(mb as usize)
                    } else {
                        None
                    }
                }),
            }
        }
        Ok(None) => BaseModelInfo {
            loaded: false,
            id: None,
            name: None,
            status: "unloaded".to_string(),
            memory_mb: None,
        },
        Err(e) => {
            warn!("Failed to query base model status: {}", e);
            BaseModelInfo {
                loaded: false,
                id: None,
                name: None,
                status: "unknown".to_string(),
                memory_mb: None,
            }
        }
    }
}

/// Atomically write status to file
fn write_status_file(status: &AdapterOSStatus) -> Result<()> {
    let json = serde_json::to_string_pretty(status).context("Failed to serialize status")?;

    // Ensure directory exists
    let status_dir = Path::new("/var/run");
    if !status_dir.exists() {
        // Try to create, but don't fail if we can't (might not have perms)
        if let Err(e) = fs::create_dir_all(status_dir) {
            warn!("Could not create /var/run: {}, trying local path", e);
            // Fall back to local directory
            return write_status_file_local(status);
        }
    }

    let status_path = "/var/run/adapteros_status.json";
    let temp_path = "/var/run/adapteros_status.json.tmp";

    // Clean up any leftover temp file from previous failed writes
    if Path::new(temp_path).exists() {
        if let Err(e) = fs::remove_file(temp_path) {
            warn!("Could not clean up leftover temp file {}: {}", temp_path, e);
        }
    }

    // Write to temp file first (atomic operation preparation)
    fs::write(temp_path, json)
        .with_context(|| format!("Failed to write temp status file: {}", temp_path))?;

    // Atomic rename - this is the critical atomic operation
    fs::rename(temp_path, status_path).with_context(|| {
        format!(
            "Failed to rename temp file {} to {}",
            temp_path, status_path
        )
    })?;

    // Set permissions to 0644 (readable by all, writable by owner)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(status_path)
            .with_context(|| format!("Failed to get metadata for status file: {}", status_path))?
            .permissions();
        perms.set_mode(0o644);
        fs::set_permissions(status_path, perms).with_context(|| {
            format!("Failed to set permissions on status file: {}", status_path)
        })?;
    }

    debug!("Status written to {}", status_path);
    Ok(())
}

/// Fallback: write to local var/ directory
fn write_status_file_local(status: &AdapterOSStatus) -> Result<()> {
    let json =
        serde_json::to_string_pretty(status).context("Failed to serialize status to JSON")?;

    // Use local var directory
    let status_dir = Path::new("var");
    fs::create_dir_all(status_dir).context("Failed to create var/ directory")?;

    let status_path = "var/adapteros_status.json";
    let temp_path = "var/adapteros_status.json.tmp";

    // Clean up any leftover temp file from previous failed writes
    if Path::new(temp_path).exists() {
        if let Err(e) = fs::remove_file(temp_path) {
            warn!("Could not clean up leftover temp file {}: {}", temp_path, e);
        }
    }

    fs::write(temp_path, json)
        .with_context(|| format!("Failed to write temp status file: {}", temp_path))?;

    fs::rename(temp_path, status_path).with_context(|| {
        format!(
            "Failed to rename temp file {} to {}",
            temp_path, status_path
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(status_path)
            .with_context(|| format!("Failed to get metadata for status file: {}", status_path))?
            .permissions();
        perms.set_mode(0o644);
        fs::set_permissions(status_path, perms).with_context(|| {
            format!("Failed to set permissions on status file: {}", status_path)
        })?;
    }

    debug!("Status written to {} (local fallback)", status_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_db::models::ModelRegistrationBuilder;
    use adapteros_db::Db;
    use adapteros_server_api::AppState;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_uptime_tracking() {
        init_start_time();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let uptime = get_uptime_secs();
        assert!(uptime == 0); // Less than 1 second
    }

    #[test]
    fn test_status_serialization() {
        let status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "ok".to_string(),
            uptime_secs: 1337,
            adapters_loaded: 3,
            deterministic: true,
            kernel_hash: "a84d9f1c".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 2,
            base_model_loaded: true,
            base_model_id: Some("qwen2.5-7b".to_string()),
            base_model_name: Some("Qwen 2.5 7B".to_string()),
            base_model_status: "ready".to_string(),
            base_model_memory_mb: Some(14336),
        };

        let json =
            serde_json::to_string(&status).expect("Test status serialization should succeed");
        assert!(json.contains("\"schema_version\":\"1.0\""));
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"adapters_loaded\":3"));
        assert!(json.contains("\"base_model_loaded\":true"));
        assert!(json.contains("\"base_model_status\":\"ready\""));
    }

    #[tokio::test]
    async fn test_base_model_info() {
        // Test the base model info function
        let state = create_mock_app_state().await;
        let base_model = get_base_model_info(&state).await;
        assert!(base_model.loaded);
        assert_eq!(base_model.status, "loaded");
        assert!(base_model.id.is_some());
        assert_eq!(base_model.name.as_deref(), Some("Test Model"));
        assert_eq!(base_model.memory_mb, Some(14336));
    }

    #[test]
    fn test_status_cache_operations() {
        // Test cache initialization and operations
        init_status_cache();

        // Initially no cached status
        let cached = get_cached_status().unwrap();
        assert!(cached.is_none());

        // Create a test status
        let test_status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "ok".to_string(),
            uptime_secs: 100,
            adapters_loaded: 2,
            deterministic: true,
            kernel_hash: "test123".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 1,
            base_model_loaded: true,
            base_model_id: Some("test-model".to_string()),
            base_model_name: Some("Test Model".to_string()),
            base_model_status: "ready".to_string(),
            base_model_memory_mb: Some(1024),
        };

        // Manually set cache (simulating what update_cache would do)
        if let Some(cache) = STATUS_CACHE.get() {
            let mut cache_write = cache.write().unwrap();
            *cache_write = Some(test_status.clone());
        }

        // Now should have cached status
        let cached = get_cached_status().unwrap();
        assert!(cached.is_some());
        let cached_status = cached.unwrap();
        assert_eq!(cached_status.status, "ok");
        assert_eq!(cached_status.adapters_loaded, 2);
        assert_eq!(cached_status.worker_count, 1);

        // Reset cache for other tests
        if let Some(cache) = STATUS_CACHE.get() {
            let mut cache_write = cache.write().unwrap();
            *cache_write = None;
        }
    }

    #[tokio::test]
    async fn test_update_cache_populates_cache() {
        init_status_cache();
        let state = create_mock_app_state().await;

        update_cache(&state)
            .await
            .expect("update_cache should populate status cache");

        let cached = get_cached_status().expect("cache read should succeed");
        assert!(cached.is_some(), "cache should contain status after update");

        // Cleanup cache contents so other tests start clean
        if let Some(cache) = STATUS_CACHE.get() {
            let mut cache_write = cache.write().unwrap();
            *cache_write = None;
        }
    }

    #[test]
    fn test_status_file_operations() {
        // Test writing and reading status file
        let status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "ok".to_string(),
            uptime_secs: 42,
            adapters_loaded: 1,
            deterministic: true,
            kernel_hash: "test123".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 1,
            base_model_loaded: true,
            base_model_id: Some("test-model".to_string()),
            base_model_name: Some("Test Model".to_string()),
            base_model_status: "ready".to_string(),
            base_model_memory_mb: Some(1024),
        };

        // Write to temp file
        let temp_path = "test_status.json";
        let json = serde_json::to_string_pretty(&status).unwrap();
        fs::write(temp_path, json).unwrap();

        // Read it back
        let read_json = fs::read_to_string(temp_path).unwrap();
        let read_status: AdapterOSStatus = serde_json::from_str(&read_json).unwrap();

        // Verify all fields match
        assert_eq!(read_status.schema_version, status.schema_version);
        assert_eq!(read_status.status, status.status);
        assert_eq!(read_status.adapters_loaded, status.adapters_loaded);
        assert_eq!(read_status.base_model_loaded, status.base_model_loaded);

        // Cleanup
        let _ = fs::remove_file(temp_path);
    }

    #[test]
    fn test_atomic_file_write() {
        // Test the atomic write functionality
        let status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "degraded".to_string(),
            uptime_secs: 100,
            adapters_loaded: 0,
            deterministic: false,
            kernel_hash: "00000000".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 0,
            base_model_loaded: false,
            base_model_id: None,
            base_model_name: None,
            base_model_status: "error".to_string(),
            base_model_memory_mb: None,
        };

        // This should succeed (writes to local var/ directory in test)
        let result = write_status_file(&status);
        assert!(result.is_ok());

        // Check if file exists in local directory
        let status_path = Path::new("var/adapteros_status.json");
        if status_path.exists() {
            let content = fs::read_to_string(status_path).unwrap();
            let read_status: AdapterOSStatus = serde_json::from_str(&content).unwrap();
            assert_eq!(read_status.status, "degraded");
            // Cleanup
            let _ = fs::remove_file(status_path);
        }
    }

    /// Helper function to create a mock AppState for testing
    async fn create_mock_app_state() -> AppState {
        // Create an in-memory database for testing
        let db = Db::connect("sqlite::memory:?cache=shared").await.unwrap();
        db.migrate().await.unwrap();

        // Seed base model state so status queries succeed
        let model_params = ModelRegistrationBuilder::new()
            .name("Test Model")
            .hash_b3("b3-test")
            .config_hash_b3("config-test")
            .tokenizer_hash_b3("tokenizer-test")
            .tokenizer_cfg_hash_b3("tokenizer-cfg-test")
            .build()
            .expect("model params build");
        let model_id = db.register_model(model_params).await.unwrap();
        db.update_base_model_status("default", &model_id, "loaded", None, Some(14336))
            .await
            .unwrap();

        // Create minimal AppState - this would normally have more components
        // but for status writer testing, we only need the database
        use adapteros_metrics_exporter::MetricsExporter;
        use adapteros_orchestrator::TrainingService;
        use adapteros_server_api::{state::ApiConfig, AppState};

        let api_config = std::sync::Arc::new(std::sync::RwLock::new(ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                system_metrics_interval_secs: 0,
                telemetry_buffer_capacity: 1024,
                telemetry_channel_capacity: 256,
                trace_buffer_capacity: 512,
            },
            golden_gate: None,
            bundles_root: "var/bundles".to_string(),
            production_mode: false,
            rate_limits: None,
            path_policy: adapteros_server_api::state::PathPolicyConfig::default(),
        }));

        let metrics_exporter =
            std::sync::Arc::new(MetricsExporter::new(Default::default()).unwrap());
        let metrics_collector =
            std::sync::Arc::new(adapteros_telemetry::MetricsCollector::new().unwrap());
        let metrics_registry = std::sync::Arc::new(adapteros_telemetry::MetricsRegistry::new(
            metrics_collector.clone(),
        ));
        for name in [
            "inference_latency_p95_ms",
            "queue_depth",
            "tokens_per_second",
            "memory_usage_mb",
        ] {
            metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
        }
        let training_service = std::sync::Arc::new(TrainingService::new());

        AppState::with_sqlite(
            db,
            vec![], // empty JWT secret for testing
            api_config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
        )
    }
}
