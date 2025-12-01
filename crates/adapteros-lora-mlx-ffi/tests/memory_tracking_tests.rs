/// Memory tracking tests for MLX backend
///
/// These tests verify that memory allocation and deallocation are properly
/// tracked through the FFI interface.

#[cfg(test)]
mod memory_tracking_tests {
    use adapteros_lora_mlx_ffi::memory;

    #[test]
    fn test_memory_initial_state() {
        // Reset to clean state
        memory::reset();

        // Initially should be zero
        assert_eq!(memory::memory_usage(), 0);
        assert_eq!(memory::allocation_count(), 0);
    }

    #[test]
    fn test_memory_stats_structure() {
        memory::reset();

        let stats = memory::stats();
        assert_eq!(stats.total_bytes, 0);
        assert_eq!(stats.allocation_count, 0);
    }

    #[test]
    fn test_bytes_to_mb_conversion() {
        assert_eq!(memory::bytes_to_mb(0), 0.0);
        assert_eq!(memory::bytes_to_mb(1024 * 1024), 1.0);
        assert_eq!(memory::bytes_to_mb(1024 * 1024 * 512), 512.0);
    }

    #[test]
    fn test_format_stats() {
        let stats = memory::MemoryStats {
            total_bytes: 1024 * 1024,
            allocation_count: 5,
        };

        let formatted = memory::format_stats(&stats);
        assert!(formatted.contains("1.00 MB"));
        assert!(formatted.contains("5 allocations"));
    }

    #[test]
    fn test_exceeds_threshold_logic() {
        memory::reset();

        // Should not exceed low threshold on empty state
        assert!(!memory::exceeds_threshold(0.01));

        // Reset and check high threshold
        assert!(!memory::exceeds_threshold(10000.0));
    }

    #[test]
    fn test_gc_collect_doesnt_crash() {
        // GC should not panic even if no allocations
        memory::reset();
        memory::gc_collect();

        // State should remain unchanged
        assert_eq!(memory::allocation_count(), 0);
    }

    #[test]
    fn test_memory_reset_clears_state() {
        memory::reset();
        assert_eq!(memory::memory_usage(), 0);

        memory::reset(); // Reset again
        assert_eq!(memory::memory_usage(), 0);
        assert_eq!(memory::allocation_count(), 0);
    }

    #[test]
    fn test_memory_stats_tuple() {
        memory::reset();

        let (total, count) = memory::memory_stats();
        assert_eq!(total, 0);
        assert_eq!(count, 0);
    }
}

#[cfg(test)]
mod memory_api_interface_tests {
    use adapteros_lora_mlx_ffi::memory;

    /// Test the memory module public API surface
    #[test]
    fn test_public_api_functions_exist() {
        // These should all compile and be callable without panicking
        let _ = memory::memory_usage();
        let _ = memory::allocation_count();
        let (_, _) = memory::memory_stats();
        let _ = memory::stats();
        let _ = memory::bytes_to_mb(1024);

        let stats = memory::MemoryStats {
            total_bytes: 1024,
            allocation_count: 1,
        };
        let _ = memory::format_stats(&stats);
        let _ = memory::exceeds_threshold(100.0);

        memory::reset();
        memory::gc_collect();
    }

    #[test]
    fn test_memory_stats_struct_fields() {
        let stats = memory::MemoryStats {
            total_bytes: 2048,
            allocation_count: 3,
        };

        assert_eq!(stats.total_bytes, 2048);
        assert_eq!(stats.allocation_count, 3);
    }

    #[test]
    fn test_memory_stats_can_be_cloned() {
        let stats1 = memory::MemoryStats {
            total_bytes: 1024,
            allocation_count: 1,
        };

        let stats2 = stats1.clone();
        assert_eq!(stats1.total_bytes, stats2.total_bytes);
        assert_eq!(stats1.allocation_count, stats2.allocation_count);
    }

    #[test]
    fn test_memory_stats_can_be_copied() {
        let stats1 = memory::MemoryStats {
            total_bytes: 512,
            allocation_count: 2,
        };

        let stats2 = stats1;
        assert_eq!(stats1.total_bytes, stats2.total_bytes);
        assert_eq!(stats1.allocation_count, stats2.allocation_count);
    }

    #[test]
    fn test_memory_stats_debug_output() {
        let stats = memory::MemoryStats {
            total_bytes: 1024,
            allocation_count: 1,
        };

        let debug_string = format!("{:?}", stats);
        assert!(debug_string.contains("1024"));
        assert!(debug_string.contains("1"));
    }
}

#[cfg(test)]
mod memory_lifecycle_scenario_tests {
    use adapteros_lora_mlx_ffi::memory;

    /// Simulate the lifecycle of a memory-monitored operation
    #[test]
    fn test_memory_checkpoint_scenario() {
        // Initial checkpoint
        memory::reset();
        let before = memory::stats();
        assert_eq!(before.total_bytes, 0);

        // Simulate some work (would normally allocate)
        let work_iterations = 5;
        for _ in 0..work_iterations {
            // In real scenario, this would be array allocations
            memory::gc_collect();
        }

        // Final checkpoint
        let after = memory::stats();

        // Memory should still be tracked
        let _ = memory::format_stats(&after);
    }

    /// Simulate memory pressure detection
    #[test]
    fn test_memory_pressure_detection() {
        memory::reset();

        // Define thresholds
        let critical_threshold_mb = 4096.0; // 4GB
        let normal_threshold_mb = 1024.0; // 1GB

        // Check thresholds at current (empty) state
        let is_normal = !memory::exceeds_threshold(normal_threshold_mb);
        let is_critical = memory::exceeds_threshold(critical_threshold_mb);

        // Should be safe at all thresholds when empty
        assert!(is_normal);
        assert!(!is_critical);
    }

    /// Simulate periodic memory monitoring
    #[test]
    fn test_periodic_memory_monitoring() {
        memory::reset();

        // Simulate monitoring loop
        let num_checkpoints = 10;
        let mut max_bytes = 0;
        let mut max_allocations = 0;

        for _ in 0..num_checkpoints {
            let stats = memory::stats();

            if stats.total_bytes > max_bytes {
                max_bytes = stats.total_bytes;
            }
            if stats.allocation_count > max_allocations {
                max_allocations = stats.allocation_count;
            }

            memory::gc_collect();
        }

        // Verify peak tracking
        let final_stats = memory::stats();
        assert!(final_stats.total_bytes <= max_bytes);
        assert!(final_stats.allocation_count <= max_allocations);
    }
}

#[cfg(test)]
mod memory_boundary_tests {
    use adapteros_lora_mlx_ffi::memory;

    #[test]
    fn test_large_memory_values() {
        // Test with realistically large values
        let large_bytes = 8 * 1024 * 1024 * 1024u64 as usize; // 8GB
        let mb = memory::bytes_to_mb(large_bytes);

        assert!((mb - 8192.0).abs() < 0.01); // Should be close to 8192 MB
    }

    #[test]
    fn test_small_memory_values() {
        // Test with small values
        assert_eq!(memory::bytes_to_mb(1), 0.00000095367431640625); // Exact 1 byte

        let one_kb = 1024;
        let mb = memory::bytes_to_mb(one_kb);
        assert!((mb - 0.0009765625).abs() < 0.00001);
    }

    #[test]
    fn test_zero_memory() {
        assert_eq!(memory::bytes_to_mb(0), 0.0);
        assert!(!memory::exceeds_threshold(-1.0)); // Negative threshold never exceeded
    }

    #[test]
    fn test_extreme_thresholds() {
        memory::reset();

        // Very large threshold should never be exceeded
        assert!(!memory::exceeds_threshold(f32::MAX));

        // Negative threshold behavior
        let is_exceeded = memory::exceeds_threshold(-1.0);
        // -1.0 < any non-negative number of MB, so should be exceeded
        assert!(is_exceeded);
    }
}
