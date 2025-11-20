//! Integration tests for AOS loader with MLX backend
//!
//! Tests the complete workflow:
//! 1. Create .aos archive with test data
//! 2. Load weights via AosLoader
//! 3. Register with MLXFFIBackend
//! 4. Verify memory management

#[cfg(feature = "mmap")]
mod aos_integration {
    use adapteros_aos::aos2_writer::AOS2Writer;
    use adapteros_aos::aos_v2_parser::AosV2Manifest;
    use adapteros_core::{AosError, B3Hash, Result};
    use adapteros_lora_mlx_ffi::aos_loader::{AosLoader, MlxBackendAosExt};
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    /// Create a realistic .aos test file with proper safetensors format
    fn create_realistic_aos_file(adapter_id: &str, rank: u32) -> Result<(NamedTempFile, B3Hash)> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        // Create safetensors with realistic LoRA weights
        let weights_data = create_safetensors_weights(rank)?;
        let weights_hash = B3Hash::hash(&weights_data);

        // Create manifest
        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: adapter_id.to_string(),
            rank,
            weights_hash: Some(weights_hash.clone()),
            tensor_shapes: Some({
                let mut shapes = HashMap::new();
                shapes.insert("q_proj.lora_A".to_string(), vec![768, rank as usize]);
                shapes.insert("q_proj.lora_B".to_string(), vec![rank as usize, 768]);
                shapes.insert("v_proj.lora_A".to_string(), vec![768, rank as usize]);
                shapes.insert("v_proj.lora_B".to_string(), vec![rank as usize, 768]);
                shapes
            }),
            metadata: {
                let mut m = HashMap::new();
                m.insert("alpha".to_string(), serde_json::json!(16.0));
                m.insert(
                    "target_modules".to_string(),
                    serde_json::json!(["q_proj", "v_proj"]),
                );
                m.insert("dropout".to_string(), serde_json::json!(0.1));
                m
            },
        };

        // Write archive
        let writer = AOS2Writer::new();
        writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        Ok((temp_file, weights_hash))
    }

    /// Create safetensors format weights
    ///
    /// Format: [header_size:u64][header_json][tensor_data]
    fn create_safetensors_weights(rank: u32) -> Result<Vec<u8>> {
        let rank = rank as usize;
        let hidden_dim = 768;

        // Calculate tensor sizes
        let a_size = hidden_dim * rank * 4; // f32 = 4 bytes
        let b_size = rank * hidden_dim * 4;

        // Create header with tensor metadata
        let header_json = serde_json::json!({
            "q_proj.lora_A": {
                "dtype": "F32",
                "shape": [hidden_dim, rank],
                "data_offsets": [0, a_size]
            },
            "q_proj.lora_B": {
                "dtype": "F32",
                "shape": [rank, hidden_dim],
                "data_offsets": [a_size, a_size + b_size]
            },
            "v_proj.lora_A": {
                "dtype": "F32",
                "shape": [hidden_dim, rank],
                "data_offsets": [a_size + b_size, a_size + b_size + a_size]
            },
            "v_proj.lora_B": {
                "dtype": "F32",
                "shape": [rank, hidden_dim],
                "data_offsets": [a_size + b_size + a_size, a_size + b_size + a_size + b_size]
            }
        });

        let header_bytes =
            serde_json::to_vec(&header_json).map_err(|e| AosError::Serialization(e))?;
        let header_size = header_bytes.len() as u64;

        // Build weights data
        let mut weights_data = Vec::new();
        weights_data.extend_from_slice(&header_size.to_le_bytes());
        weights_data.extend_from_slice(&header_bytes);

        // Add tensor data (4 tensors total)
        // Initialize with small random values (simulating trained LoRA weights)
        for tensor_idx in 0..4 {
            let size = if tensor_idx % 2 == 0 { a_size } else { b_size };
            for i in 0..size / 4 {
                // Create deterministic "random" values
                let val = ((i + tensor_idx * 1000) % 100) as f32 / 1000.0;
                weights_data.extend_from_slice(&val.to_le_bytes());
            }
        }

        Ok(weights_data)
    }

    #[test]
    fn test_aos_loader_basic_load() -> Result<()> {
        let (temp_file, _hash) = create_realistic_aos_file("test-adapter-001", 8)?;

        let loader = AosLoader::new();
        let adapter = loader.load_from_aos(temp_file.path())?;

        assert_eq!(adapter.id(), "test-adapter-001");
        assert_eq!(adapter.config().rank, 8);
        assert_eq!(adapter.config().alpha, 16.0);

        // Verify weights were loaded
        let param_count = adapter.parameter_count();
        assert!(param_count > 0, "Adapter should have parameters loaded");

        // Expected: 2 modules × lora_b (768 × 8) = 2 × 6144 = 12288
        // Note: shared_down is not included in parameter_count currently
        let expected_params = 2 * 768 * 8;
        assert_eq!(
            param_count, expected_params,
            "Parameter count mismatch: expected {}, got {}",
            expected_params, param_count
        );

        Ok(())
    }

    #[test]
    fn test_aos_loader_hash_verification() -> Result<()> {
        let (temp_file, expected_hash) = create_realistic_aos_file("test-adapter-002", 16)?;

        let loader = AosLoader::new();
        let adapter = loader.load_and_verify(temp_file.path(), &expected_hash)?;

        assert_eq!(adapter.hash, expected_hash);

        Ok(())
    }

    #[test]
    fn test_aos_loader_hash_mismatch() -> Result<()> {
        let (temp_file, _hash) = create_realistic_aos_file("test-adapter-003", 8)?;

        let loader = AosLoader::new();
        let wrong_hash = B3Hash::hash(b"wrong-hash");
        let result = loader.load_and_verify(temp_file.path(), &wrong_hash);

        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::Verification(_) => {} // Expected
            e => panic!("Expected Verification error, got: {:?}", e),
        }

        Ok(())
    }

    #[test]
    fn test_aos_loader_multiple_adapters() -> Result<()> {
        let (file1, _) = create_realistic_aos_file("adapter-001", 8)?;
        let (file2, _) = create_realistic_aos_file("adapter-002", 16)?;
        let (file3, _) = create_realistic_aos_file("adapter-003", 4)?;

        let loader = AosLoader::new();
        let adapter_paths = vec![
            (1u16, file1.path()),
            (2u16, file2.path()),
            (3u16, file3.path()),
        ];

        let adapters = loader.load_multiple(&adapter_paths)?;

        assert_eq!(adapters.len(), 3);
        assert_eq!(adapters.get(&1).unwrap().config().rank, 8);
        assert_eq!(adapters.get(&2).unwrap().config().rank, 16);
        assert_eq!(adapters.get(&3).unwrap().config().rank, 4);

        Ok(())
    }

    #[test]
    fn test_aos_loader_skip_hash_verification() -> Result<()> {
        let (temp_file, _hash) = create_realistic_aos_file("test-adapter-004", 8)?;

        // Create loader with hash verification disabled
        let loader = AosLoader::with_options(false, true);
        let adapter = loader.load_from_aos(temp_file.path())?;

        assert_eq!(adapter.id(), "test-adapter-004");

        Ok(())
    }

    #[test]
    fn test_aos_loader_different_ranks() -> Result<()> {
        let ranks = [4, 8, 16, 32, 64];

        for rank in ranks {
            let (temp_file, _) =
                create_realistic_aos_file(&format!("adapter-rank-{}", rank), rank)?;

            let loader = AosLoader::new();
            let adapter = loader.load_from_aos(temp_file.path())?;

            assert_eq!(adapter.config().rank, rank as usize);

            // Verify parameter count scales with rank
            // Expected: 2 modules × lora_b (768 × rank)
            let expected_params = 2 * 768 * rank;
            assert_eq!(adapter.parameter_count(), expected_params as usize);
        }

        Ok(())
    }

    #[test]
    fn test_aos_loader_memory_usage() -> Result<()> {
        let (temp_file, _) = create_realistic_aos_file("test-adapter-memory", 8)?;

        let loader = AosLoader::new();
        let adapter = loader.load_from_aos(temp_file.path())?;

        let memory_bytes = adapter.memory_usage();
        let param_count = adapter.parameter_count();

        // Each parameter is f32 (4 bytes)
        assert_eq!(memory_bytes, param_count * 4);

        Ok(())
    }

    #[test]
    #[ignore] // Requires actual MLX model - run manually
    fn test_mlx_backend_integration() -> Result<()> {
        // This test would require a real MLX model to be present
        // For now, it's ignored but shows the intended usage pattern

        use adapteros_lora_mlx_ffi::{backend::MLXFFIBackend, MLXFFIModel};

        let model_path = "/path/to/mlx/model";
        let model = MLXFFIModel::load(model_path)?;
        let backend = MLXFFIBackend::new(model);

        let (temp_file, _) = create_realistic_aos_file("test-adapter-mlx", 8)?;

        // Load adapter from .aos and register with backend
        backend.load_adapter_from_aos(1, temp_file.path())?;

        assert_eq!(backend.adapter_count(), 1);

        Ok(())
    }

    #[test]
    fn test_aos_loader_tensor_name_parsing() -> Result<()> {
        // Test tensor name parsing through actual files
        // The loader will parse different naming conventions during load

        let (temp_file, _) = create_realistic_aos_file("test-adapter-names", 8)?;
        let loader = AosLoader::new();

        // Load adapter - this will test parsing internally
        let adapter = loader.load_from_aos(temp_file.path())?;

        // Verify adapter loaded successfully
        assert_eq!(adapter.id(), "test-adapter-names");
        assert!(adapter.parameter_count() > 0);

        Ok(())
    }

    #[test]
    fn test_aos_loader_shape_validation_strict() -> Result<()> {
        // Test shape validation through actual file loading
        // Invalid shapes will be caught during load

        let (temp_file, _) = create_realistic_aos_file("test-adapter-shapes", 8)?;
        let loader = AosLoader::with_options(true, true); // Strict validation enabled

        // This should succeed as the file has correct shapes
        let adapter = loader.load_from_aos(temp_file.path())?;
        assert_eq!(adapter.config().rank, 8);

        Ok(())
    }

    #[test]
    fn test_aos_loader_missing_file() {
        let loader = AosLoader::new();
        let result = loader.load_from_aos("/nonexistent/path/to/adapter.aos");

        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::NotFound(_) => {} // Expected
            e => panic!("Expected NotFound error, got: {:?}", e),
        }
    }
}

#[cfg(not(feature = "mmap"))]
mod aos_disabled {
    #[test]
    fn test_aos_loader_requires_mmap_feature() {
        // This test ensures the module compiles even without mmap feature
        // The actual AOS loader would not be available
        assert!(
            true,
            "AOS loader requires 'mmap' feature - compile with --features mmap"
        );
    }
}
