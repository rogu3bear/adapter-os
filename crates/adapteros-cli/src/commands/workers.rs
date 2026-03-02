//! Worker monitoring CLI commands
//!
//! Provides:
//! - `aosctl worker list`         — list all workers
//! - `aosctl worker health`       — show worker health summary
//! - `aosctl worker drain <id>`   — initiate graceful drain
//! - `aosctl worker restart <id>` — initiate worker restart

use crate::output::OutputWriter;
use adapteros_api_types::WorkerResponse;
use adapteros_core::Result;
use clap::Subcommand;
use tracing::info;

/// Worker subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum WorkerCommand {
    /// List all workers
    #[command(
        after_help = "Examples:\n  aosctl worker list\n  aosctl worker list --include-inactive\n  aosctl worker list --json"
    )]
    List {
        /// Include stopped/error/crashed workers
        #[arg(long)]
        include_inactive: bool,

        /// Filter by tenant ID
        #[arg(long)]
        tenant_id: Option<String>,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Show worker health summary
    #[command(after_help = "Examples:\n  aosctl worker health\n  aosctl worker health --json")]
    Health {
        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Initiate graceful drain on a worker
    #[command(after_help = "Examples:\n  aosctl worker drain wrk-abc123")]
    Drain {
        /// Worker ID to drain
        worker_id: String,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Restart a worker (drain then respawn)
    #[command(after_help = "Examples:\n  aosctl worker restart wrk-abc123")]
    Restart {
        /// Worker ID to restart
        worker_id: String,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },
}

/// Handle worker commands
pub async fn handle_worker_command(cmd: WorkerCommand, output: &OutputWriter) -> Result<()> {
    let command_name = match &cmd {
        WorkerCommand::List { .. } => "worker-list",
        WorkerCommand::Health { .. } => "worker-health",
        WorkerCommand::Drain { .. } => "worker-drain",
        WorkerCommand::Restart { .. } => "worker-restart",
    };

    info!(command = %command_name, "Handling worker command");

    if let Err(e) = crate::cli_telemetry::emit_cli_command(command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        WorkerCommand::List {
            include_inactive,
            tenant_id,
            base_url,
        } => worker_list(include_inactive, tenant_id, &base_url, output).await,
        WorkerCommand::Health { base_url } => worker_health(&base_url, output).await,
        WorkerCommand::Drain {
            worker_id,
            base_url,
        } => worker_drain(&worker_id, &base_url, output).await,
        WorkerCommand::Restart {
            worker_id,
            base_url,
        } => worker_restart(&worker_id, &base_url, output).await,
    }
}

async fn worker_list(
    include_inactive: bool,
    tenant_id: Option<String>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut url = format!("{}/v1/workers", base_url);

    let mut params = Vec::new();
    if include_inactive {
        params.push("include_inactive=true".to_string());
    }
    if let Some(ref tid) = tenant_id {
        params.push(format!("tenant_id={}", tid));
    }
    if !params.is_empty() {
        url = format!("{}?{}", url, params.join("&"));
    }

    let resp = client.get(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let workers: Vec<WorkerResponse> = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&workers)?;
            } else if workers.is_empty() {
                output.info("No workers found");
            } else {
                output.section(format!("Workers ({})", workers.len()));
                println!();

                for w in &workers {
                    let name = w
                        .display_name
                        .as_deref()
                        .unwrap_or_else(|| truncate_id(&w.id));
                    output.kv("Worker", name);
                    output.kv("  ID", &w.id);
                    output.kv("  Status", &w.status);
                    output.kv("  Tenant", &w.tenant_id);
                    if let Some(ref backend) = w.backend {
                        output.kv("  Backend", backend);
                    }
                    if let Some(ref model_id) = w.model_id {
                        output.kv("  Model", model_id);
                    }
                    output.kv("  Started", &w.started_at);
                    if let Some(ref last_seen) = w.last_seen_at {
                        output.kv("  Last seen", last_seen);
                    }
                    println!();
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
            output.info("Expected endpoint: GET /v1/workers");
        }
    }

    Ok(())
}

async fn worker_health(base_url: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/workers/health/summary", base_url);

    let resp = client.get(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let summary: serde_json::Value = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&summary)?;
            } else {
                output.section("Worker Health Summary");

                // Display aggregate counts
                if let Some(total) = summary.get("total_workers").and_then(|v| v.as_u64()) {
                    output.kv("Total workers", &total.to_string());
                }
                if let Some(healthy) = summary.get("healthy").and_then(|v| v.as_u64()) {
                    output.kv("Healthy", &healthy.to_string());
                }
                if let Some(degraded) = summary.get("degraded").and_then(|v| v.as_u64()) {
                    if degraded > 0 {
                        output.kv("Degraded", &degraded.to_string());
                    }
                }
                if let Some(crashed) = summary.get("crashed").and_then(|v| v.as_u64()) {
                    if crashed > 0 {
                        output.kv("Crashed", &crashed.to_string());
                    }
                }
                if let Some(unknown) = summary.get("unknown").and_then(|v| v.as_u64()) {
                    if unknown > 0 {
                        output.kv("Unknown", &unknown.to_string());
                    }
                }

                // Display per-worker details if available
                if let Some(workers) = summary.get("workers").and_then(|v| v.as_array()) {
                    if !workers.is_empty() {
                        println!();
                        output.section("Per-Worker Health");
                        for w in workers {
                            let wid = w
                                .get("worker_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let health = w
                                .get("health_status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let latency = w
                                .get("avg_latency_ms")
                                .and_then(|v| v.as_f64())
                                .map(|l| format!("{:.1}ms", l))
                                .unwrap_or_else(|| "-".to_string());
                            let incidents = w
                                .get("recent_incidents_24h")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);

                            output.kv("Worker", truncate_id(wid));
                            output.kv("  Health", health);
                            output.kv("  Avg latency", &latency);
                            if incidents > 0 {
                                output.kv("  Incidents (24h)", &incidents.to_string());
                            }
                            println!();
                        }
                    }
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
            output.info("Expected endpoint: GET /v1/workers/health/summary");
        }
    }

    Ok(())
}

/// Response from worker stop/drain/restart operations.
/// Defined locally to avoid depending on server-api crate from CLI.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct WorkerStopResponse {
    worker_id: String,
    success: bool,
    message: String,
    previous_status: String,
    stopped_at: String,
}

async fn worker_drain(worker_id: &str, base_url: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/workers/{}/drain", base_url, worker_id);

    let resp = client.post(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: WorkerStopResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&result)?;
            } else if result.success {
                output.success(format!("Drain initiated for worker {}", result.worker_id));
                output.kv("Previous status", &result.previous_status);
                output.kv("Initiated at", &result.stopped_at);
            } else {
                output.warning(&result.message);
            }
        }
        Ok(response) if response.status().as_u16() == 404 => {
            output.error(format!("Worker not found: {}", worker_id));
        }
        Ok(response) if response.status().as_u16() == 409 => {
            let body = response.text().await.unwrap_or_default();
            output.error(format!("Invalid state transition: {}", body));
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: POST /v1/workers/{id}/drain");
        }
    }

    Ok(())
}

async fn worker_restart(worker_id: &str, base_url: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/workers/{}/restart", base_url, worker_id);

    let resp = client.post(&url).send().await;

    match resp {
        Ok(response) if response.status().is_success() => {
            let result: WorkerStopResponse = response.json().await.map_err(|e| {
                adapteros_core::AosError::internal(format!("JSON parse error: {}", e))
            })?;

            if output.is_json() {
                output.print_json(&result)?;
            } else if result.success {
                output.success(format!("Restart initiated for worker {}", result.worker_id));
                output.kv("Previous status", &result.previous_status);
                output.kv("Initiated at", &result.stopped_at);
            } else {
                output.warning(&result.message);
            }
        }
        Ok(response) if response.status().as_u16() == 404 => {
            output.error(format!("Worker not found: {}", worker_id));
        }
        Ok(response) if response.status().as_u16() == 409 => {
            let body = response.text().await.unwrap_or_default();
            output.error(format!("Invalid state transition: {}", body));
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            output.error(format!("API error ({}): {}", status, body));
        }
        Err(e) => {
            output.warning(format!("API not available: {}", e));
            output.info("Expected endpoint: POST /v1/workers/{id}/restart");
        }
    }

    Ok(())
}

/// Truncate a worker ID for display (first 16 chars)
fn truncate_id(id: &str) -> &str {
    if id.len() > 24 {
        &id[..24]
    } else {
        id
    }
}
