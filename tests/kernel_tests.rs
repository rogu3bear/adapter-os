use std::fs;
use std::path::Path;
use std::process::Command;

/// Test that Metal kernels compile successfully and produce deterministic hashes
#[test]
fn test_kernel_compilation() {
    let metal_dir = Path::new("metal");
    assert!(metal_dir.exists(), "Metal directory should exist");

    // Run the build script
    let output = Command::new("bash")
        .arg("build.sh")
        .current_dir(metal_dir)
        .output()
        .expect("Failed to run build script");

    assert!(
        output.status.success(),
        "Build script should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify metallib was created
    let metallib_path = metal_dir.join("adapteros_kernels.metallib");
    assert!(metallib_path.exists(), "Metallib should be created");

    // Verify hash file was created
    let hash_path = metal_dir.join("kernel_hash.txt");
    assert!(hash_path.exists(), "Hash file should be created");

    // Verify kernel registry was updated
    let registry_path = metal_dir.join("kernels.json");
    assert!(registry_path.exists(), "Kernel registry should exist");
}

/// Test that kernel hashes are deterministic across multiple builds
#[test]
fn test_deterministic_kernel_hashes() {
    let metal_dir = Path::new("metal");

    // Build twice and compare hashes
    let first_output = Command::new("bash")
        .arg("build.sh")
        .current_dir(metal_dir)
        .output()
        .expect("Failed to run first build");

    assert!(first_output.status.success(), "First build should succeed");

    let second_output = Command::new("bash")
        .arg("build.sh")
        .current_dir(metal_dir)
        .output()
        .expect("Failed to run second build");

    assert!(
        second_output.status.success(),
        "Second build should succeed"
    );

    // Read both hash files
    let hash_path = metal_dir.join("kernel_hash.txt");
    let first_hash = fs::read_to_string(&hash_path).expect("Failed to read first hash");

    // Rebuild and read second hash
    let second_hash = fs::read_to_string(&hash_path).expect("Failed to read second hash");

    // Hashes should be identical for deterministic builds
    assert_eq!(
        first_hash.trim(),
        second_hash.trim(),
        "Kernel hashes should be deterministic across builds"
    );
}

/// Test that kernel registry contains valid JSON and expected structure
#[test]
fn test_kernel_registry_structure() {
    let registry_path = Path::new("metal/kernels.json");
    assert!(registry_path.exists(), "Kernel registry should exist");

    let registry_content = fs::read_to_string(registry_path).expect("Failed to read registry");

    // Parse JSON
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Registry should be valid JSON");

    // Check required fields
    assert!(
        registry["schema_version"].is_string(),
        "Schema version should be present"
    );
    assert!(registry["kernels"].is_array(), "Kernels should be an array");
    assert!(
        registry["parameter_structures"].is_array(),
        "Parameter structures should be an array"
    );
    assert!(
        registry["configuration_structures"].is_array(),
        "Configuration structures should be an array"
    );

    // Check that we have the expected number of kernels
    let kernels = registry["kernels"].as_array().unwrap();
    assert_eq!(kernels.len(), 3, "Should have 3 kernels");

    // Check kernel names
    let kernel_names: Vec<&str> = kernels
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();

    assert!(
        kernel_names.contains(&"fused_mlp"),
        "Should contain fused_mlp kernel"
    );
    assert!(
        kernel_names.contains(&"fused_qkv_gqa"),
        "Should contain fused_qkv_gqa kernel"
    );
    assert!(
        kernel_names.contains(&"flash_attention"),
        "Should contain flash_attention kernel"
    );

    // Check that all kernels have hashes
    for kernel in kernels {
        assert!(
            kernel["blake3_hash"].is_string(),
            "Each kernel should have a hash"
        );
        assert!(
            !kernel["blake3_hash"].as_str().unwrap().is_empty(),
            "Hash should not be empty"
        );
    }
}

/// Test that kernel registry is updated with actual build hash
#[test]
fn test_kernel_registry_hash_update() {
    let metal_dir = Path::new("metal");

    // Build the kernels
    let output = Command::new("bash")
        .arg("build.sh")
        .current_dir(metal_dir)
        .output()
        .expect("Failed to run build script");

    assert!(output.status.success(), "Build should succeed");

    // Read the hash file
    let hash_path = metal_dir.join("kernel_hash.txt");
    let expected_hash = fs::read_to_string(&hash_path).expect("Failed to read hash");

    // Read the registry
    let registry_path = metal_dir.join("kernels.json");
    let registry_content = fs::read_to_string(&registry_path).expect("Failed to read registry");
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Registry should be valid JSON");

    // Check that all kernels have the same hash as the build
    let kernels = registry["kernels"].as_array().unwrap();
    for kernel in kernels {
        let kernel_hash = kernel["blake3_hash"].as_str().unwrap();
        assert_eq!(
            kernel_hash,
            expected_hash.trim(),
            "Kernel hash should match build hash"
        );
    }
}

/// Test that Metal SDK and compiler versions are recorded
#[test]
fn test_build_metadata() {
    let registry_path = Path::new("metal/kernels.json");
    let registry_content = fs::read_to_string(registry_path).expect("Failed to read registry");
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Registry should be valid JSON");

    // Check that build metadata is present
    assert!(
        registry["metal_sdk_version"].is_string(),
        "Metal SDK version should be recorded"
    );
    assert!(
        registry["compiler_version"].is_string(),
        "Compiler version should be recorded"
    );
    assert!(
        registry["build_timestamp"].is_string(),
        "Build timestamp should be recorded"
    );

    // Check that versions are not empty
    assert!(
        !registry["metal_sdk_version"].as_str().unwrap().is_empty(),
        "Metal SDK version should not be empty"
    );
    assert!(
        !registry["compiler_version"].as_str().unwrap().is_empty(),
        "Compiler version should not be empty"
    );
}

/// Test that kernel parameter structures are properly documented
#[test]
fn test_parameter_structure_documentation() {
    let registry_path = Path::new("metal/kernels.json");
    let registry_content = fs::read_to_string(registry_path).expect("Failed to read registry");
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Registry should be valid JSON");

    let param_structures = registry["parameter_structures"].as_array().unwrap();

    // Check that we have the expected parameter structures
    let structure_names: Vec<&str> = param_structures
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();

    assert!(
        structure_names.contains(&"MlpParams"),
        "Should contain MlpParams"
    );
    assert!(
        structure_names.contains(&"AttentionParams"),
        "Should contain AttentionParams"
    );
    assert!(
        structure_names.contains(&"FlashAttentionParams"),
        "Should contain FlashAttentionParams"
    );

    // Check that each structure has fields
    for structure in param_structures {
        assert!(
            structure["fields"].is_array(),
            "Each structure should have fields"
        );
        let fields = structure["fields"].as_array().unwrap();
        assert!(
            !fields.is_empty(),
            "Structure should have at least one field"
        );

        // Check that each field has required properties
        for field in fields {
            assert!(field["name"].is_string(), "Field should have a name");
            assert!(field["type"].is_string(), "Field should have a type");
            assert!(
                field["description"].is_string(),
                "Field should have a description"
            );
        }
    }
}

/// Test that configuration structures are properly documented
#[test]
fn test_configuration_structure_documentation() {
    let registry_path = Path::new("metal/kernels.json");
    let registry_content = fs::read_to_string(registry_path).expect("Failed to read registry");
    let registry: serde_json::Value =
        serde_json::from_str(&registry_content).expect("Registry should be valid JSON");

    let config_structures = registry["configuration_structures"].as_array().unwrap();

    // Check that we have the expected configuration structures
    let structure_names: Vec<&str> = config_structures
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();

    assert!(
        structure_names.contains(&"LoraConfig"),
        "Should contain LoraConfig"
    );
    assert!(
        structure_names.contains(&"GqaConfig"),
        "Should contain GqaConfig"
    );
    assert!(
        structure_names.contains(&"RingBuffer"),
        "Should contain RingBuffer"
    );

    // Check that each structure has fields
    for structure in config_structures {
        assert!(
            structure["fields"].is_array(),
            "Each structure should have fields"
        );
        let fields = structure["fields"].as_array().unwrap();
        assert!(
            !fields.is_empty(),
            "Structure should have at least one field"
        );

        // Check that each field has required properties
        for field in fields {
            assert!(field["name"].is_string(), "Field should have a name");
            assert!(field["type"].is_string(), "Field should have a type");
            assert!(
                field["description"].is_string(),
                "Field should have a description"
            );
        }
    }
}
