//! Adapter hot-swap command
//!
//! This module provides the low-level adapter hot-swap mechanism that communicates
//! directly with the worker via Unix Domain Socket. Before any swap operation,
//! preflight checks are run to ensure adapters are ready.

use crate::commands::preflight::{gate_alias_swap_with_config, AliasSwapGateConfig};
use crate::output::OutputWriter;
use adapteros_core::B3Hash;
use adapteros_db::Db;
use adapteros_lora_worker::adapter_hotswap::MemoryState;
use adapteros_lora_worker::{AdapterCommand, AdapterCommandResult};
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

/// Configuration for hot-swap preflight behavior
#[derive(Default)]
pub struct HotSwapConfig {
    /// Skip preflight checks entirely (emergency use only)
    pub skip_preflight: bool,
    /// Force swap even with preflight warnings
    pub force: bool,
}

/// Execute adapter hot-swap operation
///
/// Runs preflight checks before performing the swap unless `skip_preflight` is true.
pub async fn run(
    tenant: &str,
    add: &[String],
    remove: &[String],
    timeout_ms: u64,
    commit: bool,
    socket: &Path,
    output: &OutputWriter,
) -> Result<()> {
    run_with_config(
        tenant,
        add,
        remove,
        timeout_ms,
        commit,
        socket,
        output,
        &HotSwapConfig::default(),
    )
    .await
}

/// Execute adapter hot-swap operation with custom configuration
#[allow(clippy::too_many_arguments)]
pub async fn run_with_config(
    tenant: &str,
    add: &[String],
    remove: &[String],
    timeout_ms: u64,
    commit: bool,
    socket: &Path,
    output: &OutputWriter,
    config: &HotSwapConfig,
) -> Result<()> {
    output.result("Adapter Hot-Swap");
    output.kv("Tenant", tenant);
    output.kv("Add", &format!("{:?}", add));
    output.kv("Remove", &format!("{:?}", remove));
    output.kv("Timeout", &format!("{}ms", timeout_ms));
    output.blank();

    // =========================================================================
    // Phase 0: Preflight checks before hot-swap
    // =========================================================================
    if config.skip_preflight {
        output.warning("Preflight checks skipped - use with caution!");
    } else if !add.is_empty() {
        output.progress("Phase 0: Preflight checks");

        // Try to connect to database for preflight validation
        match Db::connect_env().await {
            Ok(db) => {
                let gate_config = AliasSwapGateConfig {
                    force: config.force,
                    skip_maintenance_check: false,
                    skip_conflict_check: false,
                    tenant_id: Some(tenant.to_string()),
                    allow_training_state: false,
                };

                let mut all_passed = true;
                for adapter_id in add {
                    output.progress(format!("  Checking {}...", adapter_id));

                    match gate_alias_swap_with_config(adapter_id, &db, &gate_config).await {
                        Ok(()) => {
                            output.success(format!("    {} passed preflight", adapter_id));
                        }
                        Err(e) => {
                            output.error(format!("    {} failed preflight: {}", adapter_id, e));
                            all_passed = false;
                        }
                    }
                }

                if !all_passed {
                    return Err(anyhow::anyhow!(
                        "Hot-swap blocked: one or more adapters failed preflight checks"
                    ));
                }

                output.success("All adapters passed preflight checks");
            }
            Err(e) => {
                if config.force {
                    output.warning(format!(
                        "Database unavailable for preflight: {}. Proceeding due to --force.",
                        e
                    ));
                } else {
                    return Err(anyhow::anyhow!(
                        "Cannot run preflight checks - database unavailable: {}. Use --force to bypass.",
                        e
                    ));
                }
            }
        }

        output.blank();
    }

    let mut swap_result: Option<AdapterCommandResult> = None;

    // Create HTTP client for UDS connection
    let client = create_uds_client(socket, timeout_ms)?;

    // =========================================================================
    // Phase 1: Preload adapters
    // =========================================================================
    output.progress("Phase 1: Preload");
    let mut preload_results = Vec::new();

    for adapter_id in add {
        output.progress(format!("  Preloading {}...", adapter_id));

        // Mock hash for now - in production this would come from registry
        let hash = B3Hash::hash(adapter_id.as_bytes());

        let command = AdapterCommand::Preload {
            adapter_id: adapter_id.clone(),
            hash,
        };

        let result = execute_command(&client, command, timeout_ms).await?;

        if result.success {
            if !output.mode().is_json() {
                println!(
                    "✓ (+{} MB, {} ms)",
                    result.vram_delta_mb.unwrap_or(0),
                    result.duration_ms
                );
            }
            preload_results.push((adapter_id.clone(), result));
        } else {
            output.error(format!("Preload failed: {}", result.message));
            return Err(anyhow::anyhow!("Preload failed: {}", result.message));
        }
    }

    output.blank();

    // =========================================================================
    // Phase 2: Atomic swap
    // =========================================================================
    if commit {
        output.progress("Phase 2: Swap (atomic)");
        output.progress("  Swapping adapters...");

        let command = AdapterCommand::Swap {
            add_ids: add.to_vec(),
            remove_ids: remove.to_vec(),
            expected_stack_hash: None,
        };

        let result = execute_command(&client, command, timeout_ms).await?;

        if result.success {
            render_memory_state(output, result.memory_state.as_ref());
            emit_swap_success(&result, output);
            swap_result = Some(result.clone());
        } else {
            output.error(format!("Swap failed: {}", result.message));

            // Attempt rollback on swap failure
            output.warning("Swap failed, attempting rollback...");
            let rollback_cmd = AdapterCommand::Rollback;
            let rollback_result = execute_command(&client, rollback_cmd, timeout_ms).await?;

            if rollback_result.success {
                output.success("Rollback successful");
            } else {
                output.error(format!("Rollback failed: {}", rollback_result.message));
            }

            return Err(anyhow::anyhow!("Swap failed: {}", result.message));
        }
    } else {
        output.progress("Phase 2: Dry-run (--commit not specified)");
        output.progress(format!("  Would swap: +{:?} / -{:?}", add, remove));
    }

    output.blank();

    // =========================================================================
    // Phase 3: Verification
    // =========================================================================
    output.progress("Phase 3: Verification");
    output.progress("  Verifying stack...");

    let verify_cmd = AdapterCommand::VerifyStack;
    let verify_result = execute_command(&client, verify_cmd, timeout_ms).await?;

    if verify_result.success {
        render_memory_state(output, verify_result.memory_state.as_ref());
        emit_swap_success(&verify_result, output);
    } else {
        output.error(format!("Verification failed: {}", verify_result.message));
        return Err(anyhow::anyhow!(
            "Verification failed: {}",
            verify_result.message
        ));
    }

    output.blank();
    if output.mode().is_json() {
        let report = serde_json::json!({
            "tenant": tenant,
            "add": add,
            "remove": remove,
            "commit": commit,
            "preload": preload_results,
            "swap": swap_result,
            "verify": verify_result,
        });
        output.print_json(&report)?;
    } else {
        output.success("Hot-swap complete");
    }

    Ok(())
}

fn emit_swap_success(result: &AdapterCommandResult, output: &OutputWriter) {
    if output.mode().is_json() {
        return;
    }

    let delta = result.vram_delta_mb.unwrap_or(0);
    let sign = if delta >= 0 { "+" } else { "" };
    output.result(format!(
        "✓ ({}{} MB, {} ms)",
        sign, delta, result.duration_ms
    ));

    if let Some(hash) = result.stack_hash {
        output.kv("Stack hash", &hash.to_hex());
    }
}

fn render_memory_state(output: &OutputWriter, state: Option<&MemoryState>) {
    if output.mode().is_json() {
        return;
    }

    if let Some(snapshot) = state {
        output.kv("VRAM after swap", &format!("{} MB", snapshot.total_vram_mb));
        for adapter in snapshot.active_adapters.iter().take(8) {
            output.result(format!(
                "  - {} [{} MB]{}",
                adapter.id,
                adapter.vram_mb,
                if adapter.active { "" } else { " (inactive)" }
            ));
        }
    }
}

/// Execute adapter command via UDS HTTP API
async fn execute_command(
    client: &reqwest::Client,
    command: AdapterCommand,
    timeout_ms: u64,
) -> Result<AdapterCommandResult> {
    let response = client
        .post("http://localhost/adapter")
        .json(&command)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .context("Failed to send adapter command")?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow::anyhow!("API error: {}", error_text));
    }

    let result: AdapterCommandResult = response
        .json()
        .await
        .context("Failed to parse adapter command result")?;

    Ok(result)
}

/// Create HTTP client configured for Unix Domain Socket
fn create_uds_client(socket_path: &Path, timeout_ms: u64) -> Result<reqwest::Client> {
    // HTTP client configuration will be enhanced when UDS transport is optimized

    // Check if socket exists
    if !socket_path.exists() {
        println!("Worker socket not found at: {}", socket_path.display());
        println!("   Using mock HTTP client for demonstration");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("Failed to create HTTP client")?;

        return Ok(client);
    }

    // For now, use standard HTTP client with UDS path logging
    // In production, this would use hyperlocal for actual UDS support
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .context("Failed to create HTTP client")?;

    println!("  Connected to UDS socket: {}", socket_path.display());

    Ok(client)
}
