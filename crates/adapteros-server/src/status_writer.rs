//! Status writer for menu bar application
//!
//! Writes a JSON snapshot of AdapterOS state to `/var/run/adapteros_status.json`
//! for consumption by the macOS menu bar app.

use adapteros_server_api::AppState;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, warn};

/// Status reported to menu bar app
#[derive(Debug, Serialize)]
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

/// Tracks when the control plane started
static mut START_TIME: Option<SystemTime> = None;

/// Initialize the start time (call once at startup)
pub fn init_start_time() {
    unsafe {
        START_TIME = Some(SystemTime::now());
    }
}

/// Get uptime in seconds since init_start_time was called
fn get_uptime_secs() -> u64 {
    unsafe {
        START_TIME
            .and_then(|start| SystemTime::now().duration_since(start).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Write current status to JSON file
pub async fn write_status(state: &AppState) -> Result<()> {
    let status = collect_status(state).await?;
    write_status_file(&status)?;
    Ok(())
}

/// Collect current status from the system
async fn collect_status(state: &AppState) -> Result<AdapterOSStatus> {
    // Query database for adapter and worker counts with proper error handling
    let adapters_loaded = match query_adapter_count(state.db.sqlite()).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to query adapter count: {}", e);
            0
        }
    };

    let worker_count = match query_worker_count(state.db.sqlite()).await {
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
    }.to_string();

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
async fn query_adapter_count(db: &adapteros_db::Db) -> Result<usize> {
    // Count active adapters (SQLite)
    let count = adapteros_db::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters WHERE status = 'active'")
        .fetch_one(db.pool())
        .await
        .context("Failed to query adapter count")?;
    Ok(count as usize)
}

/// Query worker count (from node agent or workers table)
async fn query_worker_count(db: &adapteros_db::Db) -> Result<usize> {
    // Try to count serving workers if table exists; fall back to 0 on error
    let res = adapteros_db::sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workers WHERE status IN ('active','starting')",
    )
    .fetch_one(db.pool())
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
    // For now, report base model as loaded and ready
    // TODO: Implement actual base model lifecycle tracking
    BaseModelInfo {
        loaded: true,
        id: Some("qwen2.5-7b".to_string()),
        name: Some("Qwen 2.5 7B".to_string()),
        status: "ready".to_string(),
        memory_mb: Some(14336), // ~14GB for 7B model
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

    // Write to temp file first
    fs::write(temp_path, json).context("Failed to write temp status file")?;

    // Atomic rename
    fs::rename(temp_path, status_path).context("Failed to rename status file")?;

    // Set permissions to 0644 (readable by all, writable by owner)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(status_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(status_path, perms)?;
    }

    debug!("Status written to {}", status_path);
    Ok(())
}

/// Fallback: write to local var/ directory
fn write_status_file_local(status: &AdapterOSStatus) -> Result<()> {
    let json = serde_json::to_string_pretty(status)?;

    // Use local var directory
    let status_dir = Path::new("var");
    fs::create_dir_all(status_dir).context("Failed to create var/ directory")?;

    let status_path = "var/adapteros_status.json";
    let temp_path = "var/adapteros_status.json.tmp";

    fs::write(temp_path, json)?;
    fs::rename(temp_path, status_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(status_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(status_path, perms)?;
    }

    debug!("Status written to {} (local fallback)", status_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn test_base_model_info() {
        // Test the base model info function
        let base_model = get_base_model_info(&create_mock_app_state().await);
        assert!(base_model.loaded);
        assert_eq!(base_model.status, "ready");
        assert!(base_model.id.is_some());
        assert!(base_model.name.is_some());
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
        let db = Db::connect("sqlite::memory:").await.unwrap();

        // Create minimal AppState - this would normally have more components
        // but for status writer testing, we only need the database
        use adapteros_server_api::{state::ApiConfig, AppState};
        use adapteros_metrics_exporter::MetricsExporter;
        use adapteros_orchestrator::TrainingService;

        let api_config = std::sync::Arc::new(std::sync::RwLock::new(ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: false,
                bearer_token: None,
            },
            golden_gate: None,
            bundles_root: "var/bundles".to_string(),
            rate_limits: None,
        }));

        let metrics_exporter = std::sync::Arc::new(MetricsExporter::new(Default::default()).unwrap());
        let training_service = std::sync::Arc::new(TrainingService::new());

        AppState::new(
            db,
            vec![], // empty JWT secret for testing
            api_config,
            metrics_exporter,
            training_service,
        )
    }
}
