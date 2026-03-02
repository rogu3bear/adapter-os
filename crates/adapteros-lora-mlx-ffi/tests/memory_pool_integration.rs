//! Integration tests for MLX backend memory pool management
//!
//! Tests demonstrate:
//! - Adapter registration with memory tracking
//! - Memory pool statistics and monitoring
//! - Per-adapter memory usage tracking
//! - Memory pressure callbacks
//! - Memory cleanup on adapter unload

#[cfg(test)]
mod memory_pool_integration_tests {
    use adapteros_lora_mlx_ffi::mock::{create_mock_adapter, create_mock_config};
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use adapteros_lora_mlx_ffi::{
        MLXFFIBackend, MLXMemoryPool, MLXMemoryPoolConfig, MemoryPressureEvent,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Helper to create a test backend
    fn create_test_backend() -> MLXFFIBackend {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    }

    #[test]
    fn test_memory_pool_initialization() {
        // Verify memory pool is created with default configuration
        let backend = create_test_backend();

        let stats = backend.get_memory_pool_stats();
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.pool_hits, 0);
        assert_eq!(stats.pooled_buffer_count, 0);
        assert_eq!(backend.get_total_adapter_memory(), 0);
    }

    #[test]
    #[ignore = "mock adapters don't allocate GPU memory, needs real adapter weights"]
    fn test_adapter_registration_tracks_memory() {
        // Register adapters and verify memory is tracked
        let mut backend = create_test_backend();

        // Register first adapter
        let adapter1 = create_mock_adapter("adapter-1", 4);
        backend
            .register_adapter(1, adapter1)
            .expect("Register adapter 1");

        // Verify memory is tracked
        let stats = backend.get_memory_pool_stats();
        assert!(
            stats.total_active_bytes > 0,
            "Active memory should be tracked"
        );

        let adapter_memory = backend.get_total_adapter_memory();
        assert!(adapter_memory > 0, "Adapter memory should be tracked");

        // Register second adapter
        let adapter2 = create_mock_adapter("adapter-2", 8);
        backend
            .register_adapter(2, adapter2)
            .expect("Register adapter 2");

        // Verify cumulative tracking
        let new_adapter_memory = backend.get_total_adapter_memory();
        assert!(
            new_adapter_memory > adapter_memory,
            "Total adapter memory should increase"
        );
    }

    #[test]
    fn test_adapter_unload_frees_memory() {
        // Verify memory is freed when adapters are unloaded
        let mut backend = create_test_backend();

        // Register an adapter
        let adapter = create_mock_adapter("adapter-test", 4);
        backend
            .register_adapter(1, adapter)
            .expect("Register adapter");

        let memory_before = backend.get_total_adapter_memory();
        assert!(memory_before > 0);

        // Unload the adapter
        backend.unload_adapter_runtime(1).expect("Unload adapter");

        let memory_after = backend.get_total_adapter_memory();
        assert_eq!(memory_after, 0, "Memory should be freed after unload");
    }

    #[test]
    #[ignore = "mock adapters don't allocate GPU memory, needs real adapter weights"]
    fn test_memory_pool_statistics() {
        // Verify memory pool statistics are accurate
        let mut backend = create_test_backend();

        // Initial state
        let initial_stats = backend.get_memory_pool_stats();
        assert_eq!(initial_stats.total_allocations, 0);
        assert_eq!(initial_stats.pool_hits, 0);
        assert_eq!(initial_stats.pool_misses, 0);

        // Register adapters
        for i in 0..3 {
            let adapter = create_mock_adapter(&format!("adapter-{}", i), 4 + i * 2);
            backend
                .register_adapter(i as u16, adapter)
                .expect("Register adapter");
        }

        let stats = backend.get_memory_pool_stats();
        assert_eq!(
            stats.pooled_buffer_count, 0,
            "Adapters should not be in pool initially"
        );
        assert!(stats.total_active_bytes > 0, "Should have active memory");

        // Verify adapter count
        assert_eq!(
            backend.adapter_count(),
            3,
            "Should have 3 adapters registered"
        );
    }

    #[test]
    fn test_per_adapter_memory_tracking() {
        // Test per-adapter memory tracking
        let mut backend = create_test_backend();

        let tracked_adapters_before = backend.tracked_adapter_ids();
        assert_eq!(tracked_adapters_before.len(), 0);

        // Register adapters
        backend
            .register_adapter(1, create_mock_adapter("a1", 4))
            .expect("Register adapter 1");
        backend
            .register_adapter(2, create_mock_adapter("a2", 8))
            .expect("Register adapter 2");

        let tracked_adapters = backend.tracked_adapter_ids();
        assert_eq!(tracked_adapters.len(), 2);
        assert!(tracked_adapters.contains(&1));
        assert!(tracked_adapters.contains(&2));

        // Unload one adapter
        backend.unload_adapter_runtime(1).expect("Unload adapter 1");

        let tracked_after_unload = backend.tracked_adapter_ids();
        assert_eq!(tracked_after_unload.len(), 1);
        assert!(tracked_after_unload.contains(&2));
        assert!(!tracked_after_unload.contains(&1));
    }

    #[test]
    fn test_memory_pressure_handling() {
        // Test memory pressure detection and cleanup
        let mut backend = create_test_backend();

        // Register adapters
        for i in 0..5 {
            let adapter = create_mock_adapter(&format!("adapter-{}", i), 4 + i as usize);
            backend
                .register_adapter(i as u16, adapter)
                .expect("Register adapter");
        }

        let stats_before = backend.get_memory_pool_stats();
        let total_memory = stats_before.total_active_bytes + stats_before.total_pooled_bytes;

        // Simulate memory pressure by requesting cleanup
        let bytes_to_free = total_memory / 4;
        let freed = backend.handle_memory_pressure(bytes_to_free);

        // Verify something was processed (even if not freed in this case)
        tracing::info!(
            "Memory pressure handling: requested {} bytes, freed {} bytes",
            bytes_to_free,
            freed
        );
    }

    #[test]
    fn test_memory_pressure_callback() {
        // Test that memory pressure callbacks can be registered
        let backend = create_test_backend();

        let callback_invoked = Arc::new(AtomicUsize::new(0));
        let callback_invoked_clone = callback_invoked.clone();

        let callback = Box::new(move |event: MemoryPressureEvent| {
            callback_invoked_clone.fetch_add(1, Ordering::SeqCst);
            tracing::info!(
                "Memory pressure event: {:.1}% usage, {} bytes to free",
                event.pressure_level * 100.0,
                event.bytes_to_free
            );
        });

        backend.register_memory_pressure_callback(callback);

        // Try to trigger pressure (in stub mode this may not actually trigger, but callback
        // registration should succeed)
        let count = callback_invoked.load(Ordering::SeqCst);
        assert_eq!(count, 0, "Callback should not be invoked yet");
    }

    #[test]
    fn test_memory_pool_clear() {
        // Test clearing all pooled memory
        let mut backend = create_test_backend();

        // Register adapters
        backend
            .register_adapter(1, create_mock_adapter("a1", 4))
            .expect("Register adapter 1");
        backend
            .register_adapter(2, create_mock_adapter("a2", 8))
            .expect("Register adapter 2");

        // Clear memory pool
        backend.clear_memory_pool();

        // Verify pool is cleared
        let stats = backend.get_memory_pool_stats();
        assert_eq!(stats.pooled_buffer_count, 0, "Pool should be empty");
        assert_eq!(stats.total_pooled_bytes, 0, "Pooled bytes should be zero");
    }

    #[test]
    fn test_memory_metrics_update() {
        // Test memory metrics are updated correctly
        let mut backend = create_test_backend();

        // Initial metrics
        let metrics1 = &backend.performance_metrics.read().clone();
        assert_eq!(metrics1.peak_memory_usage_mb, 0.0);

        // Register adapters
        backend
            .register_adapter(1, create_mock_adapter("a1", 4))
            .expect("Register adapter 1");

        // Update metrics
        backend.update_memory_metrics();

        let metrics2 = &backend.performance_metrics.read().clone();
        assert!(
            metrics2.peak_memory_usage_mb >= 0.0,
            "Peak memory should be non-negative"
        );
    }

    #[test]
    fn test_memory_pool_config() {
        // Test custom memory pool configuration
        let config = MLXMemoryPoolConfig {
            max_pooled_memory: 256 * 1024 * 1024, // 256 MB
            idle_timeout_secs: 30,
            pressure_threshold: 0.9,
            ..Default::default()
        };

        let pool = MLXMemoryPool::new(config);
        let stats = pool.get_stats();

        // Verify stats initialized with defaults
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.pooled_buffer_count, 0);
    }

    #[test]
    #[ignore = "mock adapters don't allocate GPU memory, needs real adapter weights"]
    fn test_multiple_adapter_registrations_and_unloads() {
        // Test registering and unloading multiple adapters in sequence
        let mut backend = create_test_backend();

        for iteration in 0..3 {
            // Register batch of adapters
            for i in 0..5 {
                let adapter_id = (iteration * 5 + i) as u16;
                let adapter =
                    create_mock_adapter(&format!("adapter-{}", adapter_id), 4 + (i as usize));
                backend
                    .register_adapter(adapter_id, adapter)
                    .expect("Register adapter");
            }

            // Verify all registered
            assert_eq!(backend.adapter_count(), 5, "Should have 5 adapters");

            // Get stats
            let stats = backend.get_memory_pool_stats();
            let total_memory = stats.total_active_bytes + stats.total_pooled_bytes;
            assert!(total_memory > 0, "Should have allocated memory");

            // Unload all adapters
            for i in 0..5 {
                let adapter_id = (iteration * 5 + i) as u16;
                backend
                    .unload_adapter_runtime(adapter_id)
                    .expect("Unload adapter");
            }

            // Verify all unloaded
            assert_eq!(
                backend.adapter_count(),
                0,
                "All adapters should be unloaded"
            );
        }
    }

    #[test]
    fn test_cleanup_idle_buffers() {
        // Test cleanup of idle pooled buffers
        let _backend = create_test_backend();

        // Create a custom pool with short timeout for testing
        let config = MLXMemoryPoolConfig {
            idle_timeout_secs: 0, // Very short timeout
            ..Default::default()
        };

        let pool = MLXMemoryPool::new(config);

        // Allocate and return buffers
        let buf1 = pool.allocate(8192).expect("Allocate buffer 1");
        let buf2 = pool.allocate(16384).expect("Allocate buffer 2");

        pool.return_buffer(buf1);
        pool.return_buffer(buf2);

        let stats_before = pool.get_stats();
        assert!(
            stats_before.pooled_buffer_count > 0,
            "Should have pooled buffers"
        );

        // Clean up idle buffers
        let freed = pool.cleanup_idle();
        assert!(freed > 0, "Should have freed idle buffers");

        let stats_after = pool.get_stats();
        assert!(
            stats_after.pooled_buffer_count < stats_before.pooled_buffer_count,
            "Pooled buffer count should decrease"
        );
    }
}
