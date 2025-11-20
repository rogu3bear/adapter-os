//! Regression tests for MLX backend
//!
//! Tests to detect performance regressions, accuracy issues, and API compatibility.

#[cfg(test)]
mod performance_regression_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use std::time::Instant;

    /// Baseline performance metrics for adapter operations
    const MAX_ADAPTER_REGISTRATION_MS: u128 = 100;
    const MAX_ADAPTER_UNLOAD_MS: u128 = 50;
    const MAX_MEMORY_QUERY_MS: u128 = 10;
    const MAX_HOT_SWAP_MS: u128 = 100;

    /// Test adapter registration performance
    #[test]
    fn test_adapter_registration_performance() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"perf-reg-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Measure registration time for 10 adapters
        let start = Instant::now();

        for i in 0..10 {
            let adapter = create_mock_adapter(&format!("perf-{}", i), 8);
            backend.register_adapter(i, adapter).unwrap();
        }

        let elapsed = start.elapsed().as_millis();
        let avg_per_adapter = elapsed / 10;

        assert!(
            avg_per_adapter < MAX_ADAPTER_REGISTRATION_MS,
            "Adapter registration too slow: {} ms/adapter (max: {} ms)",
            avg_per_adapter,
            MAX_ADAPTER_REGISTRATION_MS
        );
    }

    /// Test adapter unload performance
    #[test]
    fn test_adapter_unload_performance() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"perf-unload-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Pre-load adapters
        for i in 0..10 {
            let adapter = create_mock_adapter(&format!("unload-perf-{}", i), 8);
            backend.register_adapter(i, adapter).unwrap();
        }

        // Measure unload time
        let start = Instant::now();

        for i in 0..10 {
            backend.unload_adapter_runtime(i).unwrap();
        }

        let elapsed = start.elapsed().as_millis();
        let avg_per_adapter = elapsed / 10;

        assert!(
            avg_per_adapter < MAX_ADAPTER_UNLOAD_MS,
            "Adapter unload too slow: {} ms/adapter (max: {} ms)",
            avg_per_adapter,
            MAX_ADAPTER_UNLOAD_MS
        );
    }

    /// Test memory query performance
    #[test]
    fn test_memory_query_performance() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"perf-memory-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Pre-load adapter
        let adapter = create_mock_adapter("memory-perf", 8);
        backend.register_adapter(1, adapter).unwrap();

        // Measure memory query time
        let start = Instant::now();

        for _ in 0..100 {
            let _ = backend.get_adapter_memory_usage(1).unwrap();
        }

        let elapsed = start.elapsed().as_millis();
        let avg_per_query = elapsed / 100;

        assert!(
            avg_per_query < MAX_MEMORY_QUERY_MS,
            "Memory query too slow: {} ms/query (max: {} ms)",
            avg_per_query,
            MAX_MEMORY_QUERY_MS
        );
    }

    /// Test hot-swap performance
    #[test]
    fn test_hot_swap_performance() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"perf-swap-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Initial adapter
        let adapter = create_mock_adapter("initial", 8);
        backend.register_adapter(1, adapter).unwrap();

        // Measure hot-swap time
        let start = Instant::now();

        for i in 0..10 {
            let new_adapter = create_mock_adapter(&format!("swap-{}", i), 8);
            backend.load_adapter_runtime(1, new_adapter).unwrap();
        }

        let elapsed = start.elapsed().as_millis();
        let avg_per_swap = elapsed / 10;

        assert!(
            avg_per_swap < MAX_HOT_SWAP_MS,
            "Hot-swap too slow: {} ms/swap (max: {} ms)",
            avg_per_swap,
            MAX_HOT_SWAP_MS
        );
    }

    /// Test memory efficiency (no unbounded growth)
    #[test]
    fn test_memory_efficiency_regression() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"memory-efficiency-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Perform 500 operations
        for i in 0..500 {
            let adapter = create_mock_adapter(&format!("efficiency-{}", i), 4);
            backend.register_adapter(1, adapter).unwrap();
            backend.unload_adapter_runtime(1).unwrap();

            if i % 50 == 0 {
                memory::gc_collect();
            }
        }

        memory::gc_collect();

        let final_stats = memory::stats();
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        // Should not grow more than 50 MB over 500 cycles
        assert!(
            growth_mb < 50.0,
            "Memory efficiency regression: grew {:.2} MB (max: 50 MB)",
            growth_mb
        );
    }

    /// Test GC performance
    #[test]
    fn test_gc_performance() {
        memory::reset();

        // Measure GC time
        let start = Instant::now();

        for _ in 0..100 {
            memory::gc_collect();
        }

        let elapsed = start.elapsed().as_millis();
        let avg_gc_time = elapsed / 100;

        // GC should be fast (< 10ms per call)
        assert!(
            avg_gc_time < 10,
            "GC too slow: {} ms/call (max: 10 ms)",
            avg_gc_time
        );
    }

    /// Test adapter count query performance
    #[test]
    fn test_adapter_count_performance() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"count-perf-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Pre-load 50 adapters
        for i in 0..50 {
            let adapter = create_mock_adapter(&format!("count-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();
        }

        // Measure count query time
        let start = Instant::now();

        for _ in 0..1000 {
            let _ = backend.adapter_count();
        }

        let elapsed = start.elapsed().as_micros();
        let avg_per_query = elapsed / 1000;

        // Should be very fast (< 100 microseconds)
        assert!(
            avg_per_query < 100,
            "Adapter count query too slow: {} μs/query (max: 100 μs)",
            avg_per_query
        );
    }
}

#[cfg(test)]
mod accuracy_validation_tests {
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config, MockMLXFFIModel};

    /// Test output accuracy consistency
    #[test]
    fn test_output_accuracy_consistency() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        let token_ids = vec![100, 200, 300, 400, 500];

        // Run multiple times, should get consistent results
        let logits1 = model.forward(&token_ids, 0).unwrap();
        let logits2 = model.forward(&token_ids, 0).unwrap();

        // Should be identical
        assert_eq!(logits1.len(), logits2.len());

        for (i, (&l1, &l2)) in logits1.iter().zip(logits2.iter()).enumerate() {
            assert!(
                (l1 - l2).abs() < 1e-6,
                "Logit mismatch at index {}: {} vs {}",
                i,
                l1,
                l2
            );
        }
    }

    /// Test hidden states accuracy
    #[test]
    fn test_hidden_states_accuracy() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        let token_ids = vec![1, 2, 3];
        let (logits, hidden_states) = model.forward_with_hidden_states(&token_ids).unwrap();

        // Validate expected structure
        assert_eq!(hidden_states.len(), 4);

        // Check each module
        for (module_name, hidden) in &hidden_states {
            match module_name.as_str() {
                "q_proj" => {
                    assert_eq!(hidden.len(), 128);
                    // Should be all 1.0 in mock
                    for &val in hidden {
                        assert!((val - 1.0).abs() < 1e-6);
                    }
                }
                "k_proj" => {
                    assert_eq!(hidden.len(), 128);
                    for &val in hidden {
                        assert!((val - 2.0).abs() < 1e-6);
                    }
                }
                "v_proj" => {
                    assert_eq!(hidden.len(), 128);
                    for &val in hidden {
                        assert!((val - 3.0).abs() < 1e-6);
                    }
                }
                "o_proj" => {
                    assert_eq!(hidden.len(), 128);
                    for &val in hidden {
                        assert!((val - 4.0).abs() < 1e-6);
                    }
                }
                _ => panic!("Unexpected module: {}", module_name),
            }
        }

        // Logits should be correct
        assert_eq!(logits.len(), 32000);
    }

    /// Test adapter weight accuracy
    #[test]
    fn test_adapter_weight_accuracy() {
        let adapter = create_mock_adapter("accuracy-test", 8);

        // Verify adapter has correct structure
        assert_eq!(adapter.config().rank, 8);
        assert_eq!(adapter.config().target_modules.len(), 4);

        // Check that all target modules exist
        assert!(adapter.has_module("q_proj"));
        assert!(adapter.has_module("k_proj"));
        assert!(adapter.has_module("v_proj"));
        assert!(adapter.has_module("o_proj"));

        // Verify weights are accessible
        for module_name in &adapter.config().target_modules {
            let weights = adapter.get_full_weights(module_name);
            assert!(
                weights.is_some(),
                "Module {} missing weights",
                module_name
            );
        }
    }

    /// Test numerical stability
    #[test]
    fn test_numerical_stability() {
        let config = create_mock_config();
        let model = MockMLXFFIModel::new(config);

        // Test with various token sequences
        let test_cases = vec![
            vec![1, 2, 3],
            vec![100, 200, 300],
            vec![1000, 2000, 3000],
            vec![10000, 20000, 30000],
        ];

        for token_ids in test_cases {
            let logits = model.forward(&token_ids, 0).unwrap();

            // Check for numerical issues
            for (i, &logit) in logits.iter().enumerate() {
                assert!(
                    logit.is_finite(),
                    "Non-finite logit at index {} for tokens {:?}",
                    i,
                    token_ids
                );
                assert!(
                    logit.abs() < 1e6,
                    "Extreme logit value at index {}: {}",
                    i,
                    logit
                );
            }
        }
    }
}

#[cfg(test)]
mod api_compatibility_tests {
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test FusedKernels trait implementation
    #[test]
    fn test_fused_kernels_trait_compatibility() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"trait-compat-test"),
        };

        let mut backend = MLXFFIBackend::new(model);

        // Verify all trait methods are implemented
        let _ = backend.attest_determinism();
        let _ = backend.load(b"test-plan");
        let _ = backend.device_name();
    }

    /// Test backend API stability
    #[test]
    fn test_backend_api_stability() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"api-stability-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Verify all public methods exist and work
        let adapter = create_mock_adapter("api-test", 8);

        // register_adapter
        backend.register_adapter(1, adapter).unwrap();

        // adapter_count
        assert_eq!(backend.adapter_count(), 1);

        // get_adapter_memory_usage
        let _ = backend.get_adapter_memory_usage(1).unwrap();

        // load_adapter_runtime
        let new_adapter = create_mock_adapter("api-test-2", 8);
        backend.load_adapter_runtime(1, new_adapter).unwrap();

        // unload_adapter_runtime
        backend.unload_adapter_runtime(1).unwrap();

        // device_name
        let _ = backend.device_name();
    }

    /// Test RouterRing compatibility
    #[test]
    fn test_router_ring_compatibility() {
        // Test RouterRing creation and usage
        let ring = RouterRing::new(3);

        assert_eq!(ring.k, 3);
        assert_eq!(ring.position, 0);

        // Verify we can set indices and gates
        let mut ring_mut = ring;
        ring_mut.indices[0] = 1;
        ring_mut.indices[1] = 2;
        ring_mut.indices[2] = 3;

        ring_mut.gates_q15[0] = 16384;
        ring_mut.gates_q15[1] = 12288;
        ring_mut.gates_q15[2] = 8192;

        assert_eq!(ring_mut.indices[0], 1);
        assert_eq!(ring_mut.gates_q15[0], 16384);
    }

    /// Test IoBuffers compatibility
    #[test]
    fn test_io_buffers_compatibility() {
        let config = create_mock_config();

        // Test IoBuffers creation and usage
        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3, 4, 5],
            output_logits: vec![0.0; config.vocab_size],
            position: 0,
        };

        assert_eq!(io.input_ids.len(), 5);
        assert_eq!(io.output_logits.len(), 32000);

        // Update position
        io.position += 1;
        assert_eq!(io.position, 1);

        // Update logits
        io.output_logits[0] = 1.23;
        assert!((io.output_logits[0] - 1.23).abs() < 1e-6);
    }

    /// Test memory module API
    #[test]
    fn test_memory_module_api_compatibility() {
        use adapteros_lora_mlx_ffi::memory;

        // Verify all memory API functions exist
        memory::reset();
        let _ = memory::memory_usage();
        let _ = memory::allocation_count();
        let _ = memory::memory_stats();
        let _ = memory::stats();
        let _ = memory::bytes_to_mb(1024);
        let _ = memory::exceeds_threshold(100.0);
        memory::gc_collect();

        let stats = memory::stats();
        let _ = memory::format_stats(&stats);
    }

    /// Test LoRA adapter API
    #[test]
    fn test_lora_adapter_api_compatibility() {
        let adapter = create_mock_adapter("api-compat", 8);

        // Verify adapter API
        assert_eq!(adapter.id(), "api-compat");
        assert_eq!(adapter.config().rank, 8);
        assert!(adapter.has_module("q_proj"));

        let weights = adapter.get_full_weights("q_proj");
        assert!(weights.is_some());
    }

    /// Test determinism attestation API
    #[test]
    fn test_determinism_attestation_api() {
        use adapteros_lora_kernel_api::attestation::*;
        use adapteros_lora_kernel_api::FusedKernels;

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"attestation-test"),
        };

        let backend = MLXFFIBackend::new(model);

        let report = backend.attest_determinism().unwrap();

        // Verify report structure
        assert_eq!(report.backend_type, BackendType::Mlx);
        assert!(!report.deterministic);
        assert_eq!(report.rng_seed_method, RngSeedingMethod::HkdfSeeded);
    }
}

#[cfg(test)]
mod determinism_regression_tests {
    use adapteros_lora_kernel_api::FusedKernels;
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::mock::create_mock_config;
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test deterministic seeding consistency
    #[test]
    fn test_seeding_consistency() {
        // Create two backends with same model hash
        let config = create_mock_config();

        let model1 = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
            model_hash: adapteros_core::B3Hash::hash(b"determinism-test"),
        };

        let model2 = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
            model_hash: adapteros_core::B3Hash::hash(b"determinism-test"),
        };

        let mut backend1 = MLXFFIBackend::new(model1);
        let mut backend2 = MLXFFIBackend::new(model2);

        // Load same plan
        backend1.load(b"determinism-plan").unwrap();
        backend2.load(b"determinism-plan").unwrap();

        // Both should report same determinism characteristics
        let report1 = backend1.attest_determinism().unwrap();
        let report2 = backend2.attest_determinism().unwrap();

        assert_eq!(report1.deterministic, report2.deterministic);
        assert_eq!(report1.backend_type, report2.backend_type);
        assert_eq!(report1.rng_seed_method, report2.rng_seed_method);
    }

    /// Test HKDF seed derivation consistency
    #[test]
    fn test_hkdf_derivation_consistency() {
        use adapteros_core::{derive_seed, B3Hash};

        let base_hash = B3Hash::hash(b"test-model");

        // Derive seeds multiple times with same inputs
        let seed1 = derive_seed(&base_hash, "mlx-step:0");
        let seed2 = derive_seed(&base_hash, "mlx-step:0");

        // Should be identical
        assert_eq!(seed1, seed2);

        // Different labels should produce different seeds
        let seed3 = derive_seed(&base_hash, "mlx-step:1");
        assert_ne!(seed1, seed3);
    }

    /// Test adapter seeding determinism
    #[test]
    fn test_adapter_seeding_determinism() {
        use adapteros_lora_mlx_ffi::mock::create_mock_adapter;

        // Create adapters with same ID
        let adapter1 = create_mock_adapter("determinism-test", 8);
        let adapter2 = create_mock_adapter("determinism-test", 8);

        // Should have identical configuration
        assert_eq!(adapter1.config().rank, adapter2.config().rank);
        assert_eq!(adapter1.config().alpha, adapter2.config().alpha);
        assert_eq!(adapter1.config().dropout, adapter2.config().dropout);
    }
}

#[cfg(test)]
mod version_compatibility_tests {
    use adapteros_lora_mlx_ffi::mock::create_mock_config;
    use adapteros_lora_mlx_ffi::{LoRAConfig, ModelConfig};

    /// Test ModelConfig serialization stability
    #[test]
    fn test_model_config_serialization() {
        let config = create_mock_config();

        // Serialize to JSON
        let json = serde_json::to_string(&config).unwrap();

        // Deserialize back
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();

        // Should be identical
        assert_eq!(config.hidden_size, deserialized.hidden_size);
        assert_eq!(config.num_hidden_layers, deserialized.num_hidden_layers);
        assert_eq!(config.vocab_size, deserialized.vocab_size);
    }

    /// Test LoRAConfig serialization stability
    #[test]
    fn test_lora_config_serialization() {
        let config = LoRAConfig::default();

        // Serialize to JSON
        let json = serde_json::to_string(&config).unwrap();

        // Deserialize back
        let deserialized: LoRAConfig = serde_json::from_str(&json).unwrap();

        // Should be identical
        assert_eq!(config.rank, deserialized.rank);
        assert_eq!(config.alpha, deserialized.alpha);
        assert_eq!(config.dropout, deserialized.dropout);
    }

    /// Test default LoRAConfig values
    #[test]
    fn test_lora_config_defaults() {
        let config = LoRAConfig::default();

        // Verify expected defaults
        assert_eq!(config.rank, 4);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(config.dropout, 0.1);
        assert_eq!(config.target_modules.len(), 4);
        assert!(config.target_modules.contains(&"q_proj".to_string()));
        assert!(config.target_modules.contains(&"k_proj".to_string()));
        assert!(config.target_modules.contains(&"v_proj".to_string()));
        assert!(config.target_modules.contains(&"o_proj".to_string()));
    }
}
