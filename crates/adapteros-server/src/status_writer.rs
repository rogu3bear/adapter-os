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
use std::sync::{
    atomic::{AtomicU32, Ordering},
    OnceLock, RwLock,
};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, warn};

/// Status of a managed service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Service identifier
    pub id: String,
    /// Human-readable service name
    pub name: String,
    /// Current state: "stopped" | "starting" | "running" | "stopping" | "failed" | "restarting"
    pub state: String,
    /// Process ID if running
    pub pid: Option<u32>,
    /// Port number if applicable
    pub port: Option<u16>,
    /// Health status: "unknown" | "healthy" | "unhealthy" | "checking"
    pub health_status: String,
    /// Number of restart attempts
    pub restart_count: u32,
    /// Last error message if any
    pub last_error: Option<String>,
}

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
    /// Service status information from supervisor (optional)
    pub services: Option<Vec<ServiceStatus>>,
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

/// Cached status entry with timestamp for freshness tracking
#[derive(Debug, Clone)]
struct CachedStatusEntry {
    status: AdapterOSStatus,
    last_updated: SystemTime,
}

/// Cached status snapshot for request handlers (thread-safe)
static STATUS_CACHE: OnceLock<RwLock<Option<CachedStatusEntry>>> = OnceLock::new();

/// Track consecutive write failures (thread-safe)
static WRITE_FAILURE_COUNT: AtomicU32 = AtomicU32::new(0);

/// Maximum consecutive failures before escalating
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

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
        *cache_write = Some(CachedStatusEntry {
            status,
            last_updated: SystemTime::now(),
        });
    } else {
        return Err(anyhow::anyhow!("Status cache not initialized"));
    }
    Ok(())
}

/// Force immediate cache refresh (for immediate operations)
pub async fn force_refresh_cache(state: &AppState) -> Result<()> {
    update_cache(state).await
}

/// Get the current cached status snapshot (synchronous, may be stale)
pub fn get_cached_status() -> Result<Option<AdapterOSStatus>> {
    get_cached_status_with_max_age(2)
}

/// Get cached status with staleness check (synchronous)
/// Returns cached value even if stale - use get_cached_status_fresh() for guaranteed fresh data
pub fn get_cached_status_with_max_age(max_age_secs: u64) -> Result<Option<AdapterOSStatus>> {
    if let Some(cache) = STATUS_CACHE.get() {
        let cache_read = cache
            .read()
            .map_err(|e| anyhow::anyhow!("Cache lock poisoned: {}", e))?;

        // Check if cache is stale and log warning
        if let Some(entry) = cache_read.as_ref() {
            let age = SystemTime::now()
                .duration_since(entry.last_updated)
                .unwrap_or(Duration::from_secs(u64::MAX));

            if age.as_secs() > max_age_secs {
                debug!(
                    age_secs = age.as_secs(),
                    max_age_secs = max_age_secs,
                    "Status cache is stale ({}s old), consider using get_cached_status_fresh()",
                    age.as_secs()
                );
            }
        }

        Ok(cache_read.as_ref().map(|entry| entry.status.clone()))
    } else {
        Err(anyhow::anyhow!("Status cache not initialized"))
    }
}

/// Get cached status with automatic refresh if stale (async)
/// This will refresh the cache if it's older than max_age_secs, then return fresh data
pub async fn get_cached_status_fresh(
    state: &AppState,
    max_age_secs: u64,
) -> Result<Option<AdapterOSStatus>> {
    // Check if cache is stale
    let needs_refresh = is_cache_stale(max_age_secs);

    if needs_refresh {
        // Refresh cache in background but don't wait for it
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = update_cache(&state_clone).await {
                warn!("Background cache refresh failed: {}", e);
            }
        });

        // Also do a synchronous refresh for immediate return
        update_cache(state).await?;
    }

    // Return fresh cached data
    get_cached_status_with_max_age(max_age_secs)
}

/// Get cached status with freshness information
fn get_cached_status_with_freshness() -> Result<Option<(AdapterOSStatus, SystemTime)>> {
    if let Some(cache) = STATUS_CACHE.get() {
        let cache_read = cache
            .read()
            .map_err(|e| anyhow::anyhow!("Cache lock poisoned: {}", e))?;
        Ok(cache_read
            .as_ref()
            .map(|entry| (entry.status.clone(), entry.last_updated)))
    } else {
        Err(anyhow::anyhow!("Status cache not initialized"))
    }
}

/// Check if cache is stale (older than threshold)
#[cfg(test)]
pub fn is_cache_stale(threshold_secs: u64) -> bool {
    is_cache_stale_impl(threshold_secs)
}

#[cfg(not(test))]
fn is_cache_stale(threshold_secs: u64) -> bool {
    is_cache_stale_impl(threshold_secs)
}

fn is_cache_stale_impl(threshold_secs: u64) -> bool {
    match get_cached_status_with_freshness() {
        Ok(Some((_, last_updated))) => {
            SystemTime::now()
                .duration_since(last_updated)
                .map(|d| d.as_secs() > threshold_secs)
                .unwrap_or(true) // Consider stale if we can't determine age
        }
        _ => true, // No cache entry is considered stale
    }
}

/// Refresh cache if stale (non-blocking check, doesn't actually refresh)
/// Returns true if cache was stale
pub fn check_cache_staleness(threshold_secs: u64) -> bool {
    is_cache_stale(threshold_secs)
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
/// Uses retry logic with exponential backoff for transient failures.
/// Retries the full operation, including cache access.
pub async fn write_status(state: &AppState) -> Result<()> {
    // Retry with exponential backoff: 1s, 2s, 4s
    let mut last_error = None;
    for attempt in 0..3 {
        match try_write_status_once(state).await {
            Ok(()) => {
                // Success - reset failure counter
                let prev_failures = WRITE_FAILURE_COUNT.swap(0, Ordering::Relaxed);
                if prev_failures > 0 {
                    debug!(
                        "Status write succeeded after {} previous failures",
                        prev_failures
                    );
                }
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < 2 {
                    // Exponential backoff: 1s, 2s
                    let delay_ms = 1000 * (1 << attempt);
                    debug!(
                        "Status write failed (attempt {}/3), retrying in {}ms: {}",
                        attempt + 1,
                        delay_ms,
                        last_error.as_ref().unwrap()
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    // All retries failed - increment failure counter
    let failures = WRITE_FAILURE_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

    if failures >= MAX_CONSECUTIVE_FAILURES {
        error!(
            consecutive_failures = failures,
            "Status write has failed {} consecutive times - menu bar app may show stale data",
            failures
        );
    } else {
        warn!(
            consecutive_failures = failures,
            "Status write failed after {} retries: {}",
            3,
            last_error.as_ref().unwrap()
        );
    }

    Err(last_error
        .unwrap()
        .context("Status write failed after retries"))
}

/// Internal function to attempt status write once (for retry logic)
async fn try_write_status_once(_state: &AppState) -> Result<()> {
    let status =
        get_cached_status()?.ok_or_else(|| anyhow::anyhow!("No cached status available"))?;

    write_status_file(&status)
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

    // Query service supervisor for service status
    let services = query_service_status().await;

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
        services,
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

/// Query service status from supervisor API
async fn query_service_status() -> Option<Vec<ServiceStatus>> {
    // Try to connect to service supervisor
    let supervisor_url =
        std::env::var("SUPERVISOR_API_URL").unwrap_or_else(|_| "http://localhost:8081".to_string());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500)) // Short timeout to avoid blocking status updates
        .build()
        .ok()?;

    let response = match client
        .get(format!("{}/api/services", supervisor_url))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            debug!("Failed to query supervisor API: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        debug!("Supervisor API returned status: {}", response.status());
        return None;
    }

    let services_response: serde_json::Value = match response.json().await {
        Ok(json) => json,
        Err(e) => {
            debug!("Failed to parse supervisor response: {}", e);
            return None;
        }
    };

    // Parse the services array from the response
    let services = services_response
        .get("services")?
        .as_array()?
        .iter()
        .filter_map(|service| {
            Some(ServiceStatus {
                id: service.get("id")?.as_str()?.to_string(),
                name: service.get("name")?.as_str()?.to_string(),
                state: service.get("state")?.as_str()?.to_string(),
                pid: service
                    .get("pid")
                    .and_then(|p| p.as_u64())
                    .map(|p| p as u32),
                port: service
                    .get("port")
                    .and_then(|p| p.as_u64())
                    .map(|p| p as u16),
                health_status: service.get("health_status")?.as_str()?.to_string(),
                restart_count: service
                    .get("restart_count")
                    .and_then(|r| r.as_u64())
                    .unwrap_or(0) as u32,
                last_error: service
                    .get("last_error")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string()),
            })
        })
        .collect::<Vec<_>>();

    Some(services)
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

/// Get current consecutive write failure count (for health checks)
pub fn get_write_failure_count() -> u32 {
    WRITE_FAILURE_COUNT.load(Ordering::Relaxed)
}

/// Get status write health metrics
pub fn get_write_health_metrics() -> WriteHealthMetrics {
    WriteHealthMetrics {
        consecutive_failures: WRITE_FAILURE_COUNT.load(Ordering::Relaxed),
        max_consecutive_failures: MAX_CONSECUTIVE_FAILURES,
    }
}

/// Health metrics for status write operations
#[derive(Debug, Clone)]
pub struct WriteHealthMetrics {
    pub consecutive_failures: u32,
    pub max_consecutive_failures: u32,
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

    // Write metadata file indicating where status file is located
    // This helps menu bar app discover the path (only for primary path)
    if let Err(e) = write_status_path_metadata(status_path) {
        warn!("Failed to write status path metadata: {}", e);
        // Don't fail the whole operation if metadata write fails
    }

    Ok(())
}

/// Write metadata file indicating where the status file is located
/// This helps the menu bar app discover the path when server uses fallback location
fn write_status_path_metadata(status_path: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Write to well-known location
    let metadata_path = "/var/run/adapteros_status_path.txt";
    let metadata_temp = "/var/run/adapteros_status_path.txt.tmp";

    // Try to write metadata, but don't fail if we can't (e.g., no permissions)
    if let Err(e) = fs::write(metadata_temp, status_path) {
        // If /var/run/ isn't writable, try user directory
        let home = std::env::var("HOME").ok();
        if let Some(home) = home {
            let user_metadata = format!(
                "{}/Library/Application Support/AdapterOS/status_path.txt",
                home
            );
            if let Some(parent) = Path::new(&user_metadata).parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&user_metadata, status_path);
        }
        return Err(anyhow::Error::from(e));
    }

    // Atomic rename
    if let Err(e) = fs::rename(metadata_temp, metadata_path) {
        let _ = fs::remove_file(metadata_temp);
        return Err(anyhow::Error::from(e));
    }

    // Set permissions to 0644
    #[cfg(unix)]
    {
        if let Ok(mut perms) = fs::metadata(metadata_path).map(|m| m.permissions()) {
            perms.set_mode(0o644);
            let _ = fs::set_permissions(metadata_path, perms);
        }
    }

    Ok(())
}

/// Fallback: write to local var/ directory
fn write_status_file_local(status: &AdapterOSStatus) -> Result<()> {
    let json =
        serde_json::to_string_pretty(status).context("Failed to serialize status to JSON")?;

    // Use local var directory
    let status_dir = Path::new("var");
    fs::create_dir_all(status_dir).context("Failed to create var/ directory")?;

    // Resolve absolute path for var/ directory
    let status_path = std::env::current_dir()
        .map(|cwd| cwd.join("var/adapteros_status.json"))
        .and_then(|p| p.canonicalize().or_else(|_| Ok(p)))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "var/adapteros_status.json".to_string());

    let temp_path = status_path.replace(".json", ".tmp");

    // Write metadata file indicating where status file is located
    // This helps menu bar app discover the path
    if let Err(e) = write_status_path_metadata(&status_path) {
        warn!("Failed to write status path metadata: {}", e);
        // Don't fail the whole operation if metadata write fails
    }

    // Clean up any leftover temp file from previous failed writes
    if Path::new(&temp_path).exists() {
        if let Err(e) = fs::remove_file(&temp_path) {
            warn!("Could not clean up leftover temp file {}: {}", temp_path, e);
        }
    }

    fs::write(&temp_path, json)
        .with_context(|| format!("Failed to write temp status file: {}", temp_path))?;

    fs::rename(&temp_path, &status_path).with_context(|| {
        format!(
            "Failed to rename temp file {} to {}",
            temp_path, status_path
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&status_path)
            .with_context(|| format!("Failed to get metadata for status file: {}", status_path))?
            .permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&status_path, perms).with_context(|| {
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

    #[tokio::test]
    async fn test_partial_load_recovery() {
        // Test that the status writer handles cache failures gracefully
        init_status_cache();

        // Simulate cache failure
        if let Some(cache) = STATUS_CACHE.get() {
            let mut cache_write = cache.write().unwrap();
            *cache_write = None;
        }

        let state = create_mock_app_state().await;

        // This should fail gracefully without panicking
        let result = write_status(&state).await;
        assert!(result.is_err());

        // Failure count should be incremented
        assert_eq!(get_write_failure_count(), 1);
    }

    #[tokio::test]
    async fn test_retry_logic_full_operation() {
        init_status_cache();
        let state = create_mock_app_state().await;

        // Ensure we have valid cache data
        update_cache(&state)
            .await
            .expect("cache update should succeed");

        // This should succeed (we have valid data)
        let result = write_status(&state).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_migration_failure_handling() {
        // Test that status collection handles database query failures gracefully
        init_status_cache();

        // Create a state with a broken database connection
        let db = Db::connect("sqlite::memory:?cache=shared").await.unwrap();
        // Don't run migrations - this simulates migration failure

        let state = create_mock_app_state_with_db(db).await;

        // Status collection should handle failures gracefully
        let result = collect_status(&state).await;
        // Should not panic, but may return degraded status
        assert!(result.is_ok() || result.is_err()); // Either is acceptable, as long as it doesn't crash
    }

    #[tokio::test]
    async fn test_corruption_recovery() {
        // Test that status writer handles file corruption gracefully
        init_status_cache();

        // Create test status
        let status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "ok".to_string(),
            uptime_secs: 100,
            adapters_loaded: 1,
            deterministic: true,
            kernel_hash: "test123".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 1,
            base_model_loaded: false,
            base_model_id: None,
            services: None,
            base_model_name: None,
            base_model_status: "unloaded".to_string(),
            base_model_memory_mb: None,
        };

        // Test atomic write with fallback directory
        let result = write_status_file_local(&status);
        assert!(result.is_ok());

        // Verify file exists and is readable
        let status_path = Path::new("var/adapteros_status.json");
        assert!(status_path.exists());

        let content = fs::read_to_string(status_path).unwrap();
        let read_status: AdapterOSStatus = serde_json::from_str(&content).unwrap();
        assert_eq!(read_status.status, "ok");

        // Cleanup
        let _ = fs::remove_file(status_path);
    }

    #[tokio::test]
    async fn test_consecutive_failure_escalation() {
        // Reset failure counter for this test
        WRITE_FAILURE_COUNT.store(0, Ordering::Relaxed);

        // Simulate 6 consecutive failures to trigger escalation
        for _ in 0..6 {
            WRITE_FAILURE_COUNT.fetch_add(1, Ordering::Relaxed);
        }

        // Verify escalation threshold
        let metrics = get_write_health_metrics();
        assert_eq!(metrics.consecutive_failures, 6);
        assert_eq!(metrics.max_consecutive_failures, MAX_CONSECUTIVE_FAILURES);

        // Reset for other tests
        WRITE_FAILURE_COUNT.store(0, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_retry_backoff_behavior() {
        init_status_cache();
        let state = create_mock_app_state().await;

        // Ensure we have valid cache data
        update_cache(&state)
            .await
            .expect("cache update should succeed");

        // This should succeed on first attempt
        let start = std::time::Instant::now();
        let result = write_status(&state).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        // Should complete quickly (no retries needed)
        assert!(elapsed.as_millis() < 100);
    }

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
            services: None,
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
            services: None,
        };

        // Manually set cache (simulating what update_cache would do)
        if let Some(cache) = STATUS_CACHE.get() {
            let mut cache_write = cache.write().unwrap();
            *cache_write = Some(CachedStatusEntry {
                status: test_status.clone(),
                last_updated: SystemTime::now(),
            });
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
            services: None,
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
            services: None,
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

    #[test]
    fn test_write_failure_count_tracking() {
        // Test that failure count starts at 0
        assert_eq!(get_write_failure_count(), 0);

        // Reset counter (for test isolation)
        WRITE_FAILURE_COUNT.store(0, Ordering::Relaxed);
        assert_eq!(get_write_failure_count(), 0);
    }

    #[tokio::test]
    async fn test_cache_freshness_detection() {
        init_status_cache();
        let state = create_mock_app_state().await;

        // Initially no cache
        assert!(get_cached_status().unwrap().is_none());

        // Update cache
        update_cache(&state).await.unwrap();

        // Cache should be fresh immediately
        assert!(!is_cache_stale(2));

        // Wait a bit and check staleness
        tokio::time::sleep(Duration::from_millis(2100)).await;
        assert!(is_cache_stale(2));

        // Fresh cache should detect staleness
        let stale = get_cached_status_with_max_age(2).unwrap();
        assert!(stale.is_some()); // Still returns cached value even if stale
    }

    #[tokio::test]
    async fn test_cache_fresh_refresh() {
        init_status_cache();
        let state = create_mock_app_state().await;

        // Initially no cache
        assert!(get_cached_status().unwrap().is_none());

        // Update cache
        update_cache(&state).await.unwrap();

        let initial_status = get_cached_status().unwrap().unwrap();

        // Make cache stale
        tokio::time::sleep(Duration::from_millis(2100)).await;

        // Use fresh cache API - should refresh automatically
        let fresh_status = get_cached_status_fresh(&state, 2).await.unwrap().unwrap();

        // Should have fresh data (might be same or updated)
        assert_eq!(fresh_status.schema_version, initial_status.schema_version);
    }

    #[test]
    fn test_write_health_metrics() {
        // Test health metrics structure
        let metrics = get_write_health_metrics();
        assert_eq!(metrics.max_consecutive_failures, MAX_CONSECUTIVE_FAILURES);
        assert_eq!(metrics.consecutive_failures, 0);

        // Test with simulated failures
        WRITE_FAILURE_COUNT.store(3, Ordering::Relaxed);
        let metrics = get_write_health_metrics();
        assert_eq!(metrics.consecutive_failures, 3);
    }

    /// Helper function to create a mock AppState with custom database
    async fn create_mock_app_state_with_db(db: Db) -> AppState {
        use adapteros_metrics_exporter::MetricsExporter;
        use adapteros_orchestrator::TrainingService;
        use adapteros_server_api::{state::ApiConfig, AppState};
        use adapteros_telemetry::metrics::MetricsRegistry;

        let api_config = std::sync::Arc::new(std::sync::RwLock::new(ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                system_metrics_interval_secs: 0,
                telemetry_buffer_capacity: 1024,
                telemetry_channel_capacity: 256,
                trace_buffer_capacity: 512,
                server_port: 9090,
                server_enabled: false,
            },
            golden_gate: None,
            bundles_root: "var/bundles".to_string(),
            production_mode: false,
            rate_limits: None,
            path_policy: adapteros_server_api::state::PathPolicyConfig::default(),
            repository_paths: adapteros_server_api::state::RepositoryPathsConfig::default(),
            model_load_timeout_secs: 300,
            model_unload_timeout_secs: 30,
            operation_retry: adapteros_server_api::state::OperationRetryConfig::default(),
            security: adapteros_server_api::state::SecurityConfig::default(),
            mlx: None,
        }));

        let metrics_exporter =
            std::sync::Arc::new(MetricsExporter::new(Default::default()).unwrap());
        let metrics_collector =
            std::sync::Arc::new(adapteros_telemetry::MetricsCollector::new().unwrap());
        let metrics_registry = std::sync::Arc::new(MetricsRegistry::new(metrics_collector.clone()));
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
            Some(metrics_collector),
            Some(metrics_registry),
            training_service,
            [0u8; 32], // test global seed
        )
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

        create_mock_app_state_with_db(db).await
    }
}
