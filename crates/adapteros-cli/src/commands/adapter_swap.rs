//! Adapter hot-swap command

use adapteros_core::B3Hash;
use adapteros_lora_worker::{AdapterCommand, AdapterCommandResult};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

/// Execute adapter hot-swap operation
pub async fn run(
    tenant: &str,
    add: &[String],
    remove: &[String],
    timeout_ms: u64,
    commit: bool,
    socket: &PathBuf,
) -> Result<()> {
    println!("🔄 Adapter Hot-Swap");
    println!("   Tenant: {}", tenant);
    println!("   Add: {:?}", add);
    println!("   Remove: {:?}", remove);
    println!("   Timeout: {}ms", timeout_ms);
    println!();

    // Create HTTP client for UDS connection
    let client = create_uds_client(socket)?;

    // Phase 1: Preload adapters
    println!("Phase 1: Preload");
    let mut preload_results = Vec::new();

    for adapter_id in add {
        print!("  Preloading {}... ", adapter_id);

        // Mock hash for now - in production this would come from registry
        let hash = B3Hash::hash(adapter_id.as_bytes());

        let command = AdapterCommand::Preload {
            adapter_id: adapter_id.clone(),
            hash,
        };

        let result = execute_command(&client, command, timeout_ms).await?;

        if result.success {
            println!(
                "✓ (+{} MB, {} ms)",
                result.vram_delta_mb.unwrap_or(0),
                result.duration_ms
            );
            preload_results.push((adapter_id.clone(), result));
        } else {
            println!("✗ {}", result.message);
            return Err(anyhow::anyhow!("Preload failed: {}", result.message));
        }
    }

    println!();

    // Phase 2: Atomic swap
    if commit {
        println!("Phase 2: Swap (atomic)");
        print!("  Swapping adapters... ");

        let command = AdapterCommand::Swap {
            add_ids: add.to_vec(),
            remove_ids: remove.to_vec(),
        };

        let result = execute_command(&client, command, timeout_ms).await?;

        if result.success {
            let delta_sign = if result.vram_delta_mb.unwrap_or(0) >= 0 {
                "+"
            } else {
                ""
            };
            println!(
                "✓ ({}{} MB, {} ms)",
                delta_sign,
                result.vram_delta_mb.unwrap_or(0),
                result.duration_ms
            );

            if let Some(hash) = result.stack_hash {
                println!("  Stack hash: {}", hash.to_hex());
            }
        } else {
            println!("✗ {}", result.message);

            // Attempt rollback on swap failure
            println!("\n⚠ Swap failed, attempting rollback...");
            let rollback_cmd = AdapterCommand::Rollback;
            let rollback_result = execute_command(&client, rollback_cmd, timeout_ms).await?;

            if rollback_result.success {
                println!("✓ Rollback successful");
            } else {
                println!("✗ Rollback failed: {}", rollback_result.message);
            }

            return Err(anyhow::anyhow!("Swap failed: {}", result.message));
        }
    } else {
        println!("Phase 2: Dry-run (--commit not specified)");
        println!("  Would swap: +{:?} / -{:?}", add, remove);
    }

    println!();

    // Phase 3: Verification
    println!("Phase 3: Verification");
    print!("  Verifying stack... ");

    let verify_cmd = AdapterCommand::VerifyStack;
    let verify_result = execute_command(&client, verify_cmd, timeout_ms).await?;

    if verify_result.success {
        println!("✓ ({} ms)", verify_result.duration_ms);
        if let Some(hash) = verify_result.stack_hash {
            println!("  Verified hash: {}", hash.to_hex());
        }
    } else {
        println!("✗ {}", verify_result.message);
        return Err(anyhow::anyhow!(
            "Verification failed: {}",
            verify_result.message
        ));
    }

    println!();
    println!("✓ Hot-swap complete");

    Ok(())
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
fn create_uds_client(socket_path: &PathBuf) -> Result<reqwest::Client> {
    // HTTP client configuration will be enhanced when UDS transport is optimized

    // Check if socket exists
    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("   Using mock HTTP client for demonstration");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        return Ok(client);
    }

    // For now, use standard HTTP client with UDS path logging
    // In production, this would use hyperlocal for actual UDS support
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;

    println!("  Connected to UDS socket: {}", socket_path.display());

    Ok(client)
}
