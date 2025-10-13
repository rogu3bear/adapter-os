//! Determinism Stress Tests
//!
//! These tests verify that AdapterOS produces identical outputs
//! across thousands of inferences, including:
//! - Process restarts
//! - Variable CPU/GPU load
//! - Different random seeds with same domain labels
//!
//! Run with: cargo test --test determinism_stress --ignored -- --test-threads=1

use mplora_core::B3Hash;
use mplora_worker::Worker;
use std::sync::Arc;

/// Setup a worker instance for testing
fn setup_worker() -> Worker {
    // Load test manifest
    let manifest_path = "manifests/qwen7b.yaml";
    let manifest = std::fs::read_to_string(manifest_path).expect("Failed to read manifest");

    let manifest: mplora_manifest::Manifest =
        serde_yaml::from_str(&manifest).expect("Failed to parse manifest");

    Worker::new(Arc::new(manifest)).expect("Failed to create worker")
}

/// Create a deterministic test request
fn create_test_request() -> mplora_core::InferenceRequest {
    mplora_core::InferenceRequest {
        prompt: "What is the capital of France?".to_string(),
        max_tokens: 50,
        temperature: 0.0, // Deterministic sampling
        seed: Some(42),   // Fixed seed
        tenant_id: "test-tenant".to_string(),
        request_id: "test-request-001".to_string(),
    }
}

#[test]
fn test_10k_inference_determinism() {
    println!("🔬 Running 10,000 inference determinism test...");
    println!("   This will take several minutes.");
    println!();

    let mut worker = setup_worker();
    let request = create_test_request();

    let mut outputs = Vec::new();
    let mut hashes = Vec::new();

    for i in 0..10_000 {
        if i % 1000 == 0 {
            println!("   Progress: {}/10,000", i);

            // Force "reboot" simulation every 1000 iterations
            if i > 0 {
                drop(worker);
                worker = setup_worker();
            }
        }

        let output = worker.infer(&request).expect("Inference failed");

        let hash = B3Hash::hash(output.text.as_bytes());

        outputs.push(output.text);
        hashes.push(hash);
    }

    println!("   Progress: 10,000/10,000");
    println!();

    // Verify all outputs are identical
    let first_output = &outputs[0];
    let first_hash = &hashes[0];

    for (i, (output, hash)) in outputs.iter().zip(hashes.iter()).enumerate().skip(1) {
        assert_eq!(
            output, first_output,
            "Output mismatch at iteration {}\n  First: {}\n  Current: {}",
            i, first_output, output
        );

        assert_eq!(
            hash,
            first_hash,
            "Hash mismatch at iteration {}\n  First: {}\n  Current: {}",
            i,
            first_hash.to_hex(),
            hash.to_hex()
        );
    }

    println!("✅ All 10,000 outputs identical!");
    println!("   Output: {}", first_output);
    println!("   BLAKE3: {}", first_hash.to_hex());
}

#[test]
fn test_100_inference_quick() {
    println!("🔬 Running 100 inference quick determinism test...");

    let mut worker = setup_worker();
    let request = create_test_request();

    let mut hashes = Vec::new();

    for i in 0..100 {
        if i % 25 == 0 {
            println!("   Progress: {}/100", i);
        }

        let output = worker.infer(&request).expect("Inference failed");

        let hash = B3Hash::hash(output.text.as_bytes());
        hashes.push(hash);
    }

    println!("   Progress: 100/100");

    // Verify all hashes are identical
    let first_hash = &hashes[0];
    for (i, hash) in hashes.iter().enumerate().skip(1) {
        assert_eq!(
            hash,
            first_hash,
            "Hash mismatch at iteration {}: {} != {}",
            i,
            hash.to_hex(),
            first_hash.to_hex()
        );
    }

    println!("✅ All 100 outputs identical!");
    println!("   BLAKE3: {}", first_hash.to_hex());
}

#[test]
fn test_determinism_under_load() {
    println!("🔬 Running determinism under load test...");
    println!("   Spawning background CPU load...");

    // Spawn CPU load in background
    let _load_handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                // Busy loop for CPU contention
                let mut sum: u64 = 0;
                for i in 0..100_000_000 {
                    sum = sum.wrapping_add(i);
                }
                sum
            })
        })
        .collect();

    println!("   Running inferences under load...");

    let mut worker = setup_worker();
    let request = create_test_request();

    let mut hashes = Vec::new();

    for i in 0..50 {
        if i % 10 == 0 {
            println!("   Progress: {}/50", i);
        }

        let output = worker.infer(&request).expect("Inference failed");

        let hash = B3Hash::hash(output.text.as_bytes());
        hashes.push(hash);
    }

    println!("   Progress: 50/50");

    // Verify all hashes are identical despite CPU contention
    let first_hash = &hashes[0];
    for (i, hash) in hashes.iter().enumerate().skip(1) {
        assert_eq!(
            hash,
            first_hash,
            "Hash mismatch under load at iteration {}: {} != {}",
            i,
            hash.to_hex(),
            first_hash.to_hex()
        );
    }

    println!("✅ All 50 outputs identical under load!");
    println!("   BLAKE3: {}", first_hash.to_hex());
}

#[test]
fn test_determinism_same_seed_different_runs() {
    println!("🔬 Testing seed determinism across runs...");

    let request = create_test_request();
    let mut run_outputs = Vec::new();

    for run in 0..10 {
        println!("   Run {}/10", run + 1);

        // Create fresh worker for each run
        let mut worker = setup_worker();

        let output = worker.infer(&request).expect("Inference failed");

        run_outputs.push(output.text);
    }

    // All runs should produce identical output
    let first_output = &run_outputs[0];
    for (i, output) in run_outputs.iter().enumerate().skip(1) {
        assert_eq!(
            output,
            first_output,
            "Output mismatch at run {}\n  First: {}\n  Current: {}",
            i + 1,
            first_output,
            output
        );
    }

    println!("✅ All 10 runs produced identical output!");
    println!("   Output: {}", first_output);
}
