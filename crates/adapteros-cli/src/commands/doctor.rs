//! System health diagnostics command for aosctl (PRD-06)
//!
//! Provides:
//! - `aosctl doctor` – comprehensive health check of all components

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, CellAlignment, Color, Table};
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// Run the doctor command
pub async fn run(cmd: DoctorCommand, output: &OutputWriter) -> Result<()> {
    output.info("Running system health diagnostics...\n");

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

    let health_response: SystemHealthResponse = response
        .json()
        .await
        .context("Failed to parse health response")?;

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
fn display_health_summary(
    health: &SystemHealthResponse,
    output: &OutputWriter,
) -> Result<()> {
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
            output.info(&format!(
                "\n{} Details:",
                component.component
            ));
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
        let json = r#"{"component":"test","status":"healthy","message":"ok","timestamp":1234567890}"#;
        let health: ComponentHealth = serde_json::from_str(json).unwrap();
        assert_eq!(health.component, "test");
        assert_eq!(health.status, ComponentStatus::Healthy);
    }
}
