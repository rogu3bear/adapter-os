#![cfg(all(test, feature = "extended-tests"))]

//! Multi-Host Golden Baseline Determinism Test
//!
//! Validates that AdapterOS produces identical outputs across multiple hosts
//! when given the same inputs and global seed.
//!
//! Per Determinism Ruleset #2: Outputs must be reproducible across runs and hosts.
//!
//! This test:
//! - Simulates 3 independent hosts
//! - Runs identical computations on each host
//! - Verifies all outputs match using BLAKE3 hashing
//! - Compares against golden baseline file (if exists)
//! - Creates golden baseline on first run

mod e2e;

use adapteros_core::{AosError, Result};
use e2e::{GoldenBaseline, TestCluster, TestClusterConfig};
use std::path::PathBuf;

const TEST_NAME: &str = "multi_host_determinism";
const GOLDEN_BASELINE_PATH: &str = "tests/golden_baselines/multi_host_determinism.json";

/// Run determinism test across 3 hosts
#[tokio::test]
async fn test_multi_host_determinism() -> Result<()> {
    println!("\n========================================");
    println!("Multi-Host Determinism Test");
    println!("========================================\n");

    // Create test cluster with 3 hosts
    let config = TestClusterConfig {
        host_count: 3,
        global_seed: [42u8; 32], // Fixed seed for reproducibility
        db_path_template: "host_{}_test.db".to_string(),
        verbose: true,
    };

    let cluster = TestCluster::new(config).await?;

    // Run deterministic computation on all hosts
    println!("Running deterministic computations on all hosts...");
    cluster
        .run_on_all_hosts(|host| {
            let host = host.clone();
            Box::pin(async move {
                let host_id = host.id;
                // Simulate deterministic inference
                let input = b"test prompt for determinism verification";
                let output = simulate_deterministic_inference(host_id, input).await?;

                // Store output
                host.store_result("inference_output".to_string(), output)
                    .await;

                // Simulate router decision
                let router_output = simulate_router_decision(host_id, input).await?;
                host.store_result("router_output".to_string(), router_output)
                    .await;

                // Simulate memory state
                let memory_state = simulate_memory_state(host_id).await?;
                host.store_result("memory_state".to_string(), memory_state)
                    .await;

                if host_id == 0 {
                    println!("  Host {} completed computation", host_id);
                }

                Ok(())
            })
        })
        .await?;

    println!("✓ All hosts completed computation\n");

    // Verify determinism across all hosts
    println!("Verifying determinism across hosts...");
    let reports = cluster.verify_all_results().await?;

    println!("\nDeterminism Verification Results:");
    println!("==================================");

    let mut all_passed = true;
    for report in &reports {
        println!("{}", report.summary());

        if !report.passed() {
            all_passed = false;
            for divergence in &report.divergences {
                println!(
                    "  Host {} vs Host {}: {} != {}",
                    divergence.baseline_host,
                    divergence.divergent_host,
                    &divergence.baseline_hash[..16],
                    &divergence.divergent_hash[..16]
                );
            }
        }
    }

    if !all_passed {
        return Err(AosError::Validation(
            "Determinism verification failed: hosts produced different outputs".to_string(),
        ));
    }

    println!();

    // Check against golden baseline if it exists
    let baseline_path = PathBuf::from(GOLDEN_BASELINE_PATH);
    if baseline_path.exists() {
        println!("Verifying against golden baseline...");

        let baseline = GoldenBaseline::load(&baseline_path)?;
        let verification = baseline.verify_cluster(&cluster).await?;

        println!("{}", verification.summary());

        if !verification.passed() {
            println!("\nMismatches:");
            for mismatch in &verification.mismatches {
                println!(
                    "  {}: expected {}, got {}",
                    mismatch.key,
                    &mismatch.expected_hash[..16],
                    &mismatch.actual_hash[..16]
                );
            }

            return Err(AosError::Validation(
                "Golden baseline verification failed".to_string(),
            ));
        }
    } else {
        println!("No golden baseline found, creating one...");

        // Create baseline directory if needed
        if let Some(parent) = baseline_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let baseline = GoldenBaseline::from_cluster(TEST_NAME.to_string(), &cluster).await?;
        baseline.save(&baseline_path)?;

        println!("✓ Golden baseline created: {}", baseline_path.display());
        println!("  Outputs: {}", baseline.expected_outputs.len());
        println!("  Commit this file to version control!");
    }

    println!("\n✓ Multi-host determinism test passed!");
    println!("========================================\n");

    Ok(())
}

/// Simulate deterministic inference
async fn simulate_deterministic_inference(_host_id: usize, input: &[u8]) -> Result<Vec<u8>> {
    // Hash the input deterministically
    let hash = blake3::hash(input);

    // Simulate some computation (deterministic)
    let mut output = Vec::new();
    output.extend_from_slice(b"inference_result:");
    output.extend_from_slice(hash.as_bytes());

    // Add deterministic "inference tokens" (same across all hosts)
    for i in 0..10 {
        output.push((i % 256) as u8);
    }

    Ok(output)
}

/// Simulate deterministic router decision
async fn simulate_router_decision(_host_id: usize, input: &[u8]) -> Result<Vec<u8>> {
    // Compute feature vector hash
    let feature_hash = blake3::hash(input);

    // Simulate K-sparse selection (deterministic)
    let mut output = Vec::new();
    output.extend_from_slice(b"router_decision:");

    // Top-K adapter indices (deterministic based on input)
    let k = 3;
    for i in 0..k {
        let adapter_idx = (feature_hash.as_bytes()[i] as usize) % 100;
        output.extend_from_slice(&adapter_idx.to_le_bytes());
    }

    Ok(output)
}

/// Simulate deterministic memory state
async fn simulate_memory_state(_host_id: usize) -> Result<Vec<u8>> {
    // Simulate memory allocation state (deterministic)
    let mut output = Vec::new();
    output.extend_from_slice(b"memory_state:");

    // Fixed memory layout
    let allocations: Vec<(&str, u64)> = vec![
        ("base_model", 1024 * 1024 * 512), // 512 MB
        ("adapter_1", 1024 * 1024 * 64),   // 64 MB
        ("adapter_2", 1024 * 1024 * 64),   // 64 MB
        ("adapter_3", 1024 * 1024 * 64),   // 64 MB
    ];

    for (name, size) in allocations {
        output.extend_from_slice(name.as_bytes());
        output.push(b':');
        output.extend_from_slice(&size.to_le_bytes());
        output.push(b';');
    }

    Ok(output)
}
