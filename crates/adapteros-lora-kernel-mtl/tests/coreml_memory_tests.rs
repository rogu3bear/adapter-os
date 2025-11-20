//! CoreML memory management integration tests
//!
//! Tests for ANE-aware memory management:
//! - Buffer pooling and reuse
//! - Memory pressure detection and handling
//! - Transfer bandwidth tracking
//! - Integration with CoreML backend

#[cfg(all(feature = "coreml-backend", target_os = "macos"))]
mod coreml_memory_tests {
    use adapteros_lora_kernel_mtl::coreml_memory::{
        ANEMemoryStats, BufferDataType, BufferLocation, CoreMLMemoryConfig, CoreMLMemoryManager,
        PressureAction,
    };
    use std::time::Duration;

    #[test]
    fn test_memory_manager_initialization() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config);
        assert!(manager.is_ok(), "Memory manager should initialize");

        let mgr = manager.unwrap();
        let stats = mgr.stats();
        assert!(stats.total_bytes > 0, "Should detect ANE memory");
        assert_eq!(stats.allocated_bytes, 0, "No allocations initially");
    }

    #[test]
    fn test_buffer_acquisition_and_release() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Acquire buffer for typical tensor (1, 3, 224, 224)
        let shape = vec![1, 3, 224, 224];
        let buffer_id = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Check statistics
        let stats = manager.stats();
        assert_eq!(stats.allocation_count, 1);
        assert!(stats.allocated_bytes > 0);

        // Release buffer
        manager.release_buffer(buffer_id).unwrap();

        // Check pool statistics
        let pool_stats = manager.pool_stats();
        assert_eq!(pool_stats.pooled_buffers, 1);
        assert_eq!(pool_stats.active_buffers, 0);
    }

    #[test]
    fn test_buffer_reuse_from_pool() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![1, 3, 224, 224];

        // First allocation
        let buffer_id1 = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();
        let stats1 = manager.stats();
        let total_allocs1 = stats1.total_allocations;

        manager.release_buffer(buffer_id1).unwrap();

        // Second allocation (should reuse from pool)
        let buffer_id2 = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();
        let stats2 = manager.stats();
        let total_allocs2 = stats2.total_allocations;

        // Should NOT have allocated a new buffer
        assert_eq!(
            total_allocs2, total_allocs1,
            "Should reuse buffer from pool"
        );

        manager.release_buffer(buffer_id2).unwrap();
    }

    #[test]
    fn test_buffer_pinning() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![1, 3, 224, 224];
        let buffer_id = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Pin buffer
        manager.pin_buffer(buffer_id);

        // Try to release (should stay active due to pin)
        manager.release_buffer(buffer_id).unwrap();

        let pool_stats = manager.pool_stats();
        assert_eq!(pool_stats.active_buffers, 1, "Pinned buffer stays active");
        assert_eq!(pool_stats.pooled_buffers, 0, "Pinned buffer not pooled");

        // Unpin and release
        manager.unpin_buffer(buffer_id);
        manager.release_buffer(buffer_id).unwrap();

        let pool_stats2 = manager.pool_stats();
        assert_eq!(pool_stats2.pooled_buffers, 1, "Unpinned buffer can be pooled");
    }

    #[test]
    fn test_memory_pressure_detection() {
        let mut config = CoreMLMemoryConfig::default();
        config.pressure_threshold = 0.1; // Low threshold for testing
        config.ane_memory_limit = 10 * 1024 * 1024; // 10 MB limit

        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Allocate multiple large buffers to trigger pressure
        let shape = vec![512, 512, 4]; // 4 MB per buffer
        let mut buffers = Vec::new();

        for _ in 0..3 {
            let buf_id = manager
                .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
                .unwrap();
            buffers.push(buf_id);
        }

        let stats = manager.stats();
        assert!(
            stats.has_pressure(0.1),
            "Should have memory pressure at 10% threshold"
        );

        // Release buffers
        for buf_id in buffers {
            manager.release_buffer(buf_id).unwrap();
        }
    }

    #[test]
    fn test_memory_pressure_eviction() {
        let mut config = CoreMLMemoryConfig::default();
        config.pressure_threshold = 0.5;
        config.ane_memory_limit = 20 * 1024 * 1024; // 20 MB limit
        config.max_pool_size = 10;

        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Fill pool with buffers
        let shape = vec![256, 256, 4]; // 1 MB per buffer
        for _ in 0..15 {
            let buf_id = manager
                .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
                .unwrap();
            manager.release_buffer(buf_id).unwrap();
        }

        let pool_stats_before = manager.pool_stats();
        assert_eq!(
            pool_stats_before.pooled_buffers, 10,
            "Pool should be at max size"
        );

        // Trigger pressure handling
        let pressure_result = manager.check_memory_pressure();
        assert!(
            pressure_result.is_ok(),
            "Pressure handling should succeed"
        );

        // Check pressure events
        let events = manager.pressure_events(5);
        if !events.is_empty() {
            let last_event = &events[0];
            assert!(
                last_event.bytes_freed > 0,
                "Should have freed memory during pressure"
            );
        }
    }

    #[test]
    fn test_transfer_statistics_tracking() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Simulate CPU → ANE transfers
        manager.record_cpu_to_ane_transfer(1024 * 1024, Duration::from_micros(100)); // 1 MB in 100μs
        manager.record_cpu_to_ane_transfer(2048 * 1024, Duration::from_micros(200)); // 2 MB in 200μs

        // Simulate ANE → CPU transfers
        manager.record_ane_to_cpu_transfer(512 * 1024, Duration::from_micros(50)); // 512 KB in 50μs

        let transfer_stats = manager.transfer_stats();
        assert_eq!(transfer_stats.cpu_to_ane_count, 2);
        assert_eq!(transfer_stats.ane_to_cpu_count, 1);
        assert_eq!(transfer_stats.cpu_to_ane_bytes, 3 * 1024 * 1024); // 3 MB total
        assert_eq!(transfer_stats.ane_to_cpu_bytes, 512 * 1024); // 512 KB
        assert!(transfer_stats.avg_bandwidth_gbps > 0.0);
    }

    #[test]
    fn test_different_buffer_data_types() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![1, 3, 224, 224];

        // Float32 buffer
        let buf_f32 = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Float16 buffer (half size)
        let buf_f16 = manager
            .acquire_buffer(&shape, BufferDataType::Float16, BufferLocation::ANE)
            .unwrap();

        // Int8 buffer (quarter size)
        let buf_i8 = manager
            .acquire_buffer(&shape, BufferDataType::Int8, BufferLocation::ANE)
            .unwrap();

        let stats = manager.stats();
        assert_eq!(stats.allocation_count, 3);

        // Release all
        manager.release_buffer(buf_f32).unwrap();
        manager.release_buffer(buf_f16).unwrap();
        manager.release_buffer(buf_i8).unwrap();

        let pool_stats = manager.pool_stats();
        assert_eq!(pool_stats.pooled_buffers, 3);
    }

    #[test]
    fn test_buffer_location_variants() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![100, 100];

        // CPU buffer
        let buf_cpu = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::CPU)
            .unwrap();

        // ANE buffer
        let buf_ane = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Unified buffer
        let buf_unified = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::Unified)
            .unwrap();

        manager.release_buffer(buf_cpu).unwrap();
        manager.release_buffer(buf_ane).unwrap();
        manager.release_buffer(buf_unified).unwrap();

        let pool_stats = manager.pool_stats();
        assert_eq!(pool_stats.pooled_buffers, 3);
    }

    #[test]
    fn test_pool_size_limits() {
        let mut config = CoreMLMemoryConfig::default();
        config.max_pool_size = 3; // Small pool for testing

        let manager = CoreMLMemoryManager::new(config).unwrap();
        let shape = vec![100, 100];

        // Acquire and release 5 buffers (exceeds pool size)
        for _ in 0..5 {
            let buf_id = manager
                .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
                .unwrap();
            manager.release_buffer(buf_id).unwrap();
        }

        let pool_stats = manager.pool_stats();
        assert_eq!(
            pool_stats.pooled_buffers, 3,
            "Pool should be capped at max_pool_size"
        );
    }

    #[test]
    fn test_ane_memory_stats_calculations() {
        let stats = ANEMemoryStats {
            total_bytes: 1024 * 1024 * 1024, // 1 GB
            allocated_bytes: 768 * 1024 * 1024, // 768 MB
            peak_allocated_bytes: 800 * 1024 * 1024,
            allocation_count: 10,
            total_allocations: 20,
            total_deallocations: 10,
            pressure_level: 0.75,
        };

        assert_eq!(stats.usage_percent(), 75.0);
        assert_eq!(stats.headroom_percent(), 25.0);
        assert!(stats.has_pressure(0.7));
        assert!(!stats.has_pressure(0.8));
    }

    #[test]
    fn test_buffer_size_validation() {
        let mut config = CoreMLMemoryConfig::default();
        config.max_buffer_size = 1024 * 1024; // 1 MB max

        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Try to allocate oversized buffer
        let large_shape = vec![1024, 1024, 4]; // 16 MB in Float32
        let result = manager.acquire_buffer(
            &large_shape,
            BufferDataType::Float32,
            BufferLocation::ANE,
        );

        assert!(result.is_err(), "Should reject oversized buffer");
    }

    #[test]
    fn test_clear_all_buffers() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![100, 100];

        // Allocate and pool multiple buffers
        for _ in 0..5 {
            let buf_id = manager
                .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
                .unwrap();
            manager.release_buffer(buf_id).unwrap();
        }

        let pool_stats_before = manager.pool_stats();
        assert_eq!(pool_stats_before.pooled_buffers, 5);

        // Clear all buffers
        manager.clear_all_buffers().unwrap();

        let pool_stats_after = manager.pool_stats();
        assert_eq!(pool_stats_after.pooled_buffers, 0);
        assert_eq!(pool_stats_after.active_buffers, 0);

        let stats = manager.stats();
        assert_eq!(stats.allocated_bytes, 0);
    }

    #[test]
    fn test_concurrent_buffer_operations() {
        use std::sync::Arc;
        use std::thread;

        let config = CoreMLMemoryConfig::default();
        let manager = Arc::new(CoreMLMemoryManager::new(config).unwrap());
        let shape = vec![64, 64];

        let mut handles = vec![];

        // Spawn multiple threads acquiring and releasing buffers
        for _ in 0..4 {
            let mgr = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                for _ in 0..10 {
                    let buf_id = mgr
                        .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
                        .unwrap();
                    thread::sleep(std::time::Duration::from_micros(10));
                    mgr.release_buffer(buf_id).unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let stats = manager.stats();
        assert!(stats.total_allocations >= 40, "All threads should allocate");
    }
}

#[cfg(not(all(feature = "coreml-backend", target_os = "macos")))]
mod coreml_memory_tests {
    #[test]
    fn test_coreml_not_available() {
        println!("CoreML backend tests skipped (requires macos + coreml-backend feature)");
    }
}
