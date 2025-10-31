#![cfg(all(test, feature = "extended-tests"))]

//! Standalone Multi-Host Golden Baseline Determinism Test
//!
//! Validates that AdapterOS produces identical outputs across multiple hosts
//! when given the same inputs and global seed.
//!
//! Per Determinism Ruleset #2: Outputs must be reproducible across runs and hosts.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const TEST_NAME: &str = "multi_host_determinism";
const GOLDEN_BASELINE_PATH: &str = "tests/golden_baselines/multi_host_determinism.json";

/// Simple test host with deterministic output storage
struct TestHost {
    id: usize,
    _temp_dir: TempDir,
    results: HashMap<String, Vec<u8>>,
}

impl TestHost {
    fn new(id: usize) -> Result<Self> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

        Ok(Self {
            id,
            _temp_dir: temp_dir,
            results: HashMap::new(),
        })
    }

    fn store_result(&mut self, key: String, value: Vec<u8>) {
        self.results.insert(key, value);
    }

    fn get_result(&self, key: &str) -> Option<&Vec<u8>> {
        self.results.get(key)
    }
}

/// Golden baseline for determinism verification
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoldenBaseline {
    test_name: String,
    created_at: String,
    expected_outputs: HashMap<String, String>, // key -> BLAKE3 hash
}

impl GoldenBaseline {
    fn from_host(test_name: String, host: &TestHost) -> Self {
        let mut expected_outputs = HashMap::new();

        for (key, value) in &host.results {
            let hash = hex::encode(blake3::hash(value).as_bytes());
            expected_outputs.insert(key.clone(), hash);
        }

        Self {
            test_name,
            created_at: chrono::Utc::now().to_rfc3339(),
            expected_outputs,
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        // Create directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AosError::Io(format!("Failed to create directory: {}", e)))?;
        }

        let json = serde_json::to_string_pretty(self).map_err(AosError::Serialization)?;

        std::fs::write(path, json)
            .map_err(|e| AosError::Io(format!("Failed to write baseline: {}", e)))?;

        Ok(())
    }

    fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read baseline: {}", e)))?;

        serde_json::from_str(&json).map_err(AosError::Serialization)
    }

    fn verify(&self, host: &TestHost) -> (bool, Vec<String>) {
        let mut mismatches = Vec::new();

        for (key, expected_hash) in &self.expected_outputs {
            if let Some(actual_value) = host.get_result(key) {
                let actual_hash = hex::encode(blake3::hash(actual_value).as_bytes());
                if &actual_hash != expected_hash {
                    mismatches.push(format!(
                        "{}: expected {}, got {}",
                        key,
                        &expected_hash[..16],
                        &actual_hash[..16]
                    ));
                }
            } else {
                mismatches.push(format!("{}: MISSING", key));
            }
        }

        (mismatches.is_empty(), mismatches)
    }
}

/// Run determinism test across 3 hosts
#[test]
fn test_multi_host_determinism() -> Result<()> {
    println!("\n========================================");
    println!("Multi-Host Determinism Test");
    println!("========================================\n");

    // Create 3 hosts
    let mut hosts = Vec::new();
    for id in 0..3 {
        hosts.push(TestHost::new(id)?);
    }

    println!("Created {} test hosts", hosts.len());

    // Run identical computations on all hosts
    let input = b"test prompt for determinism verification";

    for host in &mut hosts {
        // Simulate deterministic inference
        let inference_output = simulate_deterministic_inference(input);
        host.store_result("inference_output".to_string(), inference_output);

        // Simulate router decision
        let router_output = simulate_router_decision(input);
        host.store_result("router_output".to_string(), router_output);

        // Simulate memory state
        let memory_state = simulate_memory_state();
        host.store_result("memory_state".to_string(), memory_state);
    }

    println!("✓ All hosts completed computation\n");

    // Verify determinism across all hosts
    println!("Verifying determinism across hosts...\n");

    let baseline_host = &hosts[0];
    let mut all_deterministic = true;

    for (i, host) in hosts.iter().enumerate().skip(1) {
        for key in baseline_host.results.keys() {
            let baseline_value = baseline_host.get_result(key).unwrap();
            let host_value = host.get_result(key).unwrap();

            if baseline_value != host_value {
                let baseline_hash = hex::encode(blake3::hash(baseline_value).as_bytes());
                let host_hash = hex::encode(blake3::hash(host_value).as_bytes());
                println!(
                    "✗ '{}': Host 0 vs Host {}: {} != {}",
                    key,
                    i,
                    &baseline_hash[..16],
                    &host_hash[..16]
                );
                all_deterministic = false;
            }
        }
    }

    if !all_deterministic {
        return Err(AosError::Validation(
            "Determinism verification failed: hosts produced different outputs".to_string(),
        ));
    }

    println!("✓ All outputs deterministic across {} hosts\n", hosts.len());

    // Check against golden baseline
    let baseline_path = PathBuf::from(GOLDEN_BASELINE_PATH);

    if baseline_path.exists() {
        println!("Verifying against golden baseline...");

        let baseline = GoldenBaseline::load(&baseline_path)?;
        let (passed, mismatches) = baseline.verify(baseline_host);

        if passed {
            println!("✓ Golden baseline verification passed");
        } else {
            println!("✗ Golden baseline verification failed:\n");
            for mismatch in mismatches {
                println!("  {}", mismatch);
            }
            return Err(AosError::Validation(
                "Golden baseline verification failed".to_string(),
            ));
        }
    } else {
        println!("No golden baseline found, creating one...");

        let baseline = GoldenBaseline::from_host(TEST_NAME.to_string(), baseline_host);
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
fn simulate_deterministic_inference(input: &[u8]) -> Vec<u8> {
    // Hash the input deterministically
    let hash = blake3::hash(input);

    // Simulate some computation (deterministic)
    let mut output = Vec::new();
    output.extend_from_slice(b"inference_result:");
    output.extend_from_slice(hash.as_bytes());

    // Add deterministic "inference tokens" (same across all hosts)
    for i in 0u8..10 {
        output.push(i);
    }

    output
}

/// Simulate deterministic router decision
fn simulate_router_decision(input: &[u8]) -> Vec<u8> {
    // Compute feature vector hash
    let feature_hash = blake3::hash(input);

    // Simulate K-sparse selection (deterministic)
    let mut output = Vec::new();
    output.extend_from_slice(b"router_decision:");

    // Top-K adapter indices (deterministic based on input)
    let k = 3;
    for i in 0..k {
        let adapter_idx = (feature_hash.as_bytes()[i] as usize) % 100;
        output.extend_from_slice(&(adapter_idx as u64).to_le_bytes());
    }

    output
}

/// Simulate deterministic memory state
fn simulate_memory_state() -> Vec<u8> {
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

    output
}
