//! Error handling tests for MLX FFI backend
//!
//! Tests for proper error propagation and handling across the FFI boundary.

#[cfg(test)]
mod ffi_error_tests {
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_seed_empty_buffer_error() {
        let empty_seed: Vec<u8> = vec![];

        let result = mlx_set_seed_from_bytes(&empty_seed);

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{:?}", e);
            assert!(error_msg.contains("empty") || error_msg.contains("Seed"));
        }
    }

    #[test]
    fn test_seed_valid_buffer() {
        let seed = vec![0u8; 32]; // 32-byte seed

        let result = mlx_set_seed_from_bytes(&seed);

        // Should succeed with valid seed
        assert!(result.is_ok());
    }

    #[test]
    fn test_seed_short_buffer() {
        let seed = vec![1, 2, 3, 4]; // Short seed

        // Should still work (MLX accepts variable length)
        let result = mlx_set_seed_from_bytes(&seed);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod tensor_error_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_tensor_dtype_mismatch() {
        let int_data = vec![1, 2, 3, 4];
        let tensor = MLXFFITensor::from_ints(&int_data, vec![2, 2]).unwrap();

        // Try to access as float data
        let result = tensor.data();

        // Should fail due to type mismatch
        assert!(result.is_err());
    }

    #[test]
    fn test_tensor_shape_mismatch() {
        // Data size doesn't match shape
        let data = vec![1.0, 2.0, 3.0]; // 3 elements
        let shape = vec![2, 2]; // Expects 4 elements

        // Creation should succeed (shape is informational)
        let result = MLXFFITensor::from_data(&data, shape);
        // MLX may or may not enforce shape consistency
        let _ = result;
    }

    #[test]
    fn test_tensor_invalid_operation() {
        let data1 = vec![1.0, 2.0];
        let data2 = vec![3.0, 4.0, 5.0]; // Different size

        let tensor1 = MLXFFITensor::from_data(&data1, vec![2]).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, vec![3]).unwrap();

        // Adding tensors of different sizes may fail
        let result = tensor1.add(&tensor2);

        // Should either succeed with broadcasting or fail
        let _ = result;
    }
}

#[cfg(test)]
mod adapter_error_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    #[test]
    fn test_unload_nonexistent_adapter() {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);

        let backend = MLXFFIBackend::new(model);

        // Try to unload adapter that doesn't exist
        let result = backend.unload_adapter_runtime(999);

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{:?}", e);
            assert!(error_msg.contains("not found") || error_msg.contains("Lifecycle"));
        }
    }

    #[test]
    fn test_get_memory_nonexistent_adapter() {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);

        let backend = MLXFFIBackend::new(model);

        // Try to get memory for adapter that doesn't exist
        let result = backend.get_adapter_memory_usage(999);

        assert!(result.is_err());
    }

    #[test]
    fn test_adapter_duplicate_registration() {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);

        let backend = MLXFFIBackend::new(model);

        let adapter1 = create_mock_adapter("adapter1", 4);
        backend.register_adapter(1, adapter1).unwrap();

        // Register another adapter with same ID (should replace)
        let adapter2 = create_mock_adapter("adapter2", 8);
        backend.register_adapter(1, adapter2).unwrap();

        // Should still have 1 adapter
        assert_eq!(backend.adapter_count(), 1);
    }
}

#[cfg(test)]
mod lora_error_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_lora_mlx_ffi::routing::apply_multi_lora;

    #[test]
    fn test_lora_missing_module() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test".to_string(), config);

        let adapters = vec![&adapter];
        let gates = vec![32767];

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];

        // Apply LoRA to module that doesn't have weights
        let result = apply_multi_lora(
            &adapters,
            &gates,
            "nonexistent_module",
            &input,
            &base_output,
        );

        // Should succeed but return base output (no adapters qualify)
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output, base_output);
    }

    #[test]
    fn test_lora_gate_adapter_mismatch() {
        let adapter1 = LoRAAdapter::new("test1".to_string(), LoRAConfig::default());
        let adapter2 = LoRAAdapter::new("test2".to_string(), LoRAConfig::default());

        let adapters = vec![&adapter1, &adapter2];
        let gates = vec![32767]; // Only 1 gate for 2 adapters

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];

        // Mismatched gates and adapters
        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output);

        // Should handle gracefully (only apply first adapter)
        assert!(result.is_ok());
    }

    #[test]
    fn test_lora_empty_weights() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test".to_string(), config);

        // No weights added
        assert!(!adapter.has_module("q_proj"));
        assert_eq!(adapter.parameter_count(), 0);
        assert_eq!(adapter.memory_usage(), 0);
    }
}

#[cfg(test)]
mod model_error_tests {
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use tempfile::TempDir;

    #[test]
    fn test_model_load_missing_config() {
        let temp_dir = TempDir::new().unwrap();

        // Try to load from directory without config.json
        let result = MLXFFIModel::load(temp_dir.path());

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{:?}", e);
            assert!(
                error_msg.contains("config")
                    || error_msg.contains("Io")
                    || error_msg.contains("NotFound")
            );
        }
    }

    #[test]
    fn test_model_load_invalid_path() {
        let result = MLXFFIModel::load("/nonexistent/invalid/path");

        assert!(result.is_err());
    }

    #[test]
    fn test_model_load_malformed_config() {
        let temp_dir = TempDir::new().unwrap();

        // Create invalid config.json
        let config_path = temp_dir.path().join("config.json");
        std::fs::write(&config_path, "{ invalid json }").unwrap();

        let result = MLXFFIModel::load(temp_dir.path());

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{:?}", e);
            assert!(error_msg.contains("Parse") || error_msg.contains("parse"));
        }
    }
}

#[cfg(test)]
mod memory_error_tests {
    use adapteros_lora_mlx_ffi::memory;

    #[test]
    fn test_memory_operations_no_crash() {
        // All memory operations should work even with no allocations
        memory::reset();

        let _ = memory::memory_usage();
        let _ = memory::allocation_count();
        let _ = memory::memory_stats();
        let _ = memory::stats();

        memory::gc_collect();

        // No crashes = success
    }

    #[test]
    fn test_memory_threshold_edge_cases() {
        memory::reset();

        // Test with various threshold values
        assert!(!memory::exceeds_threshold(f32::MAX));
        assert!(memory::exceeds_threshold(-1.0));
        assert!(!memory::exceeds_threshold(0.0));
    }

    #[test]
    fn test_memory_stats_consistency() {
        memory::reset();

        let (total, count) = memory::memory_stats();
        let stats = memory::stats();

        assert_eq!(total, stats.total_bytes);
        assert_eq!(count, stats.allocation_count);
    }
}

#[cfg(test)]
mod boundary_error_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_tensor_null_ptr_safety() {
        // Creating tensors should be safe even with edge cases
        let data = vec![1.0];
        let result = MLXFFITensor::from_data(&data, vec![1]);

        assert!(result.is_ok());
    }

    #[test]
    fn test_tensor_operations_null_handling() {
        let data = vec![1.0, 2.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2]).unwrap();

        // Operations should handle internal state safely
        let _ = tensor.shape();
        let _ = tensor.size();
        let _ = tensor.dtype();
    }
}

#[cfg(test)]
mod routing_error_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_lora_mlx_ffi::routing::{compute_adapter_score, select_top_k_adapters};

    #[test]
    fn test_routing_empty_input() {
        let adapter = LoRAAdapter::new("test".to_string(), LoRAConfig::default());
        let empty_features: Vec<f32> = vec![];

        let score = compute_adapter_score(&adapter, &empty_features, "q_proj");

        // Should handle empty features gracefully
        let _ = score;
    }

    #[test]
    fn test_routing_empty_scores() {
        let adapters: Vec<&LoRAAdapter> = vec![];
        let scores: Vec<f32> = vec![];

        let top_k = select_top_k_adapters(&adapters, &scores, 3);

        assert_eq!(top_k.len(), 0);
    }

    #[test]
    fn test_routing_nan_scores() {
        let adapter1 = LoRAAdapter::new("test1".to_string(), LoRAConfig::default());
        let adapter2 = LoRAAdapter::new("test2".to_string(), LoRAConfig::default());

        let adapters = vec![&adapter1, &adapter2];
        let scores = vec![f32::NAN, 0.5];

        // Should handle NaN gracefully
        let top_k = select_top_k_adapters(&adapters, &scores, 2);

        // May include NaN or filter it out
        let _ = top_k;
    }

    #[test]
    fn test_routing_infinite_scores() {
        let adapter1 = LoRAAdapter::new("test1".to_string(), LoRAConfig::default());
        let adapter2 = LoRAAdapter::new("test2".to_string(), LoRAConfig::default());

        let adapters = vec![&adapter1, &adapter2];
        let scores = vec![f32::INFINITY, 0.5];

        // Should handle infinity gracefully
        let top_k = select_top_k_adapters(&adapters, &scores, 2);

        assert_eq!(top_k.len(), 2);
    }
}

#[cfg(test)]
mod concurrency_error_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_adapter_registration() {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);

        let backend = Arc::new(MLXFFIBackend::new(model));

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let backend_clone = Arc::clone(&backend);
                thread::spawn(move || {
                    let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
                    backend_clone.register_adapter(i as u16, adapter).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(backend.adapter_count(), 4);
    }

    #[test]
    fn test_concurrent_adapter_access() {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);

        let backend = Arc::new(MLXFFIBackend::new(model));

        // Pre-register adapters
        for i in 0..4 {
            let adapter = create_mock_adapter(&format!("adapter{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        // Concurrent reads
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let backend_clone = Arc::clone(&backend);
                thread::spawn(move || {
                    let count = backend_clone.adapter_count();
                    assert_eq!(count, 4);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
