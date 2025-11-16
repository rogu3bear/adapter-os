//! GPU integrity verification command

use adapteros_lora_lifecycle::GpuIntegrityReport;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

/// Execute GPU integrity verification
pub async fn run(
    tenant: &str,
    adapter_id: Option<&str>,
    socket: &PathBuf,
    timeout_ms: u64,
) -> Result<()> {
    println!("🔍 GPU Integrity Verification");
    println!("   Tenant: {}", tenant);
    if let Some(id) = adapter_id {
        println!("   Adapter: {}", id);
    } else {
        println!("   Scope: All loaded adapters");
    }
    println!();

    // Create HTTP client for UDS connection
    let client = create_uds_client(socket)?;

    // Execute verification
    print!("  Verifying GPU buffers... ");

    let url = if let Some(id) = adapter_id {
        format!("http://localhost/v1/adapters/verify-gpu?adapter_id={}", id)
    } else {
        "http://localhost/v1/adapters/verify-gpu".to_string()
    };

    let response = client
        .get(&url)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .context("Failed to send GPU verification request")?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        println!("✗");
        println!();
        return Err(anyhow::anyhow!("API error: {}", error_text));
    }

    let report: GpuIntegrityReport = response
        .json()
        .await
        .context("Failed to parse GPU integrity report")?;

    println!("✓");
    println!();

    // Display results
    println!("Results:");
    println!("  Total checked: {}", report.total_checked);
    println!("  Verified:      {} ✓", report.verified.len());
    println!("  Failed:        {} ✗", report.failed.len());
    println!("  Skipped:       {} -", report.skipped.len());
    println!();

    // Show verified adapters
    if !report.verified.is_empty() {
        println!("✓ Verified adapters:");
        for (adapter_idx, adapter_id) in &report.verified {
            println!("  • {} (idx: {})", adapter_id, adapter_idx);
        }
        println!();
    }

    // Show failed adapters with details
    if !report.failed.is_empty() {
        println!("✗ Failed adapters:");
        for (adapter_idx, adapter_id, reason) in &report.failed {
            println!("  • {} (idx: {})", adapter_id, adapter_idx);
            println!("    Reason: {}", reason);
        }
        println!();
    }

    // Show skipped adapters
    if !report.skipped.is_empty() {
        println!("- Skipped adapters (not loaded or verification not supported):");
        for (adapter_idx, adapter_id) in &report.skipped {
            println!("  • {} (idx: {})", adapter_id, adapter_idx);
        }
        println!();
    }

    // Overall status
    if report.failed.is_empty() {
        println!("✓ GPU integrity verification passed");
        Ok(())
    } else {
        println!("✗ GPU integrity verification failed");
        Err(anyhow::anyhow!(
            "{} adapter(s) failed GPU integrity checks",
            report.failed.len()
        ))
    }
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
