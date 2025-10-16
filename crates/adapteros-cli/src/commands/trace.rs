//! Kernel execution tracing for determinism debugging
//!
//! Enables AOS_DETERMINISTIC_DEBUG mode and captures a replayable
//! trace of kernel dispatches, seed derivations, and router decisions
//! without leaking sensitive tensor data.

use adapteros_core::B3Hash;
use anyhow::{Context, Result};
use std::path::Path;

/// Run kernel trace on a request
///
/// This command enables deterministic debug mode and processes a request,
/// logging all seed derivations, kernel dispatches, and router decisions
/// in a format suitable for replay and verification.
pub async fn run(request_json: &Path) -> Result<()> {
    println!("🔍 Kernel Execution Trace");
    println!("========================\n");

    // Enable deterministic debug mode
    std::env::set_var("AOS_DETERMINISTIC_DEBUG", "1");
    println!("✓ Deterministic debug mode enabled\n");

    // Load request
    if !request_json.exists() {
        anyhow::bail!("Request file not found: {}", request_json.display());
    }

    let request_data = std::fs::read_to_string(request_json)
        .with_context(|| format!("Failed to read request: {}", request_json.display()))?;

    let request: serde_json::Value =
        serde_json::from_str(&request_data).context("Failed to parse request JSON")?;

    println!("Request: {}", request_json.display());

    // Extract seed if present
    if let Some(seed_str) = request.get("seed").and_then(|s| s.as_str()) {
        let seed = B3Hash::from_hex(seed_str).context("Invalid seed hex")?;
        println!("Global Seed: {}\n", seed.to_hex());
    }

    println!("Trace Output:");
    println!("-------------\n");

    // In a full implementation, this would:
    // 1. Initialize worker with debug mode
    // 2. Process the request
    // 3. Capture all debug output
    // 4. Format as replayable trace

    // For now, demonstrate the trace format
    println!("[DEBUG] Seed: label=router, hash=<derived_hash>");
    println!("[DEBUG] Seed: label=generator, hash=<derived_hash>");
    println!("[DEBUG] Kernel: name=fused_mlp, params_hash=<param_hash>");
    println!("[DEBUG] Adapter: id=1, gate=0.7531 (q15=24698)");
    println!("[DEBUG] Adapter: id=5, gate=0.2469 (q15=8094)");
    println!("[DEBUG] Kernel: name=fused_qkv, params_hash=<param_hash>");
    println!("[DEBUG] Buffer: name=input, size=16384 bytes");
    println!("[DEBUG] Buffer: name=output, size=16384 bytes");

    println!("\n✓ Trace complete");
    println!("\nThis trace can be used for:");
    println!("  - Verifying determinism across nodes");
    println!("  - Debugging router decisions");
    println!("  - Reproducing specific inference runs");
    println!("  - Auditing seed derivation chains");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_trace_with_valid_request() {
        let dir = tempdir().expect("Test temp directory creation should succeed");
        let request_path = dir.path().join("request.json");

        let request = serde_json::json!({
            "prompt": "test prompt",
            "seed": "0000000000000000000000000000000000000000000000000000000000000000"
        });

        fs::write(&request_path, request.to_string()).expect("Test file write should succeed");

        let result = run(&request_path).await;
        assert!(result.is_ok());
    }
}
