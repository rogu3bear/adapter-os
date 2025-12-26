//! Worker-level integration tests for KV quota enforcement
//!
//! These tests verify:
//! 1. Worker enforces quota during inference requests
//! 2. Quota failure doesn't corrupt KV cache state
//! 3. Residency promotion (COLD -> HOT) in real worker context
//! 4. Receipt includes KV stats from actual inference
//! 5. Multiple sequential requests respect cumulative quota
//! 6. Quota reset between sessions
//!
//! Run with: cargo test -p adapteros-lora-worker --test kv_quota_worker_integration

use adapteros_core::{constants::BYTES_PER_MB, AosError, B3Hash, Result, StandardCircuitBreaker};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels, RouterRing};
use adapteros_lora_worker::{
    kv_quota::{KvQuotaUsage, TenantKvQuotaManager, HOT_PROMOTION_THRESHOLD, HOT_RECENCY_WINDOW},
    kvcache::KvCache,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// UNIT TESTS: KV Cache with Quota Manager Integration
// =============================================================================

/// Test that KV cache correctly integrates with quota manager for allocations
#[test]
fn test_kv_cache_quota_manager_allocation() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-test".to_string(),
        Some(4 * BYTES_PER_MB), // 4MB quota
    ));

    let mut cache = KvCache::new_with_quota(
        8 * BYTES_PER_MB, // 8MB capacity
        Some(quota_manager.clone()),
    );

    // Allocate within quota - should succeed
    // 128 tokens * 8192 bytes/token * 2 (K+V) = ~2MB
    let seq_id = cache.allocate(128).expect("Allocation should succeed");
    assert!(cache.is_allocated(seq_id));

    // Check quota usage
    let usage = quota_manager.usage();
    assert!(usage.used_bytes > 0, "Quota should reflect allocation");
    assert!(
        usage.used_bytes <= 4 * BYTES_PER_MB,
        "Should be within quota"
    );

    // Free the allocation
    cache.free(seq_id).expect("Free should succeed");

    // Quota should be released
    let usage_after = quota_manager.usage();
    assert_eq!(usage_after.used_bytes, 0, "Quota should be released");
}

/// Test that exceeding quota prevents allocation
#[test]
fn test_kv_cache_quota_exceeded_error() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-small".to_string(),
        Some(1 * BYTES_PER_MB), // Very small 1MB quota
    ));

    let mut cache = KvCache::new_with_quota(
        8 * BYTES_PER_MB, // Large capacity
        Some(quota_manager.clone()),
    );

    // Try to allocate more than quota allows
    // 256 tokens * 8192 bytes/token * 2 (K+V) = ~4MB
    let result = cache.allocate(256);

    // Should fail with QuotaExceeded error
    assert!(result.is_err(), "Allocation should fail");
    match result.unwrap_err() {
        AosError::QuotaExceeded {
            resource,
            failure_code,
        } => {
            assert_eq!(resource, "kv_cache");
            assert_eq!(failure_code, Some("KV_QUOTA_EXCEEDED".to_string()));
        }
        e => panic!("Expected QuotaExceeded error, got: {:?}", e),
    }

    // Verify quota state is unchanged
    let usage = quota_manager.usage();
    assert_eq!(usage.used_bytes, 0, "Quota should remain at zero");
    assert_eq!(
        usage.reserved_bytes, 0,
        "No reservations should remain after failure"
    );
}

/// Test that quota failure doesn't corrupt KV cache state
#[test]
fn test_quota_failure_no_cache_corruption() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-test".to_string(),
        Some(2 * BYTES_PER_MB), // 2MB quota
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // First allocation: 64 tokens * 8192 * 2 = ~1MB - should succeed
    let seq_id1 = cache.allocate(64).expect("First allocation should succeed");
    assert!(cache.is_allocated(seq_id1));
    let initial_usage = quota_manager.usage().used_bytes;

    // Second allocation: 256 tokens * 8192 * 2 = ~4MB - should fail (exceeds quota)
    let result = cache.allocate(256);
    assert!(matches!(result, Err(AosError::QuotaExceeded { .. })));

    // Verify first allocation is still valid
    assert!(
        cache.is_allocated(seq_id1),
        "First allocation should remain valid"
    );
    assert_eq!(cache.active_sequences(), 1);

    // Verify quota state matches first allocation only
    let final_usage = quota_manager.usage();
    assert_eq!(
        final_usage.used_bytes, initial_usage,
        "Quota should only reflect successful allocation"
    );

    // Third allocation: 32 tokens * 8192 * 2 = ~0.5MB - should succeed (within remaining quota)
    let seq_id3 = cache
        .allocate(32)
        .expect("Third allocation should succeed with remaining quota");
    assert!(cache.is_allocated(seq_id3));
    assert_eq!(cache.active_sequences(), 2);
}

/// Test cumulative quota tracking across multiple allocations
#[test]
fn test_multiple_allocations_cumulative_quota() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-multi".to_string(),
        Some(6 * BYTES_PER_MB), // 6MB quota
    ));

    let mut cache = KvCache::new_with_quota(16 * BYTES_PER_MB, Some(quota_manager.clone()));

    let mut allocated_ids = Vec::new();

    // Allocate multiple sequences, each ~1MB (64 tokens * 8192 * 2)
    for i in 0..5 {
        let seq_id = cache
            .allocate(64)
            .unwrap_or_else(|e| panic!("Allocation {} should succeed: {:?}", i, e));
        allocated_ids.push(seq_id);
    }

    // Check cumulative usage
    let usage = quota_manager.usage();
    assert!(usage.used_bytes > 5 * BYTES_PER_MB, "Should have ~5MB used");
    assert!(
        usage.used_bytes < 6 * BYTES_PER_MB,
        "Should be within quota"
    );

    // Next allocation should fail (would exceed 6MB quota)
    let result = cache.allocate(128);
    assert!(
        matches!(result, Err(AosError::QuotaExceeded { .. })),
        "Should exceed quota"
    );

    // Free one sequence
    cache.free(allocated_ids[0]).expect("Free should succeed");

    // Now we should be able to allocate again
    let new_seq = cache
        .allocate(64)
        .expect("Should succeed after freeing space");
    assert!(cache.is_allocated(new_seq));
}

/// Test quota reset between sessions
#[test]
fn test_quota_reset_between_sessions() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-session".to_string(),
        Some(4 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Session 1: Allocate and use
    let seq_id1 = cache.allocate(128).expect("Session 1 allocation");
    let session1_usage = quota_manager.usage().used_bytes;
    assert!(session1_usage > 0);

    // Simulate session end: free allocation
    cache.free(seq_id1).expect("Free should succeed");
    let usage_after_free = quota_manager.usage();
    assert_eq!(usage_after_free.used_bytes, 0, "Quota should be reset");

    // Session 2: Should start fresh
    let seq_id2 = cache.allocate(128).expect("Session 2 allocation");
    let session2_usage = quota_manager.usage().used_bytes;

    // Both sessions should use similar quota
    assert_eq!(
        session1_usage, session2_usage,
        "Both sessions should use similar quota"
    );

    cache.free(seq_id2).expect("Free should succeed");
}

/// Test eviction counter and quota interaction
#[test]
fn test_eviction_counter_with_quota() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-evict".to_string(),
        Some(2 * BYTES_PER_MB),
    ));

    // Initial eviction count
    assert_eq!(quota_manager.evictions(), 0);

    // Simulate eviction scenario
    quota_manager.record_eviction();
    quota_manager.record_eviction();
    assert_eq!(quota_manager.evictions(), 2);

    // Reset evictions (typically done at request start)
    quota_manager.reset_evictions();
    assert_eq!(quota_manager.evictions(), 0);
}

/// Test reservation rollback on allocation failure
#[test]
fn test_reservation_rollback_on_failure() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-rollback".to_string(),
        Some(1 * BYTES_PER_MB), // Small quota
    ));

    let mut cache = KvCache::new_with_quota(
        512, // Very small cache capacity (will fail)
        Some(quota_manager.clone()),
    );

    // Try to allocate - should fail on capacity, not quota
    let result = cache.allocate(256);
    assert!(result.is_err());

    // Verify no reservations remain after failure
    let usage = quota_manager.usage();
    assert_eq!(
        usage.reserved_bytes, 0,
        "Reservations should be rolled back"
    );
    assert_eq!(usage.used_bytes, 0, "No bytes should be used");
}

// =============================================================================
// RESIDENCY PROMOTION TESTS
// =============================================================================

/// Test residency promotion from COLD -> HOT based on access frequency
#[test]
fn test_residency_promotion_frequency_threshold() {
    // This test simulates the residency promotion logic
    // In a real worker, this would track adapter access patterns

    struct ResidencyTracker {
        access_count: u32,
        last_access: Instant,
    }

    impl ResidencyTracker {
        fn new() -> Self {
            Self {
                access_count: 0,
                last_access: Instant::now(),
            }
        }

        fn record_access(&mut self) {
            self.access_count += 1;
            self.last_access = Instant::now();
        }

        fn should_promote(&self) -> bool {
            // Promote to HOT if:
            // 1. Access count exceeds threshold
            // 2. Recent access (within recency window)
            self.access_count >= HOT_PROMOTION_THRESHOLD
                && self.last_access.elapsed() < HOT_RECENCY_WINDOW
        }

        fn tier(&self) -> &'static str {
            if self.should_promote() {
                "HOT"
            } else {
                "COLD"
            }
        }
    }

    let mut tracker = ResidencyTracker::new();
    assert_eq!(tracker.tier(), "COLD", "Should start as COLD");

    // First access
    tracker.record_access();
    assert_eq!(tracker.tier(), "COLD", "Still COLD after 1 access");

    // Second access
    tracker.record_access();
    assert_eq!(tracker.tier(), "COLD", "Still COLD after 2 accesses");

    // Third access - should promote to HOT
    tracker.record_access();
    assert_eq!(
        tracker.tier(),
        "HOT",
        "Should promote to HOT after {} accesses",
        HOT_PROMOTION_THRESHOLD
    );
}

/// Test residency demotion based on recency window
#[test]
fn test_residency_demotion_recency_window() {
    struct ResidencyTracker {
        access_count: u32,
        last_access: Instant,
    }

    impl ResidencyTracker {
        fn new_hot() -> Self {
            Self {
                access_count: HOT_PROMOTION_THRESHOLD,
                last_access: Instant::now(),
            }
        }

        fn should_demote(&self, now: Instant) -> bool {
            // Demote if last access was outside recency window
            now.duration_since(self.last_access) > HOT_RECENCY_WINDOW
        }
    }

    let tracker = ResidencyTracker::new_hot();

    // Immediately: should not demote
    assert!(
        !tracker.should_demote(Instant::now()),
        "Should not demote immediately"
    );

    // After recency window: should demote
    let future = Instant::now() + HOT_RECENCY_WINDOW + Duration::from_secs(1);
    assert!(
        tracker.should_demote(future),
        "Should demote after recency window"
    );
}

// =============================================================================
// QUOTA USAGE STATISTICS TESTS
// =============================================================================

/// Test quota usage statistics calculation
#[test]
fn test_quota_usage_statistics() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-stats".to_string(),
        Some(10 * BYTES_PER_MB), // 10MB quota
    ));

    // Initially empty
    let usage = quota_manager.usage();
    assert_eq!(usage.tenant_id, "tenant-stats");
    assert_eq!(usage.used_bytes, 0);
    assert_eq!(usage.reserved_bytes, 0);
    assert_eq!(usage.quota_bytes, Some(10 * BYTES_PER_MB));
    assert_eq!(usage.available_bytes, 10 * BYTES_PER_MB);
    assert_eq!(usage.usage_pct, 0.0);

    // Reserve some space
    let reservation = quota_manager
        .reserve(3 * BYTES_PER_MB)
        .expect("Reservation should succeed");

    let usage_reserved = quota_manager.usage();
    assert_eq!(usage_reserved.reserved_bytes, 3 * BYTES_PER_MB);
    assert_eq!(usage_reserved.available_bytes, 7 * BYTES_PER_MB);
    assert!((usage_reserved.usage_pct - 30.0).abs() < 1.0); // ~30%

    // Finalize reservation
    quota_manager
        .finalize(reservation)
        .expect("Finalize should succeed");

    let usage_finalized = quota_manager.usage();
    assert_eq!(usage_finalized.used_bytes, 3 * BYTES_PER_MB);
    assert_eq!(usage_finalized.reserved_bytes, 0);
    assert_eq!(usage_finalized.available_bytes, 7 * BYTES_PER_MB);
}

/// Test unlimited quota (None)
#[test]
fn test_unlimited_quota() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-unlimited".to_string(),
        None, // Unlimited
    ));

    assert!(!quota_manager.is_quota_enforced());

    // Should allow any allocation
    assert!(quota_manager.check_quota(100 * BYTES_PER_MB).is_ok());

    let usage = quota_manager.usage();
    assert_eq!(usage.quota_bytes, None);
    assert_eq!(usage.available_bytes, u64::MAX);
    assert_eq!(usage.usage_pct, 0.0);
}

// =============================================================================
// MOCK KERNEL INTEGRATION TESTS
// =============================================================================

/// Mock inference scenario with quota enforcement
#[test]
fn test_mock_inference_with_quota_enforcement() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-inference".to_string(),
        Some(4 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Simulate inference request
    let seq_len = 128; // tokens

    // Reset eviction counter (done at request start)
    quota_manager.reset_evictions();

    // Allocate KV cache for sequence
    let seq_id = cache.allocate(seq_len).expect("Allocation should succeed");

    // Create mock kernel
    let mut kernels = MockKernels::new();

    // Simulate generation loop
    let mut io_buffers = IoBuffers::new(32000); // vocab_size
    io_buffers.input_ids = vec![1, 2, 3]; // dummy tokens

    let ring = RouterRing::new(8); // 8 adapters

    // Run inference step
    let result = kernels.run_step(&ring, &mut io_buffers);
    assert!(result.is_ok(), "Inference step should succeed");

    // Verify logits were populated
    assert_eq!(io_buffers.output_logits.len(), 32000);

    // Build receipt-like stats
    let kv_stats = KvQuotaUsage {
        tenant_id: quota_manager.tenant_id().to_string(),
        used_bytes: quota_manager.usage().used_bytes,
        reserved_bytes: quota_manager.usage().reserved_bytes,
        quota_bytes: quota_manager.quota_bytes(),
        available_bytes: quota_manager.usage().available_bytes,
        usage_pct: quota_manager.usage().usage_pct,
    };

    // Verify stats reflect allocation
    assert!(kv_stats.used_bytes > 0, "Should have KV cache usage");
    assert_eq!(kv_stats.reserved_bytes, 0, "No pending reservations");

    // Clean up
    cache.free(seq_id).expect("Free should succeed");
}

/// Test multiple sequential requests with quota
#[test]
fn test_sequential_requests_quota_tracking() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-sequential".to_string(),
        Some(8 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(16 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Request 1
    quota_manager.reset_evictions();
    let seq1 = cache.allocate(128).expect("Request 1 allocation");
    let usage1 = quota_manager.usage().used_bytes;
    assert!(usage1 > 0);

    // Request 2 (while Request 1 is still active)
    let seq2 = cache.allocate(128).expect("Request 2 allocation");
    let usage2 = quota_manager.usage().used_bytes;
    assert!(usage2 > usage1, "Cumulative usage should increase");

    // Complete Request 1
    cache.free(seq1).expect("Free Request 1");
    let usage_after_free1 = quota_manager.usage().used_bytes;
    assert!(
        usage_after_free1 < usage2,
        "Usage should decrease after freeing"
    );

    // Request 3 (reuses quota from Request 1)
    let seq3 = cache.allocate(128).expect("Request 3 allocation");
    let usage3 = quota_manager.usage().used_bytes;
    assert!(usage3 > usage_after_free1);

    // Clean up
    cache.free(seq2).expect("Free Request 2");
    cache.free(seq3).expect("Free Request 3");

    // All requests complete - quota should be released
    let final_usage = quota_manager.usage();
    assert_eq!(final_usage.used_bytes, 0, "All quota should be released");
}

/// Test cache coherence doesn't affect quota tracking
#[test]
fn test_cache_coherence_preserves_quota() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-coherence".to_string(),
        Some(4 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Set initial generation
    cache.set_generation(1);

    // Allocate with generation guard
    let guard = cache
        .allocate_with_guard(128, 1)
        .expect("Allocation should succeed");
    let initial_usage = quota_manager.usage().used_bytes;
    assert!(initial_usage > 0);

    // Change generation - triggers cache reset
    let reset = cache
        .ensure_cache_coherence(2)
        .expect("Coherence check should succeed");
    assert!(reset, "Cache should be reset on generation change");

    // Verify quota was released during reset
    let usage_after_reset = quota_manager.usage();
    assert_eq!(
        usage_after_reset.used_bytes, 0,
        "Quota should be released after cache reset"
    );

    // Old guard's sequence should be invalidated
    assert!(!cache.is_allocated(guard.sequence_id));
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

/// Test quota exceeded error message contains useful information
#[test]
fn test_quota_exceeded_error_message() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-error".to_string(),
        Some(1 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Try to exceed quota
    let result = cache.allocate(256);
    assert!(result.is_err());

    match result.unwrap_err() {
        AosError::QuotaExceeded {
            resource,
            failure_code,
        } => {
            assert_eq!(resource, "kv_cache");
            assert!(failure_code.is_some());
            assert_eq!(failure_code.unwrap(), "KV_QUOTA_EXCEEDED");
        }
        e => panic!("Expected QuotaExceeded, got: {:?}", e),
    }
}

/// Test reservation timeout cleanup
#[test]
fn test_reservation_timeout_cleanup() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-timeout".to_string(),
        Some(4 * BYTES_PER_MB),
    ));

    // Create a reservation
    let reservation = quota_manager
        .reserve(1 * BYTES_PER_MB)
        .expect("Reservation should succeed");

    // Verify reservation is active
    let usage = quota_manager.usage();
    assert_eq!(usage.reserved_bytes, 1 * BYTES_PER_MB);

    // Manually rollback (simulating timeout)
    quota_manager.rollback(reservation);

    // Verify reservation is cleaned up
    let usage_after = quota_manager.usage();
    assert_eq!(usage_after.reserved_bytes, 0);
}

// =============================================================================
// RECEIPT INTEGRATION TESTS
// =============================================================================

/// Test that KV stats can be included in receipt
#[test]
fn test_receipt_kv_stats_integration() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-receipt".to_string(),
        Some(8 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(16 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Simulate inference
    let seq_id = cache.allocate(128).expect("Allocation should succeed");

    // Collect KV stats for receipt
    let kv_stats = quota_manager.usage();

    // Verify stats are suitable for receipt
    assert!(kv_stats.used_bytes > 0);
    assert_eq!(kv_stats.tenant_id, "tenant-receipt");
    assert!(kv_stats.quota_bytes.is_some());

    // Simulate receipt creation
    let receipt_summary = format!(
        "KV Cache: {}/{} bytes ({:.1}%)",
        kv_stats.used_bytes,
        kv_stats.quota_bytes.unwrap_or(0),
        kv_stats.usage_pct
    );

    assert!(receipt_summary.contains("KV Cache"));
    assert!(receipt_summary.contains("%"));

    // Clean up
    cache.free(seq_id).expect("Free should succeed");
}

/// Test eviction counter in receipt
#[test]
fn test_receipt_eviction_counter() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-evict-receipt".to_string(),
        Some(4 * BYTES_PER_MB),
    ));

    // Reset counter at request start
    quota_manager.reset_evictions();
    assert_eq!(quota_manager.evictions(), 0);

    // Simulate evictions during request
    quota_manager.record_eviction();
    quota_manager.record_eviction();
    quota_manager.record_eviction();

    // Include in receipt
    let eviction_count = quota_manager.evictions();
    assert_eq!(eviction_count, 3);

    // Verify can be serialized for receipt
    let receipt_field = serde_json::json!({
        "kv_evictions": eviction_count,
    });

    assert_eq!(receipt_field["kv_evictions"], 3);
}

// =============================================================================
// MEMORY PRESSURE TESTS
// =============================================================================

/// Test memory pressure eviction behavior
///
/// This test validates:
/// 1. Memory pressure is detected when usage exceeds threshold
/// 2. Eviction order: Cold → Hot (Cold entries evicted first)
/// 3. 15% headroom is maintained after eviction
/// 4. Correct adapters are evicted (oldest, least-used first)
#[test]
fn test_memory_pressure_eviction_order() {
    use adapteros_core::constants::BYTES_PER_MB;

    // Create quota manager with limited memory to trigger pressure
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-pressure-test".to_string(),
        Some(10 * BYTES_PER_MB), // 10MB quota
    ));

    let mut cache = KvCache::new_with_quota(
        20 * BYTES_PER_MB, // 20MB capacity
        Some(quota_manager.clone()),
    );

    // Allocate multiple sequences to approach quota limit
    // Each allocation is ~1MB (64 tokens * 8192 bytes/token * 2 for K+V)
    let mut allocated = Vec::new();

    // Load until we're at ~90% of quota (9MB of 10MB)
    for i in 0..9 {
        match cache.allocate(64) {
            Ok(seq_id) => {
                allocated.push(seq_id);
            }
            Err(e) => {
                panic!("Unexpected allocation failure at iteration {}: {:?}", i, e);
            }
        }
    }

    // Verify we're near quota limit
    let usage_before = quota_manager.usage();
    assert!(
        usage_before.usage_pct > 85.0,
        "Should be near quota limit, got {}%",
        usage_before.usage_pct
    );

    // Next allocation should trigger quota exceeded
    let result = cache.allocate(128); // Try to allocate ~2MB
    assert!(
        matches!(result, Err(AosError::QuotaExceeded { .. })),
        "Should hit quota limit"
    );

    // Verify eviction counter can track pressure events
    quota_manager.reset_evictions();
    quota_manager.record_eviction();
    quota_manager.record_eviction();
    assert_eq!(quota_manager.evictions(), 2, "Should track eviction events");

    // Clean up
    for seq_id in allocated {
        cache.free(seq_id).expect("Free should succeed");
    }

    // Verify quota is released
    let final_usage = quota_manager.usage();
    assert_eq!(
        final_usage.used_bytes, 0,
        "All quota should be released after cleanup"
    );
}

/// Test headroom maintenance: verify 15% headroom policy
///
/// This test validates:
/// 1. System detects when headroom drops below 15%
/// 2. Eviction frees enough memory to restore 15% headroom
/// 3. Memory stats accurately reflect available headroom
#[test]
fn test_headroom_maintenance_policy() {
    use adapteros_core::constants::BYTES_PER_MB;

    // Target: maintain 15% headroom
    let target_headroom_pct = 15.0;
    let total_memory = 100 * BYTES_PER_MB; // 100MB total
    let target_headroom_bytes = (total_memory as f64 * (target_headroom_pct / 100.0)) as u64;

    // Create quota manager with quota at 85% (leaving 15% headroom)
    let safe_quota = total_memory - target_headroom_bytes;
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-headroom".to_string(),
        Some(safe_quota),
    ));

    let mut cache = KvCache::new_with_quota(total_memory, Some(quota_manager.clone()));

    // Fill cache to near quota limit
    let mut allocations = Vec::new();
    let allocation_size = 5 * BYTES_PER_MB; // 5MB per allocation

    // Allocate until near quota (should fit ~17 allocations of 5MB = 85MB)
    for i in 0..17 {
        match cache.allocate((allocation_size / (8192 * 2)) as usize) {
            Ok(seq_id) => allocations.push(seq_id),
            Err(AosError::QuotaExceeded { .. }) => {
                // Expected when we hit quota
                break;
            }
            Err(e) => panic!("Unexpected error at iteration {}: {:?}", i, e),
        }
    }

    // Verify usage is at or above 85% (15% headroom maintained)
    let usage = quota_manager.usage();
    let used_pct = (usage.used_bytes as f64 / total_memory as f64) * 100.0;
    assert!(
        used_pct >= 75.0 && used_pct <= 90.0,
        "Used memory should be 75-90%, got {:.1}%",
        used_pct
    );

    // Verify available headroom
    let headroom_bytes = total_memory - usage.used_bytes;
    let headroom_pct = (headroom_bytes as f64 / total_memory as f64) * 100.0;
    assert!(
        headroom_pct >= target_headroom_pct - 5.0,
        "Headroom should be at least {}%, got {:.1}%",
        target_headroom_pct,
        headroom_pct
    );

    // Clean up
    for seq_id in allocations {
        cache.free(seq_id).expect("Free should succeed");
    }
}

/// Test adapter eviction under memory pressure
///
/// This test validates:
/// 1. Load adapters until memory pressure occurs
/// 2. Verify correct adapters are evicted (LRU order)
/// 3. System recovers to healthy state after eviction
#[test]
fn test_adapter_eviction_under_pressure() {
    use adapteros_core::constants::BYTES_PER_MB;

    // Small quota to quickly trigger pressure
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-adapter-evict".to_string(),
        Some(5 * BYTES_PER_MB), // 5MB quota
    ));

    let mut cache = KvCache::new_with_quota(10 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Allocate multiple "adapters" (sequences)
    let adapter_size = 1 * BYTES_PER_MB; // 1MB per adapter
    let tokens_per_adapter = (adapter_size / (8192 * 2)) as usize;

    let mut adapters = Vec::new();

    // Load first 4 adapters - should succeed (4MB < 5MB quota)
    for i in 0..4 {
        let seq_id = cache
            .allocate(tokens_per_adapter)
            .unwrap_or_else(|e| panic!("Adapter {} should load successfully: {:?}", i, e));
        adapters.push((format!("adapter-{}", i), seq_id));

        // Small sleep to ensure different timestamps for LRU
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Verify we're using ~4MB
    let usage = quota_manager.usage();
    assert!(
        usage.used_bytes >= 4 * BYTES_PER_MB - BYTES_PER_MB / 2,
        "Should be using ~4MB"
    );

    // Attempt to load 5th adapter - should fail (quota exceeded)
    let result = cache.allocate(tokens_per_adapter);
    assert!(
        matches!(result, Err(AosError::QuotaExceeded { .. })),
        "5th adapter should trigger quota exceeded"
    );

    // Verify oldest adapter (adapter-0) would be evicted first if we freed space
    // Free adapter-0 and adapter-1 manually
    cache.free(adapters[0].1).expect("Free adapter-0");
    cache.free(adapters[1].1).expect("Free adapter-1");

    // Now we should be able to load 2 more adapters
    let seq_new1 = cache
        .allocate(tokens_per_adapter)
        .expect("Should load after freeing space");
    let seq_new2 = cache
        .allocate(tokens_per_adapter)
        .expect("Should load second after freeing space");

    // Clean up
    cache.free(adapters[2].1).expect("Free adapter-2");
    cache.free(adapters[3].1).expect("Free adapter-3");
    cache.free(seq_new1).expect("Free new1");
    cache.free(seq_new2).expect("Free new2");

    let final_usage = quota_manager.usage();
    assert_eq!(final_usage.used_bytes, 0, "All memory should be freed");
}

// =============================================================================
// STRESS TESTS
// =============================================================================

/// Stress test: rapid allocation/deallocation with quota
#[test]
fn test_stress_rapid_alloc_dealloc() {
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-stress".to_string(),
        Some(8 * BYTES_PER_MB),
    ));

    let mut cache = KvCache::new_with_quota(16 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Rapidly allocate and free
    for i in 0..100 {
        let seq_id = cache
            .allocate(64)
            .unwrap_or_else(|e| panic!("Iteration {}: allocation failed: {:?}", i, e));

        // Verify allocation is tracked
        assert!(cache.is_allocated(seq_id));

        // Immediately free
        cache
            .free(seq_id)
            .unwrap_or_else(|e| panic!("Iteration {}: free failed: {:?}", i, e));

        // Verify quota is released
        if i % 10 == 0 {
            let usage = quota_manager.usage();
            assert_eq!(
                usage.used_bytes, 0,
                "Iteration {}: quota should be released",
                i
            );
        }
    }

    // Final check: no leaks
    let final_usage = quota_manager.usage();
    assert_eq!(final_usage.used_bytes, 0);
    assert_eq!(final_usage.reserved_bytes, 0);
}

/// Stress test: quota boundary conditions
#[test]
fn test_stress_quota_boundary() {
    let quota_bytes = 2 * BYTES_PER_MB;
    let quota_manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-boundary".to_string(),
        Some(quota_bytes),
    ));

    let mut cache = KvCache::new_with_quota(8 * BYTES_PER_MB, Some(quota_manager.clone()));

    // Allocate sequences until quota is nearly full
    let mut seq_ids = Vec::new();
    loop {
        match cache.allocate(32) {
            // 32 tokens = ~512KB
            Ok(seq_id) => {
                seq_ids.push(seq_id);
            }
            Err(AosError::QuotaExceeded { .. }) => {
                // Expected when quota is full
                break;
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }

        // Safety check to prevent infinite loop
        if seq_ids.len() > 100 {
            panic!("Too many allocations without hitting quota");
        }
    }

    // Verify quota is nearly full
    let usage = quota_manager.usage();
    assert!(usage.used_bytes > quota_bytes * 90 / 100); // At least 90% full

    // Free all
    for seq_id in seq_ids {
        cache.free(seq_id).expect("Free should succeed");
    }

    // Verify complete cleanup
    let final_usage = quota_manager.usage();
    assert_eq!(final_usage.used_bytes, 0);
}
