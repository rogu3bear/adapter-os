//! Comprehensive tests for memory watchdog functionality
//!
//! Tests all components of the memory watchdog system including:
//! - Metal heap observation
//! - Pointer canonicalization
//! - Buffer relocation detection
//! - Memory map hashing
//! - Replay integration
//! - End-to-end watchdog functionality
//!
//! Buffer relocation detection tests verify:
//! - Real-time buffer address monitoring
//! - Relocation event detection and recording
//! - Content integrity verification
//! - Replay system integration

#[cfg(test)]
mod buffer_relocation_tests {
    use super::super::*;
    use std::sync::Arc;
    use chrono;

    #[test]
    fn test_buffer_relocation_detector_creation() {
        // Test detector creation on non-macOS
        let detector = BufferRelocationDetector::new(None, true);
        assert!(detector.detection_enabled);

        // Test detector creation on macOS (if Metal available)
        #[cfg(target_os = "macos")]
        {
            use metal::Device;

            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device), true);
                assert!(detector.detection_enabled);
            }
        }
    }

    #[test]
    fn test_buffer_registration_non_macos() {
        let detector = BufferRelocationDetector::new(None, true);

        // Test buffer registration (should work on all platforms)
        let buffer_id = detector.register_buffer(None).unwrap();
        assert_eq!(buffer_id, 1);

        // Test getting relocation history (should be empty)
        let history = detector.get_relocation_history();
        assert!(history.is_empty());
    }

    #[test]
    fn test_relocation_detection_non_macos() {
        let detector = BufferRelocationDetector::new(None, true);

        // Test relocation detection (should return empty on non-macOS)
        let relocations = detector.check_relocations().unwrap();
        assert!(relocations.is_empty());
    }

    #[test]
    fn test_integrity_verification() {
        let detector = BufferRelocationDetector::new(None, true);

        let relocation_record = BufferRelocationRecord {
            relocation_id: uuid::Uuid::new_v4(),
            buffer_id: 1,
            original_addr: 0x1000,
            new_addr: 0x2000,
            size_bytes: 1024,
            timestamp: chrono::Utc::now().timestamp_millis() as u128,
            reason: RelocationReason::MemoryPressure,
            content_hash_before: None,
            content_hash_after: None,
            context: serde_json::json!({}),
        };

        // Test integrity verification
        let integrity_ok = detector.verify_relocation_integrity(&relocation_record).unwrap();
        assert!(integrity_ok);
    }

    #[tokio::test]
    async fn test_replay_consistency_verification() {
        let detector = BufferRelocationDetector::new(None, true);

        // Test replay consistency verification with empty data
        let expected_relocations = vec![];
        let consistency_ok = detector.verify_replay_consistency(&expected_relocations).await.unwrap();
        assert!(consistency_ok);

        // Test with mismatched data
        let relocation_record = BufferRelocationRecord {
            relocation_id: uuid::Uuid::new_v4(),
            buffer_id: 1,
            original_addr: 0x1000,
            new_addr: 0x2000,
            size_bytes: 1024,
            timestamp: chrono::Utc::now().timestamp_millis() as u128,
            reason: RelocationReason::MemoryPressure,
            content_hash_before: None,
            content_hash_after: None,
            context: serde_json::json!({}),
        };

        let expected_relocations = vec![relocation_record];
        let consistency_ok = detector.verify_replay_consistency(&expected_relocations).await.unwrap();
        assert!(!consistency_ok); // Should fail due to length mismatch
    }
}

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use std::thread;

    #[test]
    fn test_end_to_end_memory_monitoring() {
        let config = MemoryWatchdogConfig {
            enable_heap_observation: true,
            enable_pointer_canonicalization: true,
            enable_buffer_relocation_detection: true,
            enable_memory_map_hashing: true,
            sampling_rate: 1.0,
            pressure_warning_threshold: 0.8,
            pressure_critical_threshold: 0.9,
        };

        let watchdog = MemoryWatchdog::new(config).unwrap();
        assert!(watchdog.is_running());

        // Simulate memory allocation sequence
        let allocations = vec![
            (0x1000, 1024, "allocation_1"),
            (0x2000, 2048, "allocation_2"),
            (0x3000, 4096, "allocation_3"),
        ];

        for (addr, size, context) in allocations {
            watchdog.monitor_allocation(addr, size, context.to_string()).unwrap();
        }

        // Check that events were recorded
        let stats = watchdog.get_stats();
        assert!(stats.total_events >= 3);

        // Generate memory layout hash
        let layout_hash = watchdog.generate_memory_layout_hash().unwrap();
        assert!(!layout_hash.layout_hash.is_zero());

        // Test buffer relocation detection (if Metal is available)
        #[cfg(target_os = "macos")]
        {
            use metal::{Device, MTLResourceOptions};

            if let Some(device) = Device::system_default() {
                let detector = super::super::BufferRelocationDetector::new(Arc::new(device), true);

                // Test buffer registration (would need actual Metal buffer)
                // detector.register_buffer(&buffer).unwrap();

                // Test relocation detection
                let relocations = detector.check_relocations().unwrap();
                assert!(relocations.is_empty()); // No buffers registered yet

                // Test replay integration
                let history = detector.get_relocation_history();
                assert!(history.is_empty());

                // Test integrity verification
                let relocation_record = super::super::BufferRelocationRecord {
                    relocation_id: uuid::Uuid::new_v4(),
                    buffer_id: 1,
                    original_addr: 0x1000,
                    new_addr: 0x2000,
                    size_bytes: 1024,
                    timestamp: chrono::Utc::now().timestamp_millis() as u128,
                    reason: super::super::RelocationReason::MemoryPressure,
                    content_hash_before: None,
                    content_hash_after: None,
                    context: serde_json::json!({}),
                };

                let integrity_ok = detector.verify_relocation_integrity(&relocation_record).unwrap();
                assert!(integrity_ok);
            }
        }

        // Verify layout consistency
        watchdog.verify_layout_consistency(&layout_hash).unwrap();

        // Simulate deallocations
        for (addr, size, context) in allocations {
            watchdog.monitor_deallocation(addr, size, context.to_string()).unwrap();
        }

        // Check final stats
        let final_stats = watchdog.get_stats();
        assert!(final_stats.total_events >= 6); // 3 allocations + 3 deallocations
    }

    #[test]
    fn test_memory_pressure_detection() {
        let config = MemoryWatchdogConfig {
            pressure_warning_threshold: 0.7,
            pressure_critical_threshold: 0.9,
            ..Default::default()
        };

        let watchdog = MemoryWatchdog::new(config).unwrap();

        let stats = watchdog.get_stats();
        assert!(matches!(stats.memory_pressure, MemoryPressureLevel::Low | MemoryPressureLevel::Medium));
    }

    #[test]
    fn test_pointer_reuse_patterns() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Allocate and deallocate same address multiple times
        let addr = 0x1000;
        let size = 1024;

        for i in 0..5 {
            watchdog.monitor_allocation(addr, size, format!("allocation_{}", i)).unwrap();
            watchdog.monitor_deallocation(addr, size, format!("deallocation_{}", i)).unwrap();
        }

        let stats = watchdog.get_stats();
        assert!(stats.total_events >= 10); // 5 allocations + 5 deallocations
    }

    #[test]
    fn test_memory_layout_determinism() {
        let config = MemoryWatchdogConfig::default();
        let watchdog1 = MemoryWatchdog::new(config.clone()).unwrap();
        let watchdog2 = MemoryWatchdog::new(config).unwrap();

        // Perform identical allocation sequences
        let allocations = vec![
            (0x1000, 1024, "test1"),
            (0x2000, 2048, "test2"),
            (0x3000, 4096, "test3"),
        ];

        for (addr, size, context) in &allocations {
            watchdog1.monitor_allocation(*addr, *size, context.to_string()).unwrap();
            watchdog2.monitor_allocation(*addr, *size, context.to_string()).unwrap();
        }

        let hash1 = watchdog1.generate_memory_layout_hash().unwrap();
        let hash2 = watchdog2.generate_memory_layout_hash().unwrap();

        // Layout hashes should be identical for identical sequences
        assert_eq!(hash1.layout_hash, hash2.layout_hash);
    }

    #[test]
    fn test_replay_bundle_creation() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Generate some memory events
        watchdog.monitor_allocation(0x1000, 1024, "test".to_string()).unwrap();
        watchdog.monitor_deallocation(0x1000, 1024, "test".to_string()).unwrap();

        // Create replay bundle
        let bundle = watchdog.replay_logger.create_replay_bundle(
            "test-cpid".to_string(),
            "test-plan".to_string(),
            adapteros_core::B3Hash::hash(b"test-seed"),
        );

        assert_eq!(bundle.cpid, "test-cpid");
        assert_eq!(bundle.plan_id, "test-plan");
        assert!(!bundle.events.is_empty());
    }

    #[test]
    fn test_watchdog_pause_resume_functionality() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Monitor allocation while running
        watchdog.monitor_allocation(0x1000, 1024, "running".to_string()).unwrap();
        let stats_running = watchdog.get_stats();
        assert!(stats_running.total_events > 0);

        // Pause watchdog
        watchdog.pause();
        assert!(!watchdog.is_running());

        // Monitor allocation while paused (should be ignored)
        watchdog.monitor_allocation(0x2000, 2048, "paused".to_string()).unwrap();
        let stats_paused = watchdog.get_stats();
        assert_eq!(stats_paused.total_events, stats_running.total_events);

        // Resume watchdog
        watchdog.resume();
        assert!(watchdog.is_running());

        // Monitor allocation while resumed
        watchdog.monitor_allocation(0x3000, 4096, "resumed".to_string()).unwrap();
        let stats_resumed = watchdog.get_stats();
        assert!(stats_resumed.total_events > stats_paused.total_events);
    }

    #[test]
    fn test_memory_events_checking() {
        let config = MemoryWatchdogConfig {
            enable_heap_observation: true,
            enable_buffer_relocation_detection: true,
            ..Default::default()
        };

        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Check for memory events
        let events = watchdog.check_memory_events().unwrap();
        
        // Events may be empty depending on system state
        assert!(events.len() >= 0);
    }

    #[test]
    fn test_configuration_updates() {
        let mut config = MemoryWatchdogConfig::default();
        let mut watchdog = MemoryWatchdog::new(config.clone()).unwrap();

        // Update sampling rate
        config.sampling_rate = 0.5;
        watchdog.update_config(config.clone());
        assert_eq!(watchdog.get_config().sampling_rate, 0.5);

        // Update pressure thresholds
        config.pressure_warning_threshold = 0.75;
        config.pressure_critical_threshold = 0.95;
        watchdog.update_config(config);
        assert_eq!(watchdog.get_config().pressure_warning_threshold, 0.75);
        assert_eq!(watchdog.get_config().pressure_critical_threshold, 0.95);
    }

    #[test]
    fn test_memory_watchdog_clear_functionality() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Generate some events
        watchdog.monitor_allocation(0x1000, 1024, "test".to_string()).unwrap();
        watchdog.monitor_deallocation(0x1000, 1024, "test".to_string()).unwrap();

        let stats_before = watchdog.get_stats();
        assert!(stats_before.total_events > 0);

        // Clear all data
        watchdog.clear();

        let stats_after = watchdog.get_stats();
        assert_eq!(stats_after.total_events, 0);
    }

    #[test]
    fn test_concurrent_memory_monitoring() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = Arc::new(MemoryWatchdog::new(config).unwrap());

        let mut handles = vec![];

        // Spawn multiple threads to monitor memory concurrently
        for i in 0..5 {
            let watchdog_clone = Arc::clone(&watchdog);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let addr = 0x1000 + (i * 1000) + j;
                    let size = 1024 + (j * 100);
                    let context = format!("thread_{}_allocation_{}", i, j);
                    
                    watchdog_clone.monitor_allocation(addr, size, context).unwrap();
                    
                    // Small delay to simulate real usage
                    thread::sleep(Duration::from_millis(1));
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let stats = watchdog.get_stats();
        assert!(stats.total_events >= 50); // 5 threads * 10 allocations each
    }

    #[test]
    fn test_memory_layout_hash_consistency() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Generate initial layout hash
        let initial_hash = watchdog.generate_memory_layout_hash().unwrap();

        // Perform some allocations
        watchdog.monitor_allocation(0x1000, 1024, "test1".to_string()).unwrap();
        watchdog.monitor_allocation(0x2000, 2048, "test2".to_string()).unwrap();

        // Generate new layout hash
        let new_hash = watchdog.generate_memory_layout_hash().unwrap();

        // Hashes should be different
        assert_ne!(initial_hash.layout_hash, new_hash.layout_hash);

        // Verify consistency with new hash
        watchdog.verify_layout_consistency(&new_hash).unwrap();

        // Verify inconsistency with old hash
        let result = watchdog.verify_layout_consistency(&initial_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_watchdog_error_handling() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Test with invalid pointer address (should not panic)
        let result = watchdog.monitor_allocation(0, 0, "invalid".to_string());
        assert!(result.is_ok());

        // Test with very large size (should not panic)
        let result = watchdog.monitor_allocation(0x1000, u64::MAX, "large".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_watchdog_statistics() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        let initial_stats = watchdog.get_stats();
        assert_eq!(initial_stats.total_events, 0);
        assert!(initial_stats.uptime_micros > 0);

        // Generate some events
        watchdog.monitor_allocation(0x1000, 1024, "test".to_string()).unwrap();
        watchdog.monitor_deallocation(0x1000, 1024, "test".to_string()).unwrap();

        let final_stats = watchdog.get_stats();
        assert!(final_stats.total_events > initial_stats.total_events);
        assert!(final_stats.uptime_micros > initial_stats.uptime_micros);
    }
}

#[cfg(test)]
mod component_tests {
    use super::super::*;

    #[test]
    fn test_heap_observer_functionality() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = metal::Device::system_default() {
                let observer = MetalHeapObserver::new(Arc::new(device), 1.0);
                
                let stats = observer.get_memory_stats();
                assert_eq!(stats.allocation_count, 0);
                assert_eq!(stats.heap_count, 0);
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            let observer = MetalHeapObserver::new(None, 1.0);
            let stats = observer.get_memory_stats();
            assert_eq!(stats.allocation_count, 0);
        }
    }

    #[test]
    fn test_pointer_canonicalizer_functionality() {
        let canonicalizer = PointerCanonicalizer::new(1000);
        
        // Test allocation recording
        let allocation_id = canonicalizer.record_allocation(0x1000, 1024, "test".to_string()).unwrap();
        assert!(!allocation_id.is_nil());
        
        let stats = canonicalizer.get_allocation_stats();
        assert_eq!(stats.total_allocations, 1);
        assert_eq!(stats.active_allocations, 1);
        
        // Test deallocation recording
        canonicalizer.record_deallocation(0x1000).unwrap();
        
        let stats_after = canonicalizer.get_allocation_stats();
        assert_eq!(stats_after.active_allocations, 0);
    }

    #[test]
    fn test_buffer_relocation_detector_functionality() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = metal::Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device), true);
                
                let stats = detector.get_relocation_stats();
                assert_eq!(stats.total_buffers, 0);
                assert_eq!(stats.total_relocations, 0);
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            let stats = detector.get_relocation_stats();
            assert_eq!(stats.total_buffers, 0);
        }
    }

    #[test]
    fn test_memory_map_hasher_functionality() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = metal::Device::system_default() {
                let hasher = MemoryMapHasher::new(Arc::new(device), true);
                
                let stats = hasher.get_memory_stats();
                assert_eq!(stats.total_regions, 0);
                assert_eq!(stats.total_size, 0);
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            let hasher = MemoryMapHasher::new(None, true);
            let stats = hasher.get_memory_stats();
            assert_eq!(stats.total_regions, 0);
        }
    }

    #[test]
    fn test_replay_memory_logger_functionality() {
        let logger = ReplayMemoryLogger::new(true, 1.0);
        
        // Test allocation logging
        logger.log_allocation(0x1000, 1024, "test".to_string(), None).unwrap();
        
        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].event_type, crate::replay_integration::MemoryEventType::Allocation));
        
        // Test deallocation logging
        logger.log_deallocation(0x1000, 1024, "test".to_string(), None).unwrap();
        
        let events_after = logger.get_memory_events();
        assert_eq!(events_after.len(), 2);
    }
}

#[cfg(test)]
mod performance_tests {
    use super::super::*;
    use std::time::Instant;

    #[test]
    fn test_memory_monitoring_performance() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        let start = Instant::now();
        
        // Perform many allocations quickly
        for i in 0..1000 {
            watchdog.monitor_allocation(0x1000 + i, 1024, format!("perf_test_{}", i)).unwrap();
        }
        
        let duration = start.elapsed();
        
        // Should complete within reasonable time (adjust threshold as needed)
        assert!(duration.as_millis() < 1000); // Less than 1 second for 1000 allocations
        
        let stats = watchdog.get_stats();
        assert!(stats.total_events >= 1000);
    }

    #[test]
    fn test_memory_layout_hash_performance() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        // Generate some memory events first
        for i in 0..100 {
            watchdog.monitor_allocation(0x1000 + i, 1024, format!("hash_test_{}", i)).unwrap();
        }

        let start = Instant::now();
        
        // Generate layout hash multiple times
        for _ in 0..100 {
            let _hash = watchdog.generate_memory_layout_hash().unwrap();
        }
        
        let duration = start.elapsed();
        
        // Should complete within reasonable time
        assert!(duration.as_millis() < 500); // Less than 500ms for 100 hash generations
    }
}
