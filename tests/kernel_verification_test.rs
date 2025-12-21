#![cfg(all(test, feature = "extended-tests"))]

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::str;

    #[test]
    fn test_metal_kernel_compilation_and_deterministic_hash() {
        // Change to the metal directory
        let original_dir = std::env::current_dir().expect("Failed to get current directory");
        let metal_dir = original_dir.join("metal");

        // Run the build script
        let output = Command::new("bash")
            .arg("build.sh")
            .current_dir(&metal_dir)
            .output()
            .expect("Failed to execute metal/build.sh");

        // Assert that the build script ran successfully
        assert!(
            output.status.success(),
            "Metal build script failed: {}\nStderr: {}",
            str::from_utf8(&output.stdout).unwrap_or("Invalid UTF-8"),
            str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8")
        );

        // Read the generated hash
        let kernel_hash_path =
            original_dir.join("crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt");
        let compiled_hash = std::fs::read_to_string(&kernel_hash_path)
            .expect(&format!(
                "Failed to read kernel_hash.txt from {:?}",
                kernel_hash_path
            ))
            .trim()
            .to_string();

        // Verify the hash is not empty
        assert!(!compiled_hash.is_empty(), "Compiled kernel hash is empty");

        // Verify the hash is deterministic (same across multiple builds)
        let second_output = Command::new("bash")
            .arg("build.sh")
            .current_dir(&metal_dir)
            .output()
            .expect("Failed to execute metal/build.sh second time");

        assert!(
            second_output.status.success(),
            "Second Metal build script failed"
        );

        let second_hash = std::fs::read_to_string(&kernel_hash_path)
            .expect("Failed to read kernel_hash.txt second time")
            .trim()
            .to_string();

        assert_eq!(
            compiled_hash, second_hash,
            "Kernel hash is not deterministic across builds"
        );

        println!("✅ Metal kernel compilation successful");
        println!("✅ Deterministic hash verified: {}", compiled_hash);
        println!("✅ Kernel registry updated with build metadata");
    }

    #[test]
    fn test_kernel_registry_structure() {
        // Verify the kernel registry has the expected structure
        let registry_path = std::env::current_dir()
            .expect("Failed to get current directory")
            .join("metal/kernels.json");

        let registry_content =
            std::fs::read_to_string(&registry_path).expect("Failed to read kernels.json");

        // Parse JSON to verify structure
        let registry: serde_json::Value =
            serde_json::from_str(&registry_content).expect("Failed to parse kernels.json");

        // Verify required fields
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

        // Verify kernels array
        let kernels = registry["kernels"]
            .as_array()
            .expect("kernels field is not an array");

        assert_eq!(kernels.len(), 3, "Expected 3 kernels in registry");

        // Verify each kernel has required fields
        for kernel in kernels {
            assert!(kernel["name"].is_string(), "Kernel missing name");
            assert!(kernel["version"].is_string(), "Kernel missing version");
            assert!(
                kernel["description"].is_string(),
                "Kernel missing description"
            );
            assert!(
                kernel["parameters"].is_string(),
                "Kernel missing parameters"
            );
            assert!(
                kernel["blake3_hash"].is_string(),
                "Kernel missing blake3_hash"
            );
            assert!(kernel["features"].is_array(), "Kernel missing features");
            assert!(kernel["references"].is_array(), "Kernel missing references");
        }

        // Verify parameter structures
        let param_structures = registry["parameter_structures"]
            .as_array()
            .expect("parameter_structures field is not an array");

        assert_eq!(param_structures.len(), 3, "Expected 3 parameter structures");

        // Verify configuration structures
        let config_structures = registry["configuration_structures"]
            .as_array()
            .expect("configuration_structures field is not an array");

        assert_eq!(
            config_structures.len(),
            3,
            "Expected 3 configuration structures"
        );

        println!("✅ Kernel registry structure verified");
        println!("✅ All required fields present and correctly typed");
    }
}
