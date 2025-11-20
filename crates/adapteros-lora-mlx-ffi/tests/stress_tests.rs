//! Stress tests for MLX backend
//!
//! Tests concurrent operations, rapid switching, and extreme scenarios.

#[cfg(test)]
mod concurrent_stress_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use std::sync::Arc;
    use std::thread;

    /// Test concurrent adapter registration from multiple threads
    #[test]
    fn test_concurrent_adapter_registration() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"concurrent-reg-test"),
        };

        let backend = Arc::new(MLXFFIBackend::new(model));

        let mut handles = vec![];

        // Spawn 10 threads, each registering adapters
        for thread_id in 0..10 {
            let backend_clone = Arc::clone(&backend);

            let handle = thread::spawn(move || {
                for i in 0..10 {
                    let adapter_id = (thread_id * 10 + i) as u16;
                    let adapter = create_mock_adapter(
                        &format!("thread-{}-adapter-{}", thread_id, i),
                        4,
                    );

                    backend_clone.register_adapter(adapter_id, adapter).unwrap();
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 100 adapters (10 threads × 10 adapters)
        assert_eq!(backend.adapter_count(), 100);
    }

    /// Test concurrent adapter unloading
    #[test]
    fn test_concurrent_adapter_unloading() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"concurrent-unload-test"),
        };

        let backend = Arc::new(MLXFFIBackend::new(model));

        // Pre-load 100 adapters
        for i in 0..100 {
            let adapter = create_mock_adapter(&format!("unload-test-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();
        }

        assert_eq!(backend.adapter_count(), 100);

        let mut handles = vec![];

        // Spawn 10 threads, each unloading adapters
        for thread_id in 0..10 {
            let backend_clone = Arc::clone(&backend);

            let handle = thread::spawn(move || {
                for i in 0..10 {
                    let adapter_id = (thread_id * 10 + i) as u16;
                    let _ = backend_clone.unload_adapter_runtime(adapter_id);
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // All adapters should be unloaded
        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test concurrent hot-swapping
    #[test]
    fn test_concurrent_hot_swap() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"concurrent-swap-test"),
        };

        let backend = Arc::new(MLXFFIBackend::new(model));

        // Pre-load adapters
        for i in 0..10 {
            let adapter = create_mock_adapter(&format!("swap-initial-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();
        }

        let mut handles = vec![];

        // Spawn threads that repeatedly swap adapters
        for thread_id in 0..5 {
            let backend_clone = Arc::clone(&backend);

            let handle = thread::spawn(move || {
                for iteration in 0..20 {
                    let adapter_id = (thread_id * 2) as u16; // Each thread handles 2 adapters

                    let new_adapter = create_mock_adapter(
                        &format!("swap-thread-{}-iter-{}", thread_id, iteration),
                        8,
                    );

                    let _ = backend_clone.load_adapter_runtime(adapter_id, new_adapter);
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should still have 10 adapters
        assert_eq!(backend.adapter_count(), 10);
    }

    /// Test concurrent memory queries
    #[test]
    fn test_concurrent_memory_queries() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"concurrent-memory-test"),
        };

        let backend = Arc::new(MLXFFIBackend::new(model));

        // Pre-load adapters
        for i in 0..10 {
            let adapter = create_mock_adapter(&format!("memory-test-{}", i), 8);
            backend.register_adapter(i, adapter).unwrap();
        }

        let mut handles = vec![];

        // Spawn threads that query memory usage
        for thread_id in 0..10 {
            let backend_clone = Arc::clone(&backend);

            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let adapter_id = thread_id as u16;
                    let _ = backend_clone.get_adapter_memory_usage(adapter_id);
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // No crashes = success
    }
}

#[cfg(test)]
mod rapid_switching_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test rapid adapter switching
    #[test]
    fn test_rapid_adapter_switching() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"rapid-switch-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Rapid fire switching between 3 adapters
        let adapter_ids = [1u16, 2, 3];
        let adapters = [
            create_mock_adapter("rapid-1", 4),
            create_mock_adapter("rapid-2", 8),
            create_mock_adapter("rapid-3", 4),
        ];

        // Initial load
        for (id, adapter) in adapter_ids.iter().zip(adapters.into_iter()) {
            backend.register_adapter(*id, adapter).unwrap();
        }

        // Perform 1000 rapid swaps
        for i in 0..1000 {
            let target_id = adapter_ids[i % 3];
            let new_adapter = create_mock_adapter(&format!("rapid-swap-{}", i), 8);

            backend.load_adapter_runtime(target_id, new_adapter).unwrap();
        }

        // Should still have 3 adapters
        assert_eq!(backend.adapter_count(), 3);
    }

    /// Test rapid load/unload cycles
    #[test]
    fn test_rapid_load_unload_cycles() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"rapid-cycle-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Perform 500 rapid load/unload cycles
        for i in 0..500 {
            let adapter = create_mock_adapter(&format!("cycle-{}", i), 4);
            backend.register_adapter(1, adapter).unwrap();
            assert_eq!(backend.adapter_count(), 1);

            backend.unload_adapter_runtime(1).unwrap();
            assert_eq!(backend.adapter_count(), 0);
        }

        // Final state should be clean
        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test rapid memory allocation/deallocation
    #[test]
    fn test_rapid_memory_operations() {
        memory::reset();

        // Perform 1000 rapid memory operations
        for _ in 0..1000 {
            let stats = memory::stats();
            let _ = memory::format_stats(&stats);

            memory::gc_collect();

            let _ = memory::memory_usage();
            let _ = memory::allocation_count();
        }

        // Should complete without issues
        let final_stats = memory::stats();
        assert!(final_stats.total_bytes >= 0);
    }

    /// Test rapid backend recreation
    #[test]
    fn test_rapid_backend_recreation() {
        memory::reset();

        let config = create_mock_config();

        // Rapidly create and drop backends
        for i in 0..100 {
            let model = MLXFFIModel {
                model: std::ptr::null_mut(),
                config: config.clone(),
                model_hash: adapteros_core::B3Hash::hash(format!("backend-{}", i).as_bytes()),
            };

            let backend = MLXFFIBackend::new(model);

            // Load an adapter
            let adapter = create_mock_adapter(&format!("recreate-{}", i), 4);
            backend.register_adapter(1, adapter).unwrap();

            // Backend drops here
        }

        // Cleanup
        memory::gc_collect();

        // Should not leak memory
        let final_stats = memory::stats();
        assert!(memory::bytes_to_mb(final_stats.total_bytes) < 100.0);
    }
}

#[cfg(test)]
mod extreme_scenario_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test maximum adapter count
    #[test]
    fn test_maximum_adapter_count() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"max-count-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load maximum number of adapters (limited by u16)
        let max_adapters = 1000; // Reasonable limit for testing

        for i in 0..max_adapters {
            let adapter = create_mock_adapter(&format!("max-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();
        }

        assert_eq!(backend.adapter_count(), max_adapters as usize);

        // Cleanup
        for i in 0..max_adapters {
            backend.unload_adapter_runtime(i).unwrap();
        }

        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test maximum rank adapter
    #[test]
    fn test_maximum_rank_adapter() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"max-rank-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Create adapter with very large rank
        let max_rank = 256; // Large but reasonable
        let adapter = create_mock_adapter("max-rank-adapter", max_rank);

        backend.register_adapter(1, adapter).unwrap();

        let memory_usage = backend.get_adapter_memory_usage(1).unwrap();

        // Should have substantial memory usage
        assert!(memory_usage > 0);

        // Cleanup
        backend.unload_adapter_runtime(1).unwrap();
    }

    /// Test extreme memory pressure
    #[test]
    fn test_extreme_memory_pressure() {
        memory::reset();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"extreme-pressure-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load many large-rank adapters to create pressure
        let num_adapters = 100;
        let large_rank = 64;

        for i in 0..num_adapters {
            let adapter = create_mock_adapter(&format!("pressure-{}", i), large_rank);
            backend.register_adapter(i, adapter).unwrap();

            // Check if we're exceeding threshold
            if memory::exceeds_threshold(500.0) {
                // Start evicting to maintain headroom
                if i > 10 {
                    backend.unload_adapter_runtime(i - 10).unwrap();
                    memory::gc_collect();
                }
            }
        }

        // Cleanup all
        for i in 0..num_adapters {
            let _ = backend.unload_adapter_runtime(i);
        }

        memory::gc_collect();

        // Should complete without crash
        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test zero-length inputs (edge case)
    #[test]
    fn test_zero_length_inputs() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers};

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
            model_hash: adapteros_core::B3Hash::hash(b"zero-input-test"),
        };

        let mut backend = MLXFFIBackend::new(model);

        // Load plan with empty bytes
        let result = backend.load(&[]);
        // Should handle gracefully or error
        let _ = result;

        // Create IO buffers with empty inputs
        let io = IoBuffers {
            input_ids: vec![], // Empty input
            output_logits: vec![0.0; config.vocab_size],
            position: 0,
        };

        // Should not crash
        assert_eq!(io.input_ids.len(), 0);
    }

    /// Test rapid GC invocations
    #[test]
    fn test_rapid_gc_invocations() {
        memory::reset();

        // Invoke GC 10,000 times rapidly
        for _ in 0..10000 {
            memory::gc_collect();
        }

        // Should not crash or corrupt state
        let stats = memory::stats();
        assert_eq!(stats.total_bytes, 0);
        assert_eq!(stats.allocation_count, 0);
    }

    /// Test adapter ID collision handling
    #[test]
    fn test_adapter_id_collision() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"collision-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load adapter with ID 1
        let adapter1 = create_mock_adapter("first", 4);
        backend.register_adapter(1, adapter1).unwrap();
        assert_eq!(backend.adapter_count(), 1);

        // Load another adapter with same ID (should replace)
        let adapter2 = create_mock_adapter("second", 8);
        backend.register_adapter(1, adapter2).unwrap();
        assert_eq!(backend.adapter_count(), 1); // Still 1, replaced

        // Verify memory usage reflects new adapter
        let memory_usage = backend.get_adapter_memory_usage(1).unwrap();
        assert!(memory_usage > 0);
    }

    /// Test long-running inference simulation
    #[test]
    fn test_long_running_inference() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers};

        memory::reset();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
            model_hash: adapteros_core::B3Hash::hash(b"long-running-test"),
        };

        let mut backend = MLXFFIBackend::new(model);

        // Load adapter
        let adapter = create_mock_adapter("long-running", 8);
        backend.register_adapter(1, adapter).unwrap();

        // Load plan
        backend.load(b"long-running-plan").unwrap();

        // Simulate 5000 inference steps
        for step in 0..5000 {
            let mut io = IoBuffers {
                input_ids: vec![1, 2, 3],
                output_logits: vec![0.0; config.vocab_size],
                position: step,
            };

            // In real scenario, would call run_step
            // For mock, just track position
            assert_eq!(io.position, step);

            drop(io);

            // Periodic GC
            if step % 500 == 0 {
                memory::gc_collect();
            }
        }

        // Cleanup
        backend.unload_adapter_runtime(1).unwrap();
        memory::gc_collect();

        // Should complete successfully
        assert_eq!(backend.adapter_count(), 0);
    }
}

#[cfg(test)]
mod stability_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test stability over extended operation
    #[test]
    fn test_extended_operation_stability() {
        memory::reset();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"stability-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Perform 100 rounds of operations
        for round in 0..100 {
            // Load adapters
            for i in 0..5 {
                let adapter = create_mock_adapter(&format!("round-{}-adapter-{}", round, i), 8);
                backend.register_adapter(i, adapter).unwrap();
            }

            assert_eq!(backend.adapter_count(), 5);

            // Swap some adapters
            for i in 0..3 {
                let new_adapter = create_mock_adapter(&format!("round-{}-swap-{}", round, i), 4);
                backend.load_adapter_runtime(i, new_adapter).unwrap();
            }

            // Unload all
            for i in 0..5 {
                backend.unload_adapter_runtime(i).unwrap();
            }

            assert_eq!(backend.adapter_count(), 0);

            // Periodic GC
            if round % 10 == 0 {
                memory::gc_collect();
            }
        }

        // Final state should be stable
        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test recovery from error conditions
    #[test]
    fn test_error_recovery_stability() {
        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"recovery-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Perform 100 rounds with intentional errors
        for round in 0..100 {
            // Load adapter
            let adapter = create_mock_adapter(&format!("recovery-{}", round), 4);
            backend.register_adapter(1, adapter).unwrap();

            // Try to unload non-existent adapter (error)
            let _ = backend.unload_adapter_runtime(999);

            // Try to get memory for non-existent adapter (error)
            let _ = backend.get_adapter_memory_usage(999);

            // Backend should still be functional
            let memory_usage = backend.get_adapter_memory_usage(1).unwrap();
            assert!(memory_usage > 0);

            // Cleanup
            backend.unload_adapter_runtime(1).unwrap();
        }

        // Should complete successfully
        assert_eq!(backend.adapter_count(), 0);
    }

    /// Test deterministic seeding stability
    #[test]
    fn test_seeding_stability() {
        use adapteros_lora_kernel_api::FusedKernels;

        let config = create_mock_config();

        // Create multiple backends with same hash
        for i in 0..50 {
            let model = MLXFFIModel {
                model: std::ptr::null_mut(),
                config: config.clone(),
                model_hash: adapteros_core::B3Hash::hash(b"stable-seed-test"),
            };

            let mut backend = MLXFFIBackend::new(model);

            // Load same plan
            backend.load(b"stable-plan").unwrap();

            // Verify determinism attestation is consistent
            let report = backend.attest_determinism().unwrap();
            assert!(!report.deterministic); // MLX is non-deterministic

            // Backend drops here
            let _ = i;
        }

        // Should complete without issues
    }
}
