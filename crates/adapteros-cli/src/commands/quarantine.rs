//! Quarantine management CLI commands
//!
//! Provides:
//! - `aosctl quarantine status`   — show current quarantine state
//! - `aosctl quarantine clear`    — clear quarantine violations
//! - `aosctl quarantine rollback` — rollback to last known good policy config

use crate::output::OutputWriter;
use adapteros_api_types::{
    ClearQuarantineRequest, ClearQuarantineResponse, QuarantineStatusResponse,
    RollbackQuarantineRequest, RollbackQuarantineResponse,
};
use adapteros_core::Result;
use clap::Subcommand;
use tracing::info;

/// Quarantine subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum QuarantineCommand {
    /// Show current quarantine status
    #[command(
        after_help = "Examples:\n  aosctl quarantine status\n  aosctl quarantine status --json"
    )]
    Status {
        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Clear quarantine violations
    #[command(
        after_help = "Examples:\n  aosctl quarantine clear\n  aosctl quarantine clear --pack-id Egress\n  aosctl quarantine clear --rollback"
    )]
    Clear {
        /// Clear violations for a specific policy pack only
        #[arg(long)]
        pack_id: Option<String>,

        /// Reload baseline cache from database before clearing (rollback mode)
        #[arg(long)]
        rollback: bool,

        /// Operator identity (defaults to CLI user)
        #[arg(long)]
        operator: Option<String>,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Rollback to last known good policy configuration
    #[command(
        after_help = "Examples:\n  aosctl quarantine rollback\n  aosctl quarantine rollback --operator admin@example.com"
    )]
    Rollback {
        /// Operator identity (defaults to CLI user)
        #[arg(long)]
        operator: Option<String>,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },
}

/// Handle quarantine commands
pub async fn handle_quarantine_command(
    cmd: QuarantineCommand,
    output: &OutputWriter,
) -> Result<()> {
    let command_name = match &cmd {
        QuarantineCommand::Status { .. } => "quarantine-status",
        QuarantineCommand::Clear { .. } => "quarantine-clear",
        QuarantineCommand::Rollback { .. } => "quarantine-rollback",
    };

    info!(command = %command_name, "Handling quarantine command");

    if let Err(e) = crate::cli_telemetry::emit_cli_command(command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        QuarantineCommand::Status { base_url } => quarantine_status(&base_url, output).await,
        QuarantineCommand::Clear {
            pack_id,
            rollback,
            operator,
            base_url,
        } => quarantine_clear(pack_id, rollback, operator, &base_url, output).await,
        QuarantineCommand::Rollback { operator, base_url } => {
            quarantine_rollback(operator, &base_url, output).await
        }
    }
}

async fn quarantine_status(base_url: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/policy/quarantine/status", base_url);

    let resp = client.get(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let status: QuarantineStatusResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&status)?;
            } else {
                output.section("Quarantine Status");
                output.kv("Quarantined", if status.quarantined { "YES" } else { "no" });
                output.kv("Violations", &status.violation_count.to_string());
                if let Some(ref summary) = status.violation_summary {
                    output.kv("Summary", summary);
                }
                output.blank();
                if status.quarantined {
                    output.warning(&status.message);
                } else {
                    output.success(&status.message);
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: GET /v1/policy/quarantine/status");
        }
    }

    Ok(())
}

async fn quarantine_clear(
    pack_id: Option<String>,
    rollback: bool,
    operator: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/policy/quarantine/clear", base_url);

    let request = ClearQuarantineRequest {
        pack_id: pack_id.clone(),
        cpid: None,
        rollback,
        operator,
    };

    let resp = client.post(&url).json(&request).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: ClearQuarantineResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&result)?;
            } else if result.success {
                output.success(&result.message);
                if !result.cleared_packs.is_empty() {
                    output.kv("Cleared packs", &result.cleared_packs.join(", "));
                }
                output.kv("Violations cleared", &result.violations_cleared.to_string());
                if result.cache_reloaded {
                    output.info("Baseline cache reloaded from database");
                }
            } else {
                output.warning(&result.message);
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: POST /v1/policy/quarantine/clear");
        }
    }

    Ok(())
}

async fn quarantine_rollback(
    operator: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/policy/quarantine/rollback", base_url);

    let request = RollbackQuarantineRequest {
        cpid: None,
        operator,
    };

    let resp = client.post(&url).json(&request).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: RollbackQuarantineResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&result)?;
            } else if result.success {
                output.success(&result.message);
                output.kv("Violations cleared", &result.violations_cleared.to_string());
                if result.still_quarantined {
                    output.warning("System is still quarantined after rollback");
                } else {
                    output.info("System is no longer quarantined");
                }
            } else {
                output.warning("Rollback did not succeed");
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: POST /v1/policy/quarantine/rollback");
        }
    }

    Ok(())
}
