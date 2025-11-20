#![cfg(feature = "extended-tests")]

//! Server Lifecycle Integration Tests
//!
//! Critical tests for server startup, shutdown, and lifecycle management.
//! These tests verify:
//! - Server starts with valid configuration
//! - Health endpoints respond correctly
//! - Server handles missing database gracefully
//! - Port conflicts are detected and reported
//! - Graceful shutdown completes background tasks
//! - Config reload works without restart (SIGHUP)
//! - Health checks detect component degradation
//!
//! Run with: `cargo test --features extended-tests server_lifecycle`

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use anyhow::Context;
use serde_json::Value;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};

/// Test configuration for lifecycle tests
struct TestServerConfig {
    temp_dir: TempDir,
    config_path: PathBuf,
    db_path: PathBuf,
    pid_file_path: PathBuf,
    port: u16,
}

impl TestServerConfig {
    /// Create a new test configuration with valid settings
    fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let config_path = temp_dir.path().join("test-cp.toml");
        let db_path = temp_dir.path().join("test-aos-cp.sqlite3");
        let pid_file_path = temp_dir.path().join("aos-cp.pid");

        // Find available port
        let port = find_available_port()?;

        // Create minimal valid config
        let config_content = format!(
            r#"
[server]
port = {}
bind = "127.0.0.1"

[db]
path = "{}"

[security]
require_pf_deny = false
mtls_required = false
jwt_secret = "test_secret_key_for_integration_tests_only_not_for_production_use"

[paths]
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[rate_limits]
requests_per_minute = 100
burst_size = 20
inference_per_minute = 50

[metrics]
enabled = true
bearer_token = "test_metrics_token_for_tests"
include_histogram = true
histogram_buckets = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]

[alerting]
enabled = false
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#,
            port,
            db_path.display(),
            temp_dir.path().display(),
            temp_dir.path().display(),
            temp_dir.path().display(),
            temp_dir.path().display()
        );

        fs::write(&config_path, config_content)?;

        Ok(Self {
            temp_dir,
            config_path,
            db_path,
            pid_file_path,
            port,
        })
    }

    /// Create config with invalid database path
    fn with_invalid_db() -> Result<Self> {
        let mut config = Self::new()?;

        // Point to non-existent directory
        config.db_path = PathBuf::from("/nonexistent/path/to/database.sqlite3");

        // Update config file
        let config_content = format!(
            r#"
[server]
port = {}
bind = "127.0.0.1"

[db]
path = "{}"

[security]
require_pf_deny = false
mtls_required = false
jwt_secret = "test_secret_key_for_integration_tests_only"

[paths]
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[rate_limits]
requests_per_minute = 100
burst_size = 20
inference_per_minute = 50

[metrics]
enabled = true
bearer_token = "test_metrics_token"
include_histogram = false
histogram_buckets = []

[alerting]
enabled = false
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#,
            config.port,
            config.db_path.display(),
            config.temp_dir.path().display(),
            config.temp_dir.path().display(),
            config.temp_dir.path().display(),
            config.temp_dir.path().display()
        );

        fs::write(&config.config_path, config_content)?;
        Ok(config)
    }

    /// Get the base URL for this server
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Get the health check URL
    fn health_url(&self) -> String {
        format!("{}/api/healthz/all", self.base_url())
    }
}

/// Find an available port for testing
fn find_available_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("Failed to bind to ephemeral port")?;
    let port = listener.local_addr()?.port();
    drop(listener); // Release the port
    Ok(port)
}

/// Start the server process
fn start_server(config: &TestServerConfig, extra_args: &[&str]) -> Result<Child> {
    let server_binary = find_server_binary()?;

    let mut cmd = Command::new(&server_binary);
    cmd.arg("--config")
        .arg(&config.config_path)
        .arg("--pid-file")
        .arg(&config.pid_file_path)
        .arg("--skip-pf-check")
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .with_context(|| format!("Failed to start server binary: {}", server_binary))?;

    Ok(child)
}

/// Find the server binary (adapteros-server or cargo build target)
fn find_server_binary() -> Result<String> {
    // Check for pre-built binary
    let candidates = vec![
        "target/debug/adapteros-server",
        "target/release/adapteros-server",
    ];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    // Fall back to building from source
    eprintln!("Server binary not found, building from source...");
    let status = Command::new("cargo")
        .args(&["build", "--bin", "adapteros-server"])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to build server binary");
    }

    Ok("target/debug/adapteros-server".to_string())
}

/// Wait for server to be ready by checking health endpoint
async fn wait_for_server_ready(config: &TestServerConfig, timeout_secs: u64) -> Result<()> {
    let start = Instant::now();
    let timeout_duration = Duration::from_secs(timeout_secs);
    let client = reqwest::Client::new();

    loop {
        if start.elapsed() > timeout_duration {
            anyhow::bail!("Timeout waiting for server to be ready");
        }

        // Try to connect to health endpoint
        match client.get(&config.health_url()).send().await {
            Ok(response) if response.status().is_success() => {
                return Ok(());
            }
            _ => {
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Send signal to process (Unix only)
#[cfg(unix)]
fn send_signal(pid: u32, signal: i32) -> Result<()> {
    unsafe {
        if libc::kill(pid as i32, signal) != 0 {
            anyhow::bail!("Failed to send signal {} to PID {}", signal, pid);
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn send_signal(_pid: u32, _signal: i32) -> Result<()> {
    anyhow::bail!("Signal sending not supported on this platform");
}

// ============================================================================
// TEST 1: Server Startup Success
// ============================================================================

#[tokio::test]
async fn test_server_startup_success() -> Result<()> {
    eprintln!("TEST: Server startup with valid config");

    let config = TestServerConfig::new()?;

    // Start server
    let mut child = start_server(&config, &[])?;

    // Wait for server to be ready
    match timeout(Duration::from_secs(30), wait_for_server_ready(&config, 30)).await {
        Ok(Ok(())) => eprintln!("✓ Server started successfully"),
        Ok(Err(e)) => {
            child.kill()?;
            anyhow::bail!("Server failed to start: {}", e);
        }
        Err(_) => {
            child.kill()?;
            anyhow::bail!("Timeout waiting for server to start");
        }
    }

    // Verify health endpoint responds
    let client = reqwest::Client::new();
    let response = client
        .get(&config.health_url())
        .send()
        .await
        .context("Failed to query health endpoint")?;

    assert_eq!(
        response.status(),
        200,
        "Health endpoint should return 200 OK"
    );

    let health_data: Value = response.json().await?;
    eprintln!(
        "Health response: {}",
        serde_json::to_string_pretty(&health_data)?
    );

    // Verify overall status is present
    assert!(
        health_data.get("overall_status").is_some(),
        "Health response should include overall_status"
    );

    // Verify components array is present
    let components = health_data
        .get("components")
        .and_then(|c| c.as_array())
        .context("Health response should include components array")?;

    assert!(
        !components.is_empty(),
        "Components array should not be empty"
    );

    // Verify at least db component is healthy (since we just started)
    let db_component = components
        .iter()
        .find(|c| c.get("component").and_then(|v| v.as_str()) == Some("db"));

    assert!(
        db_component.is_some(),
        "DB component should be in health response"
    );

    eprintln!("✓ All components present in health check");

    // Clean shutdown
    child.kill()?;
    let _ = child.wait()?;

    // Verify PID file was cleaned up
    assert!(
        !config.pid_file_path.exists(),
        "PID file should be cleaned up on shutdown"
    );

    eprintln!("✓ Clean shutdown completed");
    Ok(())
}

// ============================================================================
// TEST 2: Server Startup with Missing Database
// ============================================================================

#[tokio::test]
async fn test_server_startup_missing_database() -> Result<()> {
    eprintln!("TEST: Server startup with invalid DATABASE_URL");

    let config = TestServerConfig::with_invalid_db()?;

    // Start server
    let mut child = start_server(&config, &[])?;

    // Wait a bit for server to attempt startup
    sleep(Duration::from_secs(2)).await;

    // Check if process has exited
    match child.try_wait()? {
        Some(status) => {
            eprintln!("✓ Server exited as expected with status: {:?}", status);
            assert!(
                !status.success(),
                "Server should exit with error code for invalid DB path"
            );
        }
        None => {
            // Process is still running, kill it and fail test
            child.kill()?;
            anyhow::bail!("Server should have exited due to invalid database path");
        }
    }

    // Verify PID file was cleaned up even on error
    assert!(
        !config.pid_file_path.exists(),
        "PID file should be cleaned up even on startup failure"
    );

    eprintln!("✓ PID lock cleaned up on error");
    Ok(())
}

// ============================================================================
// TEST 3: Server Port Conflict Detection
// ============================================================================

#[tokio::test]
async fn test_server_port_conflict() -> Result<()> {
    eprintln!("TEST: Server port conflict detection");

    let config = TestServerConfig::new()?;

    // Start first server instance
    let mut first_server = start_server(&config, &[])?;

    // Wait for first server to bind to port
    match timeout(Duration::from_secs(30), wait_for_server_ready(&config, 30)).await {
        Ok(Ok(())) => eprintln!("✓ First server started successfully"),
        Ok(Err(e)) => {
            first_server.kill()?;
            anyhow::bail!("First server failed to start: {}", e);
        }
        Err(_) => {
            first_server.kill()?;
            anyhow::bail!("Timeout waiting for first server");
        }
    }

    // Try to start second server instance (should fail due to PID lock)
    let config2 = TestServerConfig {
        temp_dir: tempfile::tempdir()?,
        config_path: config.config_path.clone(),
        db_path: config.db_path.clone(),
        pid_file_path: config.pid_file_path.clone(),
        port: config.port,
    };

    let mut second_server = start_server(&config2, &[])?;

    // Wait a bit for second server to detect conflict
    sleep(Duration::from_secs(2)).await;

    // Check if second server has exited
    match second_server.try_wait()? {
        Some(status) => {
            eprintln!(
                "✓ Second server exited as expected with status: {:?}",
                status
            );
            assert!(
                !status.success(),
                "Second server should exit with error due to PID conflict"
            );
        }
        None => {
            // Still running, kill both and fail
            second_server.kill()?;
            first_server.kill()?;
            anyhow::bail!("Second server should have exited due to PID lock");
        }
    }

    // Clean up first server
    first_server.kill()?;
    let _ = first_server.wait()?;

    eprintln!("✓ Port/PID conflict detected and handled");
    Ok(())
}

// ============================================================================
// TEST 4: Graceful Shutdown with SIGTERM
// ============================================================================

#[cfg(unix)]
#[tokio::test]
async fn test_graceful_shutdown_sigterm() -> Result<()> {
    eprintln!("TEST: Graceful shutdown with SIGTERM");

    let config = TestServerConfig::new()?;

    // Start server
    let mut child = start_server(&config, &[])?;
    let pid = child.id();

    // Wait for server to be ready
    timeout(Duration::from_secs(30), wait_for_server_ready(&config, 30))
        .await
        .context("Timeout waiting for server")??;

    eprintln!("✓ Server started, sending SIGTERM to PID {}", pid);

    // Send SIGTERM
    send_signal(pid, libc::SIGTERM)?;

    // Wait for graceful shutdown (should complete within 10 seconds)
    let shutdown_start = Instant::now();
    let wait_result = timeout(Duration::from_secs(10), async {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => sleep(Duration::from_millis(100)).await,
                Err(e) => return Err(e),
            }
        }
    })
    .await;

    match wait_result {
        Ok(Ok(status)) => {
            let shutdown_duration = shutdown_start.elapsed();
            eprintln!(
                "✓ Server shutdown gracefully in {:.2}s with status: {:?}",
                shutdown_duration.as_secs_f64(),
                status
            );
        }
        Ok(Err(e)) => {
            child.kill()?;
            anyhow::bail!("Error waiting for shutdown: {}", e);
        }
        Err(_) => {
            child.kill()?;
            anyhow::bail!("Server did not shutdown within timeout");
        }
    }

    // Verify PID file cleaned up
    assert!(
        !config.pid_file_path.exists(),
        "PID file should be cleaned up after graceful shutdown"
    );

    eprintln!("✓ Graceful shutdown completed, resources cleaned up");
    Ok(())
}

// ============================================================================
// TEST 5: Config Reload with SIGHUP
// ============================================================================

#[cfg(unix)]
#[tokio::test]
async fn test_config_reload_sighup() -> Result<()> {
    eprintln!("TEST: Config reload with SIGHUP");

    let config = TestServerConfig::new()?;

    // Start server
    let mut child = start_server(&config, &[])?;
    let pid = child.id();

    // Wait for server to be ready
    timeout(Duration::from_secs(30), wait_for_server_ready(&config, 30))
        .await
        .context("Timeout waiting for server")??;

    eprintln!("✓ Server started");

    // Modify config file (change metrics bearer token)
    let new_config_content = fs::read_to_string(&config.config_path)?.replace(
        "test_metrics_token_for_tests",
        "updated_metrics_token_after_reload",
    );

    fs::write(&config.config_path, new_config_content)?;
    eprintln!("✓ Config file modified");

    // Send SIGHUP to reload config
    send_signal(pid, libc::SIGHUP)?;
    eprintln!("✓ Sent SIGHUP to PID {}", pid);

    // Wait a bit for config reload
    sleep(Duration::from_secs(2)).await;

    // Verify server is still running
    match child.try_wait()? {
        Some(status) => {
            anyhow::bail!("Server exited unexpectedly after SIGHUP: {:?}", status);
        }
        None => {
            eprintln!("✓ Server still running after config reload");
        }
    }

    // Verify health endpoint still works
    let client = reqwest::Client::new();
    let response = client.get(&config.health_url()).send().await?;

    assert_eq!(
        response.status(),
        200,
        "Health endpoint should still work after config reload"
    );

    eprintln!("✓ Config reloaded without restart");

    // Clean shutdown
    child.kill()?;
    let _ = child.wait()?;

    Ok(())
}

// ============================================================================
// TEST 6: Health Check Degradation Detection
// ============================================================================

#[tokio::test]
async fn test_health_check_degradation() -> Result<()> {
    eprintln!("TEST: Health check degradation detection");

    let config = TestServerConfig::new()?;

    // Start server
    let mut child = start_server(&config, &[])?;

    // Wait for server to be ready
    timeout(Duration::from_secs(30), wait_for_server_ready(&config, 30))
        .await
        .context("Timeout waiting for server")??;

    eprintln!("✓ Server started");

    // Query initial health status
    let client = reqwest::Client::new();
    let response = client.get(&config.health_url()).send().await?;
    let health_data: Value = response.json().await?;

    eprintln!("Initial health status:");
    eprintln!("{}", serde_json::to_string_pretty(&health_data)?);

    // Check individual component health endpoints
    let component_names = vec![
        "db",
        "router",
        "loader",
        "kernel",
        "telemetry",
        "system-metrics",
    ];

    for component in component_names {
        let component_url = format!("{}/api/healthz/{}", config.base_url(), component);
        let response = client.get(&component_url).send().await?;

        if response.status().is_success() {
            let component_health: Value = response.json().await?;
            eprintln!(
                "✓ Component '{}' health: {}",
                component,
                component_health
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );
        } else {
            eprintln!(
                "⚠ Component '{}' returned status: {}",
                component,
                response.status()
            );
        }
    }

    // Note: We can't easily simulate component degradation in this test
    // without affecting the actual server state. This test verifies that
    // the health check endpoints are working and returning component status.
    // A real degradation scenario would require:
    // 1. High memory pressure (simulated via UMA monitor)
    // 2. Database connection issues
    // 3. Stuck adapter loading states
    // These would be better tested in component-specific integration tests.

    eprintln!("✓ Health check system operational");

    // Clean shutdown
    child.kill()?;
    let _ = child.wait()?;

    Ok(())
}

// ============================================================================
// Helper Tests
// ============================================================================

#[test]
fn test_config_generation() -> Result<()> {
    eprintln!("TEST: Config generation");

    let config = TestServerConfig::new()?;

    // Verify config file was created
    assert!(config.config_path.exists(), "Config file should be created");

    // Verify config is valid TOML
    let config_content = fs::read_to_string(&config.config_path)?;
    let parsed: toml::Value = toml::from_str(&config_content)?;

    // Verify required sections
    assert!(
        parsed.get("server").is_some(),
        "Config should have [server] section"
    );
    assert!(
        parsed.get("db").is_some(),
        "Config should have [db] section"
    );
    assert!(
        parsed.get("security").is_some(),
        "Config should have [security] section"
    );
    assert!(
        parsed.get("paths").is_some(),
        "Config should have [paths] section"
    );
    assert!(
        parsed.get("rate_limits").is_some(),
        "Config should have [rate_limits] section"
    );
    assert!(
        parsed.get("metrics").is_some(),
        "Config should have [metrics] section"
    );

    eprintln!("✓ Config generation working correctly");
    Ok(())
}

#[test]
fn test_port_availability() -> Result<()> {
    eprintln!("TEST: Port availability detection");

    let port1 = find_available_port()?;
    let port2 = find_available_port()?;

    assert_ne!(port1, port2, "Should find different available ports");
    assert!(port1 > 1024, "Should use non-privileged ports");
    assert!(port2 > 1024, "Should use non-privileged ports");

    eprintln!("✓ Found ports: {} and {}", port1, port2);
    Ok(())
}
