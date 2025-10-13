//! Status writer for menu bar application
//!
//! Writes a JSON snapshot of AdapterOS state to `/var/run/adapteros_status.json`
//! for consumption by the macOS menu bar app.

use anyhow::{Context, Result};
use adapteros_server_api::AppState;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Status reported to menu bar app
#[derive(Debug, Serialize)]
pub struct AdapterOSStatus {
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
    // Query database for adapter and worker counts
    let adapters_loaded = query_adapter_count(&state.db).await.unwrap_or(0);
    let worker_count = query_worker_count(&state.db).await.unwrap_or(0);

    // Determine overall system status
    let status = if adapters_loaded > 0 && worker_count > 0 {
        "ok"
    } else if adapters_loaded > 0 || worker_count > 0 {
        "degraded"
    } else {
        "error"
    }
    .to_string();

    // Get kernel hash from plan (stub for now)
    let kernel_hash = get_kernel_hash()
        .await
        .unwrap_or_else(|| "00000000".to_string());

    // Check if deterministic mode is enabled
    let deterministic = check_deterministic_mode().await.unwrap_or(true);

    Ok(AdapterOSStatus {
        status,
        uptime_secs: get_uptime_secs(),
        adapters_loaded,
        deterministic,
        kernel_hash,
        telemetry_mode: "local".to_string(),
        worker_count,
    })
}

/// Query adapter count from database
async fn query_adapter_count(db: &adapteros_db::Db) -> Result<usize> {
    // Query from adapters table
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters")
        .fetch_one(db.pool())
        .await
        .context("Failed to query adapter count")?;
    Ok(count as usize)
}

/// Query worker count (from node agent or workers table)
async fn query_worker_count(_db: &adapteros_db::Db) -> Result<usize> {
    // For now, return a mock count
    // In production, would query from workers/sessions table
    Ok(0)
}

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
            status: "ok".to_string(),
            uptime_secs: 1337,
            adapters_loaded: 3,
            deterministic: true,
            kernel_hash: "a84d9f1c".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 2,
        };

        let json =
            serde_json::to_string(&status).expect("Test status serialization should succeed");
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"adapters_loaded\":3"));
    }
}
