#![cfg(all(test, feature = "extended-tests"))]

use std::process::Command;
use std::str;

/// Test Metal kernel compilation and hash generation
/// This test verifies that the Metal kernels compile successfully and produce deterministic hashes
#[test]
fn test_metal_kernel_compilation_and_hash() {
    // Run the build script
    let output = Command::new("bash")
        .arg("metal/build.sh")
        .output()
        .expect("Failed to execute metal/build.sh");

    // Assert that the build script ran successfully
    assert!(
        output.status.success(),
        "Metal build script failed: {}",
        str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8")
    );

    // Read the generated hash
    let kernel_hash_path = "crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt";
    let compiled_hash = std::fs::read_to_string(kernel_hash_path)
        .expect("Failed to read kernel_hash.txt")
        .trim()
        .to_string();

    // Verify the hash is not empty
    assert!(!compiled_hash.is_empty(), "Compiled kernel hash is empty");

    println!("Compiled kernel hash: {}", compiled_hash);

    // Verify the metallib file exists
    let metallib_path = "crates/adapteros-lora-kernel-mtl/shaders/adapteros_kernels.metallib";
    assert!(
        std::path::Path::new(metallib_path).exists(),
        "Metallib file does not exist"
    );

    // Verify the kernel registry was updated
    let registry_path = "metal/kernels.json";
    let registry_content =
        std::fs::read_to_string(registry_path).expect("Failed to read kernels.json");

    // Parse JSON to verify structure
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Failed to parse kernels.json");

    // Verify required fields exist
    assert!(
        registry["schema_version"].is_string(),
        "Missing schema_version"
    );
    assert!(
        registry["build_timestamp"].is_string(),
        "Missing build_timestamp"
    );
    assert!(
        registry["metal_sdk_version"].is_string(),
        "Missing metal_sdk_version"
    );
    assert!(
        registry["compiler_version"].is_string(),
        "Missing compiler_version"
    );
    assert!(registry["kernels"].is_array(), "Missing kernels array");

    // Verify kernels array has expected kernels
    let kernels = registry["kernels"].as_array().unwrap();
    assert_eq!(kernels.len(), 3, "Expected 3 kernels");

    let kernel_names: Vec<&str> = kernels
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();

    assert!(
        kernel_names.contains(&"fused_mlp"),
        "Missing fused_mlp kernel"
    );
    assert!(
        kernel_names.contains(&"fused_qkv_gqa"),
        "Missing fused_qkv_gqa kernel"
    );
    assert!(
        kernel_names.contains(&"flash_attention"),
        "Missing flash_attention kernel"
    );

    // Verify each kernel has the expected hash
    for kernel in kernels {
        let kernel_hash = kernel["blake3_hash"].as_str().unwrap();
        assert_eq!(
            kernel_hash,
            compiled_hash,
            "Kernel hash mismatch for {}",
            kernel["name"].as_str().unwrap()
        );
    }
}

/// Test deterministic hash generation across multiple builds
#[test]
fn test_deterministic_kernel_hashes() {
    // Run the build script twice
    let output1 = Command::new("bash")
        .arg("metal/build.sh")
        .output()
        .expect("Failed to execute metal/build.sh");

    assert!(output1.status.success(), "First build failed");

    let output2 = Command::new("bash")
        .arg("metal/build.sh")
        .output()
        .expect("Failed to execute metal/build.sh");

    assert!(output2.status.success(), "Second build failed");

    // Read hashes from both builds
    let hash1 = std::fs::read_to_string("crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt")
        .expect("Failed to read first hash")
        .trim()
        .to_string();

    let hash2 = std::fs::read_to_string("crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt")
        .expect("Failed to read second hash")
        .trim()
        .to_string();

    // Verify hashes are identical
    assert_eq!(
        hash1, hash2,
        "Kernel hashes are not deterministic: {} != {}",
        hash1, hash2
    );

    println!("Deterministic hash verified: {}", hash1);
}
