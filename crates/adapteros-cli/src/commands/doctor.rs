//! System health diagnostics command for aosctl
//!
//! Provides:
//! - `aosctl doctor` – comprehensive health check of all components

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// =============================================================================
// Boot Invariants Types
// =============================================================================

/// Invariant violation details from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantViolationDto {
    pub id: String,
    pub message: String,
    pub is_fatal: bool,
    pub remediation: String,
}

/// Boot invariants status response from /boot/invariants endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantStatusResponse {
    pub checked: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub fatal: u64,
    pub violations: Vec<InvariantViolationDto>,
    pub skipped_ids: Vec<String>,
    pub production_mode: bool,
}

/// Doctor command to check system health
#[derive(Debug, Args, Clone)]
pub struct DoctorCommand {
    /// Server URL (defaults to AOS_SERVER_URL env var or http://localhost:18080)
    #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:18080")]
    pub server_url: String,

    /// Timeout for health checks in seconds
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

/// Component health status from server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Individual component health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component: String,
    pub status: ComponentStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub timestamp: u64,
}

/// Aggregate health response for all components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthResponse {
    pub overall_status: ComponentStatus,
    pub components: Vec<ComponentHealth>,
    pub timestamp: u64,
}

// =============================================================================
// Backend Availability Checks
// =============================================================================

/// Get current timestamp for health check components
fn get_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Check MLX backend availability
fn check_mlx_backend() -> ComponentHealth {
    let timestamp = get_timestamp();

    // Check if multi-backend feature (which enables MLX) is compiled in
    let has_mlx_feature = cfg!(feature = "multi-backend");

    if has_mlx_feature {
        ComponentHealth {
            component: "MLX Backend".to_string(),
            status: ComponentStatus::Healthy,
            message: "MLX backend compiled (multi-backend feature enabled)".to_string(),
            details: Some(serde_json::json!({
                "feature": "multi-backend",
                "status": "compiled",
                "recommendation": "MLX backend is available for inference and training"
            })),
            timestamp,
        }
    } else {
        ComponentHealth {
            component: "MLX Backend".to_string(),
            status: ComponentStatus::Degraded,
            message: "MLX backend not compiled (use --features multi-backend)".to_string(),
            details: Some(serde_json::json!({
                "feature": "multi-backend",
                "status": "not_compiled",
                "recommendation": "Rebuild with --features multi-backend for MLX support"
            })),
            timestamp,
        }
    }
}

/// Check CoreML backend availability
///
/// On macOS with coreml-backend feature, this calls actual FFI functions to verify
/// runtime availability of CoreML, Neural Engine, and GPU capabilities.
#[allow(unexpected_cfgs)]
fn check_coreml_backend() -> ComponentHealth {
    let timestamp = get_timestamp();

    // On macOS with coreml-backend, call actual FFI to verify runtime availability
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    {
        use adapteros_lora_kernel_coreml::{
            capabilities, get_system_capabilities, has_neural_engine, is_coreml_available,
        };

        let coreml_available = is_coreml_available();
        let ane_available = has_neural_engine();
        let caps = get_system_capabilities();
        let gpu_available = caps & capabilities::GPU != 0;
        let mltensor_available = caps & capabilities::MLTENSOR_AVAILABLE != 0;
        let enhanced_api = caps & capabilities::ENHANCED_API != 0;

        let (status, message) = if coreml_available && ane_available && gpu_available {
            (
                ComponentStatus::Healthy,
                "CoreML: available, ANE: available, GPU: available".to_string(),
            )
        } else if coreml_available && ane_available {
            (
                ComponentStatus::Healthy,
                "CoreML: available, ANE: available, GPU: unavailable".to_string(),
            )
        } else if coreml_available {
            (
                ComponentStatus::Degraded,
                format!(
                    "CoreML: available, ANE: unavailable, GPU: {}",
                    if gpu_available {
                        "available"
                    } else {
                        "unavailable"
                    }
                ),
            )
        } else {
            (
                ComponentStatus::Unhealthy,
                "CoreML: unavailable (FFI check failed)".to_string(),
            )
        };

        return ComponentHealth {
            component: "CoreML Backend".to_string(),
            status,
            message,
            details: Some(serde_json::json!({
                "platform": "macos",
                "feature": "coreml-backend",
                "runtime_ffi_check": true,
                "coreml_available": coreml_available,
                "ane_available": ane_available,
                "gpu_available": gpu_available,
                "mltensor_available": mltensor_available,
                "enhanced_api": enhanced_api,
                "capabilities_bitmask": caps
            })),
            timestamp,
        };
    }

    // Fallback for non-macOS or when coreml-backend feature is not enabled
    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    {
        let is_macos = cfg!(target_os = "macos");
        let has_coreml_feature = cfg!(feature = "coreml-backend");
        if is_macos && has_coreml_feature {
            ComponentHealth {
                component: "CoreML Backend".to_string(),
                status: ComponentStatus::Healthy,
                message: "CoreML backend compiled (feature enabled)".to_string(),
                details: Some(serde_json::json!({
                    "platform": "macos",
                    "feature": "coreml-backend",
                    "runtime_ffi_check": false,
                    "recommendation": "CoreML backend is available for supported operations"
                })),
                timestamp,
            }
        } else if is_macos {
            ComponentHealth {
                component: "CoreML Backend".to_string(),
                status: ComponentStatus::Degraded,
                message: "macOS detected but coreml-backend feature not enabled".to_string(),
                details: Some(serde_json::json!({
                    "platform": "macos",
                    "feature": "coreml-backend",
                    "status": "not_compiled",
                    "recommendation": "Rebuild with --features coreml-backend for CoreML ANE support"
                })),
                timestamp,
            }
        } else {
            ComponentHealth {
                component: "CoreML Backend".to_string(),
                status: ComponentStatus::Degraded,
                message: "CoreML only available on macOS".to_string(),
                details: Some(serde_json::json!({
                    "platform": std::env::consts::OS,
                    "feature": "coreml-backend",
                    "status": "platform_unsupported",
                    "recommendation": "Run on macOS for CoreML ANE acceleration"
                })),
                timestamp,
            }
        }
    }
}

/// Check Metal backend availability
#[allow(unexpected_cfgs)]
fn check_metal_backend() -> ComponentHealth {
    let timestamp = get_timestamp();

    // Check platform and feature availability at compile time
    let is_macos = cfg!(target_os = "macos");
    let has_metal_feature = cfg!(feature = "metal-backend");

    if is_macos && has_metal_feature {
        ComponentHealth {
            component: "Metal Backend".to_string(),
            status: ComponentStatus::Healthy,
            message: "Metal backend available - GPU acceleration enabled".to_string(),
            details: Some(serde_json::json!({
                "platform": "macos",
                "feature": "metal-backend",
                "gpu_support": true,
                "recommendation": "Metal GPU kernels are available for compute operations"
            })),
            timestamp,
        }
    } else if is_macos {
        ComponentHealth {
            component: "Metal Backend".to_string(),
            status: ComponentStatus::Degraded,
            message: "macOS detected but metal-backend feature not enabled".to_string(),
            details: Some(serde_json::json!({
                "platform": "macos",
                "feature": "metal-backend",
                "status": "not_compiled",
                "recommendation": "Rebuild with --features metal-backend for Metal GPU kernels"
            })),
            timestamp,
        }
    } else {
        ComponentHealth {
            component: "Metal Backend".to_string(),
            status: ComponentStatus::Degraded,
            message: "Metal only available on macOS".to_string(),
            details: Some(serde_json::json!({
                "platform": std::env::consts::OS,
                "feature": "metal-backend",
                "status": "platform_unsupported",
                "recommendation": "Run on macOS for Metal GPU acceleration"
            })),
            timestamp,
        }
    }
}

/// Run the doctor command
pub async fn run(cmd: DoctorCommand, output: &OutputWriter) -> Result<()> {
    output.info("Running system health diagnostics...\n");

    // ==========================================================================
    // Backend Availability Checks (local platform capabilities)
    // ==========================================================================
    output.info("Checking backend availability...\n");

    // Check all backend availability
    let mlx_check = check_mlx_backend();
    let coreml_check = check_coreml_backend();
    let metal_check = check_metal_backend();

    // Build HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cmd.timeout))
        .build()
        .context("Failed to create HTTP client")?;

    // Call the /healthz/all endpoint
    let url = format!("{}/healthz/all", cmd.server_url.trim_end_matches('/'));

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to connect to server at {}", cmd.server_url))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Health check endpoint returned error: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    let mut health_response: SystemHealthResponse = response
        .json()
        .await
        .context("Failed to parse health response")?;

    // Add local backend availability checks to the components
    health_response.components.push(mlx_check.clone());
    health_response.components.push(coreml_check.clone());
    health_response.components.push(metal_check.clone());

    // Update overall status if any backend check is degraded
    let any_backend_degraded = mlx_check.status == ComponentStatus::Degraded
        || coreml_check.status == ComponentStatus::Degraded
        || metal_check.status == ComponentStatus::Degraded;

    if any_backend_degraded && health_response.overall_status == ComponentStatus::Healthy {
        health_response.overall_status = ComponentStatus::Degraded;
    }

    // Display results
    display_health_summary(&health_response, output)?;

    // ==========================================================================
    // Boot Invariants Check
    // ==========================================================================
    output.info("\nChecking boot invariants...\n");

    let invariants_result = fetch_boot_invariants(&client, &cmd.server_url).await;
    display_boot_invariants(invariants_result, output)?;

    // Exit with non-zero code if any component is unhealthy
    let has_unhealthy = health_response
        .components
        .iter()
        .any(|c| c.status == ComponentStatus::Unhealthy);

    if has_unhealthy {
        output.error("\nSystem health check FAILED");
        std::process::exit(1);
    } else if health_response.overall_status == ComponentStatus::Degraded {
        output.warning("\nSystem health is DEGRADED");
    } else {
        output.success("\nSystem health check PASSED");
    }

    Ok(())
}

/// Fetch boot invariants from the server
async fn fetch_boot_invariants(
    client: &reqwest::Client,
    server_url: &str,
) -> Result<InvariantStatusResponse, String> {
    let url = format!("{}/v1/invariants", server_url.trim_end_matches('/'));

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                response
                    .json::<InvariantStatusResponse>()
                    .await
                    .map_err(|e| format!("Failed to parse invariants response: {}", e))
            } else {
                Err(format!(
                    "Invariants endpoint returned {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ))
            }
        }
        Err(e) => Err(format!("Failed to connect to invariants endpoint: {}", e)),
    }
}

/// Display boot invariants in a tree format
fn display_boot_invariants(
    result: Result<InvariantStatusResponse, String>,
    output: &OutputWriter,
) -> Result<()> {
    println!("Boot Invariants");

    match result {
        Ok(invariants) => {
            // Determine overall status
            let (status_symbol, status_label) = if invariants.fatal > 0 {
                ("\u{2717}", "FAILED") // X mark
            } else if invariants.failed > 0 || invariants.skipped > 0 {
                ("\u{26A0}", "DEGRADED") // Warning sign
            } else {
                ("\u{2713}", "OK") // Check mark
            };

            // Display summary tree
            println!("\u{251C}\u{2500}\u{2500} Checked:  {}", invariants.checked);
            println!("\u{251C}\u{2500}\u{2500} Passed:   {}", invariants.passed);
            println!("\u{251C}\u{2500}\u{2500} Failed:   {}", invariants.failed);
            println!("\u{251C}\u{2500}\u{2500} Skipped:  {}", invariants.skipped);
            println!(
                "\u{2514}\u{2500}\u{2500} Status:   {} {}",
                status_symbol, status_label
            );

            // Display violations if any
            if !invariants.violations.is_empty() {
                for (i, violation) in invariants.violations.iter().enumerate() {
                    let is_last =
                        i == invariants.violations.len() - 1 && invariants.skipped_ids.is_empty();
                    let prefix = if is_last {
                        "    \u{2514}\u{2500}\u{2500}"
                    } else {
                        "    \u{251C}\u{2500}\u{2500}"
                    };

                    let fatal_marker = if violation.is_fatal { " [FATAL]" } else { "" };
                    println!(
                        "{} {}: {}{}",
                        prefix, violation.id, violation.message, fatal_marker
                    );
                }
            }

            // Display skipped invariants if any
            if !invariants.skipped_ids.is_empty() {
                for (i, id) in invariants.skipped_ids.iter().enumerate() {
                    let is_last = i == invariants.skipped_ids.len() - 1;
                    let prefix = if is_last {
                        "    \u{2514}\u{2500}\u{2500}"
                    } else {
                        "    \u{251C}\u{2500}\u{2500}"
                    };
                    println!("{} {}: skipped via config", prefix, id);
                }
            }

            // Log production mode info
            if invariants.production_mode {
                output.info("  (production mode: fatal violations block boot)");
            }
        }
        Err(err) => {
            // Server not available or endpoint not found
            println!("\u{251C}\u{2500}\u{2500} Checked:  -");
            println!("\u{251C}\u{2500}\u{2500} Passed:   -");
            println!("\u{251C}\u{2500}\u{2500} Failed:   -");
            println!("\u{251C}\u{2500}\u{2500} Skipped:  -");
            println!("\u{2514}\u{2500}\u{2500} Status:   ? UNAVAILABLE");
            println!("    \u{2514}\u{2500}\u{2500} {}", err);
            output.warning("  (server may not be running or endpoint not available)");
        }
    }

    Ok(())
}

/// Display health summary in a formatted table
fn display_health_summary(health: &SystemHealthResponse, output: &OutputWriter) -> Result<()> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Component", "Status", "Message"]);

    for component in &health.components {
        let (status_symbol, status_color) = match component.status {
            ComponentStatus::Healthy => ("✓", Color::Green),
            ComponentStatus::Degraded => ("⚠", Color::Yellow),
            ComponentStatus::Unhealthy => ("✗", Color::Red),
        };

        table.add_row(vec![
            Cell::new(&component.component),
            Cell::new(status_symbol).fg(status_color),
            Cell::new(&component.message),
        ]);
    }

    println!("{}", table);

    // Display overall status
    let (overall_symbol, overall_color) = match health.overall_status {
        ComponentStatus::Healthy => ("✓", Color::Green),
        ComponentStatus::Degraded => ("⚠", Color::Yellow),
        ComponentStatus::Unhealthy => ("✗", Color::Red),
    };

    println!();
    let mut overall_table = Table::new();
    overall_table.load_preset(UTF8_FULL);
    overall_table.add_row(vec![
        Cell::new("Overall System Health"),
        Cell::new(overall_symbol).fg(overall_color),
        Cell::new(format!("{:?}", health.overall_status)),
    ]);
    println!("{}", overall_table);

    // Display details if any component has them
    for component in &health.components {
        if let Some(details) = &component.details {
            output.info(format!("\n{} Details:", component.component));
            println!("{}", serde_json::to_string_pretty(details)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_status_deserialization() {
        let json =
            r#"{"component":"test","status":"healthy","message":"ok","timestamp":1234567890}"#;
        let health: ComponentHealth = serde_json::from_str(json).unwrap();
        assert_eq!(health.component, "test");
        assert_eq!(health.status, ComponentStatus::Healthy);
    }

    #[test]
    fn test_check_mlx_backend() {
        let health = check_mlx_backend();
        assert_eq!(health.component, "MLX Backend");
        // Status depends on whether multi-backend feature is enabled
        assert!(
            health.status == ComponentStatus::Healthy || health.status == ComponentStatus::Degraded
        );
        assert!(health.details.is_some());
    }

    #[test]
    fn test_check_coreml_backend() {
        let health = check_coreml_backend();
        assert_eq!(health.component, "CoreML Backend");
        // Status depends on platform and feature
        assert!(
            health.status == ComponentStatus::Healthy || health.status == ComponentStatus::Degraded
        );
        assert!(health.details.is_some());
    }

    #[test]
    fn test_check_metal_backend() {
        let health = check_metal_backend();
        assert_eq!(health.component, "Metal Backend");
        // Status depends on platform and feature
        assert!(
            health.status == ComponentStatus::Healthy || health.status == ComponentStatus::Degraded
        );
        assert!(health.details.is_some());
    }

    #[test]
    fn test_get_timestamp() {
        let ts1 = get_timestamp();
        let ts2 = get_timestamp();
        // Timestamps should be non-zero and monotonic
        assert!(ts1 > 0);
        assert!(ts2 >= ts1);
    }

    #[test]
    fn test_invariant_status_response_deserialization() {
        let json = r#"{
            "checked": 16,
            "passed": 14,
            "failed": 1,
            "skipped": 1,
            "fatal": 0,
            "violations": [
                {
                    "id": "SEC-005",
                    "message": "SameSite=None without Secure flag",
                    "is_fatal": false,
                    "remediation": "Set cookie_secure=true"
                }
            ],
            "skipped_ids": ["SEC-001"],
            "production_mode": false
        }"#;

        let response: InvariantStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.checked, 16);
        assert_eq!(response.passed, 14);
        assert_eq!(response.failed, 1);
        assert_eq!(response.skipped, 1);
        assert_eq!(response.fatal, 0);
        assert_eq!(response.violations.len(), 1);
        assert_eq!(response.violations[0].id, "SEC-005");
        assert!(!response.violations[0].is_fatal);
        assert_eq!(response.skipped_ids.len(), 1);
        assert_eq!(response.skipped_ids[0], "SEC-001");
        assert!(!response.production_mode);
    }

    #[test]
    fn test_invariant_violation_dto_deserialization() {
        let json = r#"{
            "id": "SEC-003",
            "message": "Deterministic executor initialized with default seed",
            "is_fatal": true,
            "remediation": "Provide valid manifest via --manifest-path"
        }"#;

        let violation: InvariantViolationDto = serde_json::from_str(json).unwrap();
        assert_eq!(violation.id, "SEC-003");
        assert!(violation.is_fatal);
        assert!(violation.message.contains("default seed"));
        assert!(violation.remediation.contains("manifest-path"));
    }
}
