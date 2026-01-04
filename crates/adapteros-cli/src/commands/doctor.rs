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

/// Doctor command to check system health
#[derive(Debug, Args, Clone)]
pub struct DoctorCommand {
    /// Server URL (defaults to AOS_SERVER_URL env var or http://localhost:8080)
    #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
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
#[allow(unexpected_cfgs)]
fn check_coreml_backend() -> ComponentHealth {
    let timestamp = get_timestamp();

    // Check platform and feature availability at compile time
    let is_macos = cfg!(target_os = "macos");
    let has_coreml_feature = cfg!(feature = "coreml-backend");

    if is_macos && has_coreml_feature {
        ComponentHealth {
            component: "CoreML Backend".to_string(),
            status: ComponentStatus::Healthy,
            message: "CoreML backend available - ANE acceleration enabled".to_string(),
            details: Some(serde_json::json!({
                "platform": "macos",
                "feature": "coreml-backend",
                "ane_support": true,
                "recommendation": "CoreML ANE acceleration is available for supported operations"
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

    // Exit with non-zero code if any component is unhealthy
    let has_unhealthy = health_response
        .components
        .iter()
        .any(|c| c.status == ComponentStatus::Unhealthy);

    if has_unhealthy {
        output.error("\n❌ System health check FAILED");
        std::process::exit(1);
    } else if health_response.overall_status == ComponentStatus::Degraded {
        output.warning("\n⚠️  System health is DEGRADED");
    } else {
        output.success("\n✅ System health check PASSED");
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
}
