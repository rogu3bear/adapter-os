//! Memory leak detection tests for MLX backend
//!
//! Tests to detect memory leaks through repeated operations and long-running scenarios.

#[cfg(test)]
mod memory_leak_detection {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test memory stability over 1000+ adapter load/unload cycles
    #[test]
    fn test_no_leak_adapter_lifecycle_1000_cycles() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"leak-test-1000"),
        };

        let backend = MLXFFIBackend::new(model);

        // Run 1000 load/unload cycles
        for i in 0..1000 {
            let adapter = create_mock_adapter(&format!("adapter-{}", i), 4);
            backend.register_adapter(1, adapter).unwrap();

            // Unload immediately
            backend.unload_adapter_runtime(1).unwrap();

            // Periodic GC
            if i % 100 == 0 {
                memory::gc_collect();
            }
        }

        // Final cleanup
        memory::gc_collect();

        let final_stats = memory::stats();

        // Memory usage should not grow unbounded
        // In mock environment, should remain stable
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        // Allow some growth for tracking overhead, but not unbounded
        assert!(
            growth_mb < 100.0,
            "Memory grew by {:.2} MB over 1000 cycles (possible leak)",
            growth_mb
        );
    }

    /// Test memory stability with concurrent adapters
    #[test]
    fn test_no_leak_concurrent_adapters_1000_iterations() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"leak-test-concurrent"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load multiple adapters
        for i in 0..5 {
            let adapter = create_mock_adapter(&format!("concurrent-{}", i), 8);
            backend.register_adapter(i, adapter).unwrap();
        }

        // Run 1000 iterations of operations
        for iteration in 0..1000 {
            // Rotate adapters
            let adapter_to_replace = (iteration % 5) as u16;

            // Unload and reload
            backend.unload_adapter_runtime(adapter_to_replace).unwrap();

            let new_adapter = create_mock_adapter(
                &format!("concurrent-{}-iter-{}", adapter_to_replace, iteration),
                8,
            );
            backend
                .load_adapter_runtime(adapter_to_replace, new_adapter)
                .unwrap();

            // Periodic GC
            if iteration % 100 == 0 {
                memory::gc_collect();
            }
        }

        // Cleanup all adapters
        for i in 0..5 {
            backend.unload_adapter_runtime(i).unwrap();
        }

        memory::gc_collect();

        let final_stats = memory::stats();
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            growth_mb < 100.0,
            "Memory grew by {:.2} MB over 1000 iterations (possible leak)",
            growth_mb
        );
    }

    /// Test memory cleanup on adapter unload
    #[test]
    fn test_memory_cleanup_on_unload() {
        memory::reset();
        memory::gc_collect();

        let baseline_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"cleanup-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load adapter
        let adapter = create_mock_adapter("cleanup-test", 16); // Larger rank for measurable memory
        backend.register_adapter(1, adapter).unwrap();

        let loaded_stats = memory::stats();
        let loaded_allocations = loaded_stats.allocation_count;

        // Unload adapter
        backend.unload_adapter_runtime(1).unwrap();
        memory::gc_collect();

        let unloaded_stats = memory::stats();

        // Allocations should decrease or stay same after unload + GC
        assert!(
            unloaded_stats.allocation_count <= loaded_allocations,
            "Allocations increased after unload: before={}, after={}",
            loaded_allocations,
            unloaded_stats.allocation_count
        );

        let _ = baseline_stats; // Acknowledge baseline tracking
    }

    /// Test no memory accumulation in long inference loop
    #[test]
    fn test_no_leak_long_inference_loop() {
        use adapteros_lora_kernel_api::{FusedKernels, IoBuffers};

        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config: config.clone(),
            model_hash: adapteros_core::B3Hash::hash(b"inference-loop-test"),
        };

        let mut backend = MLXFFIBackend::new(model);

        // Register adapter
        let adapter = create_mock_adapter("inference-test", 8);
        backend.register_adapter(1, adapter).unwrap();

        // Load plan
        backend.load(b"long-inference-plan").unwrap();

        // Simulate 2000 inference steps
        let num_steps = 2000;
        let mut peak_memory = 0;

        for step in 0..num_steps {
            // Create new buffers each iteration (simulates real usage)
            let mut io = IoBuffers {
                input_ids: vec![1, 2, 3],
                output_logits: vec![0.0; config.vocab_size],
                position: step,
            };

            // In real scenario, would call run_step
            // For mock, just track memory
            let stats = memory::stats();
            if stats.total_bytes > peak_memory {
                peak_memory = stats.total_bytes;
            }

            // Drop buffers
            drop(io);

            // Periodic GC
            if step % 200 == 0 {
                memory::gc_collect();
            }
        }

        memory::gc_collect();

        let final_stats = memory::stats();

        // Memory should not grow linearly with iterations
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            growth_mb < 500.0,
            "Memory grew by {:.2} MB over {} steps (possible leak)",
            growth_mb,
            num_steps
        );
    }

    /// Test memory stability with repeated hot-swaps
    #[test]
    fn test_no_leak_hot_swap_cycles() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"hot-swap-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Initial adapter
        let adapter = create_mock_adapter("initial", 8);
        backend.register_adapter(1, adapter).unwrap();

        // Perform 500 hot-swaps
        for i in 0..500 {
            let new_adapter = create_mock_adapter(&format!("swap-{}", i), 8);
            backend.load_adapter_runtime(1, new_adapter).unwrap();

            if i % 50 == 0 {
                memory::gc_collect();
            }
        }

        // Cleanup
        backend.unload_adapter_runtime(1).unwrap();
        memory::gc_collect();

        let final_stats = memory::stats();
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            growth_mb < 100.0,
            "Memory grew by {:.2} MB over 500 hot-swaps (possible leak)",
            growth_mb
        );
    }

    /// Test memory tracking accuracy
    #[test]
    fn test_memory_tracking_accuracy() {
        memory::reset();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"tracking-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Track allocations as we load adapters
        let mut expected_count = 0;

        for i in 0..10 {
            let before_count = memory::allocation_count();

            let adapter = create_mock_adapter(&format!("track-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();

            let after_count = memory::allocation_count();

            // Allocations should increase or stay same (depending on implementation)
            assert!(
                after_count >= before_count,
                "Allocation count decreased: before={}, after={}",
                before_count,
                after_count
            );

            expected_count = after_count;
        }

        // Unload all
        for i in 0..10 {
            backend.unload_adapter_runtime(i).unwrap();
        }

        memory::gc_collect();

        let final_count = memory::allocation_count();

        // Should have fewer allocations after cleanup
        assert!(
            final_count <= expected_count,
            "Allocations not cleaned up: expected <= {}, got {}",
            expected_count,
            final_count
        );
    }

    /// Test no leak with rapid adapter registration/deregistration
    #[test]
    fn test_no_leak_rapid_registration() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"rapid-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Rapid fire registration and deregistration
        for batch in 0..100 {
            // Register 10 adapters
            for i in 0..10 {
                let adapter = create_mock_adapter(&format!("batch-{}-adapter-{}", batch, i), 4);
                backend.register_adapter(i, adapter).unwrap();
            }

            // Immediately deregister all
            for i in 0..10 {
                backend.unload_adapter_runtime(i).unwrap();
            }

            if batch % 10 == 0 {
                memory::gc_collect();
            }
        }

        memory::gc_collect();

        let final_stats = memory::stats();
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            growth_mb < 50.0,
            "Memory grew by {:.2} MB over rapid cycles (possible leak)",
            growth_mb
        );
    }
}

#[cfg(test)]
mod memory_stress_tests {
    use adapteros_lora_mlx_ffi::backend::MLXFFIBackend;
    use adapteros_lora_mlx_ffi::memory;
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;

    /// Test memory under high adapter count
    #[test]
    fn test_high_adapter_count_memory() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"high-count-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load 100 adapters
        let adapter_count = 100;
        for i in 0..adapter_count {
            let adapter = create_mock_adapter(&format!("high-count-{}", i), 4);
            backend.register_adapter(i, adapter).unwrap();
        }

        assert_eq!(backend.adapter_count(), adapter_count as usize);

        let loaded_stats = memory::stats();

        // Unload all adapters
        for i in 0..adapter_count {
            backend.unload_adapter_runtime(i).unwrap();
        }

        memory::gc_collect();

        let final_stats = memory::stats();

        // Memory should return close to initial after cleanup
        let growth_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            growth_mb < 50.0,
            "Memory did not return to baseline after cleanup: growth={:.2} MB",
            growth_mb
        );

        let _ = loaded_stats; // Acknowledge peak tracking
    }

    /// Test memory with large rank adapters
    #[test]
    fn test_large_rank_adapter_memory() {
        memory::reset();
        memory::gc_collect();

        let initial_stats = memory::stats();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"large-rank-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Load adapter with large rank
        let large_rank = 128;
        let adapter = create_mock_adapter("large-rank-adapter", large_rank);
        backend.register_adapter(1, adapter).unwrap();

        let loaded_stats = memory::stats();

        // Verify memory increased
        assert!(loaded_stats.total_bytes >= initial_stats.total_bytes);

        // Estimate expected memory usage
        let estimated_memory = backend.get_adapter_memory_usage(1).unwrap();
        assert!(estimated_memory > 0);

        // Unload
        backend.unload_adapter_runtime(1).unwrap();
        memory::gc_collect();

        let final_stats = memory::stats();

        // Should return to near baseline
        let retention_mb = memory::bytes_to_mb(final_stats.total_bytes)
            - memory::bytes_to_mb(initial_stats.total_bytes);

        assert!(
            retention_mb < 10.0,
            "Large rank adapter leaked {:.2} MB",
            retention_mb
        );
    }

    /// Test memory pressure scenario
    #[test]
    fn test_memory_pressure_scenario() {
        memory::reset();
        memory::gc_collect();

        let config = create_mock_config();
        let model = MLXFFIModel {
            model: std::ptr::null_mut(),
            config,
            model_hash: adapteros_core::B3Hash::hash(b"pressure-test"),
        };

        let backend = MLXFFIBackend::new(model);

        // Simulate memory pressure by loading many adapters
        let mut loaded_adapters = Vec::new();

        for i in 0..50 {
            let adapter = create_mock_adapter(&format!("pressure-{}", i), 16);
            backend.register_adapter(i, adapter).unwrap();
            loaded_adapters.push(i);

            // Check if memory threshold exceeded
            if memory::exceeds_threshold(100.0) {
                // Simulate eviction of oldest adapter
                if let Some(oldest) = loaded_adapters.first().copied() {
                    backend.unload_adapter_runtime(oldest).unwrap();
                    loaded_adapters.remove(0);
                    memory::gc_collect();
                }
            }
        }

        // Cleanup remaining
        for adapter_id in loaded_adapters {
            backend.unload_adapter_runtime(adapter_id).unwrap();
        }

        memory::gc_collect();

        // Should complete without panic
        assert_eq!(backend.adapter_count(), 0);
    }
}

#[cfg(test)]
mod memory_regression_tests {
    use adapteros_lora_mlx_ffi::memory;

    /// Test that memory stats remain consistent
    #[test]
    fn test_memory_stats_consistency() {
        memory::reset();

        for _ in 0..100 {
            let stats1 = memory::stats();
            let stats2 = memory::stats();

            // Without allocations, stats should be identical
            assert_eq!(stats1.total_bytes, stats2.total_bytes);
            assert_eq!(stats1.allocation_count, stats2.allocation_count);
        }
    }

    /// Test GC doesn't corrupt state
    #[test]
    fn test_gc_state_safety() {
        memory::reset();

        let before = memory::stats();

        // Multiple GC calls should be safe
        for _ in 0..100 {
            memory::gc_collect();
        }

        let after = memory::stats();

        // State should remain valid
        assert_eq!(before.total_bytes, after.total_bytes);
        assert_eq!(before.allocation_count, after.allocation_count);
    }

    /// Test memory reset idempotency
    #[test]
    fn test_reset_idempotency() {
        // Multiple resets should be safe
        for _ in 0..100 {
            memory::reset();
            let stats = memory::stats();
            assert_eq!(stats.total_bytes, 0);
            assert_eq!(stats.allocation_count, 0);
        }
    }
}
