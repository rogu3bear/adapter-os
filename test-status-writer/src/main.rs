use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use tokio::time::{sleep, Duration};

/// Status reported to menu bar app
#[derive(Debug, Serialize)]
struct AdapterOSStatus {
    /// Schema version for forward/backward compatibility
    schema_version: String,
    /// System status: "ok" | "degraded" | "error"
    status: String,
    /// Uptime in seconds since control plane started
    uptime_secs: u64,
    /// Number of adapters currently loaded
    adapters_loaded: usize,
    /// Whether deterministic mode is enabled
    deterministic: bool,
    /// Short kernel hash (first 8 chars)
    kernel_hash: String,
    /// Telemetry mode: "local" | "disabled"
    telemetry_mode: String,
    /// Number of active workers
    worker_count: usize,
    /// Whether base model is loaded
    base_model_loaded: bool,
    /// Base model identifier (optional)
    base_model_id: Option<String>,
    /// Base model display name (optional)
    base_model_name: Option<String>,
    /// Base model status: "ready" | "loading" | "error"
    base_model_status: String,
    /// Base model memory usage in MB (optional)
    base_model_memory_mb: Option<usize>,
}

/// Tracks when the control plane started
static mut START_TIME: Option<SystemTime> = None;

fn init_start_time() {
    unsafe {
        START_TIME = Some(SystemTime::now());
    }
}

fn get_uptime_secs() -> u64 {
    unsafe {
        START_TIME
            .and_then(|start| SystemTime::now().duration_since(start).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

fn write_status_file(status: &AdapterOSStatus) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(status)?;

    // Ensure directory exists
    let status_dir = Path::new("/var/run");
    if !status_dir.exists() {
        // Try to create, but don't fail if we can't (might not have perms)
        if let Err(e) = fs::create_dir_all(status_dir) {
            eprintln!("Could not create /var/run: {}, trying local path", e);
            // Fall back to local directory
            return write_status_file_local(status);
        }
    }

    let status_path = "/var/run/adapteros_status.json";
    let temp_path = "/var/run/adapteros_status.json.tmp";

    // Write to temp file first
    fs::write(temp_path, json)?;

    // Atomic rename
    fs::rename(temp_path, status_path)?;

    // Set permissions to 0644 (readable by all, writable by owner)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(status_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(status_path, perms)?;
    }

    println!("Status written to {}", status_path);
    Ok(())
}

fn write_status_file_local(status: &AdapterOSStatus) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(status)?;

    // Use local var directory
    let status_dir = Path::new("var");
    fs::create_dir_all(status_dir)?;

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

    println!("Status written to {} (local fallback)", status_path);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting test status writer...");
    init_start_time();

    loop {
        let status = AdapterOSStatus {
            schema_version: "1.0".to_string(),
            status: "ok".to_string(),
            uptime_secs: get_uptime_secs(),
            adapters_loaded: 2,
            deterministic: true,
            kernel_hash: "a84d9f1c".to_string(),
            telemetry_mode: "local".to_string(),
            worker_count: 1,
            base_model_loaded: true,
            base_model_id: Some("qwen2.5-7b".to_string()),
            base_model_name: Some("Qwen 2.5 7B".to_string()),
            base_model_status: "ready".to_string(),
            base_model_memory_mb: Some(14336),
        };

        if let Err(e) = write_status_file(&status) {
            eprintln!("Failed to write status: {}", e);
        }

        sleep(Duration::from_secs(5)).await;
    }
}
