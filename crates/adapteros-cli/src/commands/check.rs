//! Post-reboot startup checklist command for aosctl.
//!
//! Provides:
//! - `aosctl check startup` – post-reboot startup verification (requires running server)

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::Serialize;
use std::time::Duration;

/// Top-level `check` command.
#[derive(Debug, Args, Clone)]
pub struct CheckCommand {
    #[command(subcommand)]
    pub subcommand: CheckSubcommand,
}

/// Subcommands under `aosctl check`.
#[derive(Debug, Subcommand, Clone)]
pub enum CheckSubcommand {
    /// Post-reboot startup verification (requires running server)
    Startup {
        /// Server URL (defaults to AOS_SERVER_URL env var or http://localhost:8080)
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

        /// Timeout in seconds
        #[arg(long, default_value = "10")]
        timeout: u64,
    },
}

/// Individual check result
#[derive(Debug, Serialize)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    message: String,
}

/// Status of a check
#[derive(Debug, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Pass,
    Fail,
}

/// Dispatch the selected check subcommand.
pub async fn run(cmd: CheckCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        CheckSubcommand::Startup {
            server_url,
            timeout,
        } => startup_check(&server_url, timeout, output).await,
    }
}

async fn startup_check(server_url: &str, timeout: u64, output: &OutputWriter) -> Result<()> {
    output.info("Running post-reboot startup checklist...\n");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .build()
        .context("Failed to create HTTP client")?;

    let base = server_url.trim_end_matches('/');
    let mut results = Vec::new();

    // Check 1: Server health endpoint
    let health_result = match client.get(&format!("{}/healthz", base)).send().await {
        Ok(resp) if resp.status().is_success() => CheckResult {
            name: "Server reachable".to_string(),
            status: CheckStatus::Pass,
            message: format!("Connected to {}", server_url),
        },
        Ok(resp) => CheckResult {
            name: "Server reachable".to_string(),
            status: CheckStatus::Fail,
            message: format!("Server returned {}", resp.status()),
        },
        Err(e) => CheckResult {
            name: "Server reachable".to_string(),
            status: CheckStatus::Fail,
            message: format!("Connection failed: {}", e),
        },
    };
    let server_ok = health_result.status == CheckStatus::Pass;
    results.push(health_result);

    if !server_ok {
        // Cannot continue without server
        display_results(&results, output)?;
        output.error(
            "\nServer is not reachable. Start with: cargo run --release -p adapteros-server",
        );
        std::process::exit(1);
    }

    // Check 2: Full health check
    let health_all = match client.get(&format!("{}/healthz/all", base)).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let status = body["overall_status"].as_str().unwrap_or("unknown");
            if status == "healthy" {
                CheckResult {
                    name: "Component health".to_string(),
                    status: CheckStatus::Pass,
                    message: "All components healthy".to_string(),
                }
            } else {
                CheckResult {
                    name: "Component health".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Status: {}", status),
                }
            }
        }
        Ok(resp) => CheckResult {
            name: "Component health".to_string(),
            status: CheckStatus::Fail,
            message: format!("HTTP {}", resp.status()),
        },
        Err(e) => CheckResult {
            name: "Component health".to_string(),
            status: CheckStatus::Fail,
            message: format!("Error: {}", e),
        },
    };
    results.push(health_all);

    // Check 3: Meta endpoint (version/env sanity)
    let mut server_version: Option<String> = None;
    let meta_check = match client.get(&format!("{}/v1/meta", base)).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let version = body["version"].as_str().unwrap_or("unknown");
            server_version = Some(version.to_string());
            CheckResult {
                name: "Server metadata".to_string(),
                status: CheckStatus::Pass,
                message: format!("Version {}", version),
            }
        }
        Ok(resp) => CheckResult {
            name: "Server metadata".to_string(),
            status: CheckStatus::Fail,
            message: format!("HTTP {}", resp.status()),
        },
        Err(e) => CheckResult {
            name: "Server metadata".to_string(),
            status: CheckStatus::Fail,
            message: format!("Error: {}", e),
        },
    };
    results.push(meta_check);

    // Check 4: Version match (CLI vs Server)
    let cli_version = env!("CARGO_PKG_VERSION");
    let version_check = match &server_version {
        Some(sv) if sv == cli_version => CheckResult {
            name: "Version match".to_string(),
            status: CheckStatus::Pass,
            message: format!("CLI and server both v{}", cli_version),
        },
        Some(sv) => CheckResult {
            name: "Version match".to_string(),
            status: CheckStatus::Fail,
            message: format!("CLI v{} != Server v{}", cli_version, sv),
        },
        None => CheckResult {
            name: "Version match".to_string(),
            status: CheckStatus::Fail,
            message: "Could not determine server version".to_string(),
        },
    };
    results.push(version_check);

    // Check 5: Auth endpoint reachable (401 without token is OK - means auth is working)
    let auth_check = match client.get(&format!("{}/v1/auth/me", base)).send().await {
        Ok(resp) => {
            // 401 is expected without token, means auth is working
            if resp.status().as_u16() == 401 || resp.status().is_success() {
                CheckResult {
                    name: "Auth endpoints".to_string(),
                    status: CheckStatus::Pass,
                    message: "Auth system responding".to_string(),
                }
            } else {
                CheckResult {
                    name: "Auth endpoints".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Unexpected status: {}", resp.status()),
                }
            }
        }
        Err(e) => CheckResult {
            name: "Auth endpoints".to_string(),
            status: CheckStatus::Fail,
            message: format!("Error: {}", e),
        },
    };
    results.push(auth_check);

    // Check 6: Database health via /healthz/all (includes db component)
    // Parse the full health response to check database specifically
    let db_check = match client.get(&format!("{}/healthz/all", base)).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            // Look for database component in the components array
            let db_healthy = body["components"]
                .as_array()
                .and_then(|components| {
                    components.iter().find(|c| {
                        c["component"].as_str() == Some("database")
                            || c["component"].as_str() == Some("db")
                    })
                })
                .map(|db| db["status"].as_str() == Some("healthy"))
                .unwrap_or(true); // If no db component, assume healthy

            if db_healthy {
                CheckResult {
                    name: "Database".to_string(),
                    status: CheckStatus::Pass,
                    message: "Database accessible".to_string(),
                }
            } else {
                CheckResult {
                    name: "Database".to_string(),
                    status: CheckStatus::Fail,
                    message: "Database component unhealthy".to_string(),
                }
            }
        }
        Ok(resp) => CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Fail,
            message: format!("HTTP {}", resp.status()),
        },
        Err(e) => CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Fail,
            message: format!("Error: {}", e),
        },
    };
    results.push(db_check);

    // Check 7: Readiness endpoint (confirms server is ready to serve requests)
    let ready_check = match client.get(&format!("{}/readyz", base)).send().await {
        Ok(resp) if resp.status().is_success() => CheckResult {
            name: "Server readiness".to_string(),
            status: CheckStatus::Pass,
            message: "Server ready to serve requests".to_string(),
        },
        Ok(resp) => CheckResult {
            name: "Server readiness".to_string(),
            status: CheckStatus::Fail,
            message: format!("Not ready: HTTP {}", resp.status()),
        },
        Err(e) => CheckResult {
            name: "Server readiness".to_string(),
            status: CheckStatus::Fail,
            message: format!("Error: {}", e),
        },
    };
    results.push(ready_check);

    display_results(&results, output)?;

    let passed = results
        .iter()
        .filter(|r| r.status == CheckStatus::Pass)
        .count();
    let total = results.len();

    output.blank();
    if passed == total {
        output.success(&format!("All {} startup checks passed", total));
    } else {
        output.warning(&format!("{}/{} startup checks passed", passed, total));
        std::process::exit(1);
    }

    Ok(())
}

fn display_results(results: &[CheckResult], output: &OutputWriter) -> Result<()> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Check", "Status", "Details"]);

    for result in results {
        let (symbol, color) = match result.status {
            CheckStatus::Pass => ("PASS", Color::Green),
            CheckStatus::Fail => ("FAIL", Color::Red),
        };

        table.add_row(vec![
            Cell::new(&result.name),
            Cell::new(symbol).fg(color),
            Cell::new(&result.message),
        ]);
    }

    println!("{}", table);

    if output.is_json() {
        output.json(&results)?;
    }

    Ok(())
}
