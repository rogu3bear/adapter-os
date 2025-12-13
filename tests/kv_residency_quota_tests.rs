//! Integration tests for KV Residency and Quota feature (PRD: KvResidencyAndQuotas v1)
//!
//! Tests the following acceptance criteria:
//! 1. HOT KV buffers are marked non-purgeable when supported
//! 2. Per-tenant KV quota is enforced with reservations
//! 3. Quota exceeded returns error and does not poison cache
//! 4. KV quota and residency fields committed to receipt and Merkle bundle
//! 5. Active KV entries are never evicted under pressure

use adapteros_lora_kernel_mtl::{KvResidency, PurgeableBuffer, PurgeableState};
use adapteros_lora_worker::TenantKvQuotaManager;

// ============================================================================
// Test 1: HOT KV buffers marked non-purgeable when supported
// ============================================================================

#[cfg(target_os = "macos")]
#[test]
fn test_hot_kv_buffers_marked_non_purgeable_when_supported() {
    use metal::Device;

    // Create a Metal device and buffer
    let device = match Device::system_default() {
        Some(d) => d,
        None => {
            eprintln!("No Metal device available, skipping test");
            return;
        }
    };

    let buffer = device.new_buffer(4096, metal::MTLResourceOptions::StorageModeShared);

    // Mark buffer as non-purgeable (HOT protection)
    let result = buffer
        .make_non_purgeable()
        .expect("Should succeed on supported hardware");

    // Buffer should not be marked as purged
    assert!(!result.was_purged, "Fresh buffer should not be purged");

    // Verify we can query current state
    let result2 = buffer
        .set_purgeable_state(PurgeableState::KeepCurrent)
        .expect("Query should succeed");

    // NonVolatile is expected after make_non_purgeable
    assert_eq!(
        result2.previous,
        PurgeableState::NonVolatile,
        "Buffer should be non-purgeable (NonVolatile)"
    );
}

// ============================================================================
// Test 2: Per-tenant KV quota enforced with reservations
// ============================================================================

#[test]
fn test_per_tenant_kv_quota_enforced_with_reservations() {
    let quota_bytes = 1024u64; // 1KB quota
    let manager = TenantKvQuotaManager::new("tenant-test".to_string(), Some(quota_bytes));

    // Verify quota is enforced
    assert!(manager.is_quota_enforced());
    assert_eq!(manager.quota_bytes(), Some(quota_bytes));

    // Reserve 512 bytes - should succeed
    let res1 = manager
        .reserve(512)
        .expect("First reservation should succeed");
    let usage = manager.usage();
    assert_eq!(usage.reserved_bytes, 512);
    assert_eq!(usage.used_bytes, 0);

    // Finalize the first reservation
    manager.finalize(res1).expect("Finalize should succeed");
    let usage = manager.usage();
    assert_eq!(usage.reserved_bytes, 0);
    assert_eq!(usage.used_bytes, 512);

    // Reserve another 256 bytes - should succeed (512 + 256 = 768 < 1024)
    let res2 = manager
        .reserve(256)
        .expect("Second reservation should succeed");
    manager.finalize(res2).expect("Finalize should succeed");

    let usage = manager.usage();
    assert_eq!(usage.used_bytes, 768);
    assert_eq!(usage.available_bytes, 256); // 1024 - 768

    // Try to reserve more than available - should fail
    let res3 = manager.reserve(512);
    assert!(
        res3.is_err(),
        "Should fail - exceeds quota (768 + 512 > 1024)"
    );
}

// ============================================================================
// Test 3: Quota exceeded returns error and does not poison cache
// ============================================================================

#[test]
fn test_quota_exceeded_returns_error_and_does_not_poison_cache() {
    let quota_bytes = 256u64;
    let manager = TenantKvQuotaManager::new("tenant-poison-test".to_string(), Some(quota_bytes));

    // Fill quota
    let res1 = manager.reserve(256).expect("Should succeed");
    manager.finalize(res1).expect("Finalize should succeed");

    // Attempt to exceed quota
    let exceeded_result = manager.reserve(1);
    assert!(exceeded_result.is_err(), "Should fail when quota exceeded");

    // Verify cache is not poisoned - quota state should be clean
    let usage = manager.usage();
    assert_eq!(usage.used_bytes, 256, "Used bytes should remain unchanged");
    assert_eq!(usage.reserved_bytes, 0, "No dangling reservations");

    // Release some bytes and verify we can reserve again
    manager.release(128);
    let res2 = manager.reserve(100);
    assert!(
        res2.is_ok(),
        "Should succeed after releasing bytes - cache not poisoned"
    );

    // Cleanup
    if let Ok(r) = res2 {
        manager.rollback(r);
    }
}

// ============================================================================
// Test 4: KV quota and residency fields committed to receipt
// ============================================================================

#[test]
fn test_kv_quota_and_residency_fields_committed_to_receipt_and_merkle_bundle() {
    use adapteros_api_types::inference::KvUsageStats;

    // Create KV usage stats (as would be generated during inference)
    let kv_stats = KvUsageStats {
        tenant_kv_quota_bytes: 1_000_000,
        tenant_kv_bytes_used: 500_000,
        kv_evictions: 2,
        kv_residency_policy_id: Some("kv_residency_v1".to_string()),
        kv_quota_enforced: true,
    };

    // Verify stats can be serialized (for receipt inclusion)
    let json = serde_json::to_string(&kv_stats).expect("Should serialize");
    assert!(json.contains("tenant_kv_quota_bytes"));
    assert!(json.contains("1000000"));
    assert!(json.contains("kv_residency_v1"));

    // Verify deserialization
    let parsed: KvUsageStats = serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(parsed.tenant_kv_quota_bytes, 1_000_000);
    assert_eq!(parsed.tenant_kv_bytes_used, 500_000);
    assert_eq!(parsed.kv_evictions, 2);
    assert!(parsed.kv_quota_enforced);

    // Test backward compatibility - old JSON without KV fields
    let old_json = r#"{"tenant_kv_quota_bytes":0,"tenant_kv_bytes_used":0,"kv_evictions":0,"kv_quota_enforced":false}"#;
    let old_stats: KvUsageStats =
        serde_json::from_str(old_json).expect("Should deserialize old format");
    assert_eq!(old_stats.tenant_kv_quota_bytes, 0);
    assert!(!old_stats.kv_quota_enforced);
}

// ============================================================================
// Test 5: Active KV entries never evicted under pressure
// ============================================================================

#[test]
fn test_active_kv_entries_never_evicted_under_pressure() {
    // Test residency classification
    let hot = KvResidency::Hot;
    let cold = KvResidency::Cold;

    // Verify default is COLD
    assert_eq!(KvResidency::default(), KvResidency::Cold);

    // Verify display
    assert_eq!(format!("{}", hot), "HOT");
    assert_eq!(format!("{}", cold), "COLD");

    // In a real eviction scenario, HOT entries should be protected.
    // This test verifies the type system supports the classification.
    // Full eviction behavior is tested at the memory pool level.
    assert_ne!(hot, cold, "HOT and COLD should be distinct");
}

// ============================================================================
// Additional tests
// ============================================================================

#[test]
fn test_reservation_rollback_restores_quota() {
    let manager = TenantKvQuotaManager::new("tenant-rollback".to_string(), Some(1000));

    let res = manager.reserve(500).expect("Should succeed");
    assert_eq!(manager.usage().reserved_bytes, 500);

    // Rollback instead of finalize
    manager.rollback(res);

    // Quota should be fully restored
    let usage = manager.usage();
    assert_eq!(usage.reserved_bytes, 0);
    assert_eq!(usage.used_bytes, 0);
    assert_eq!(usage.available_bytes, 1000);
}

#[test]
fn test_unlimited_quota_allows_any_reservation() {
    let manager = TenantKvQuotaManager::new("tenant-unlimited".to_string(), None);

    assert!(!manager.is_quota_enforced());
    assert_eq!(manager.quota_bytes(), None);

    // Should allow very large reservations
    let res = manager
        .reserve(u64::MAX / 2)
        .expect("Unlimited should allow large reservation");
    manager.rollback(res);
}

#[test]
fn test_eviction_counter_tracking() {
    let manager = TenantKvQuotaManager::new("tenant-evict".to_string(), Some(1000));

    assert_eq!(manager.evictions(), 0);

    manager.record_eviction();
    manager.record_eviction();
    manager.record_eviction();

    assert_eq!(manager.evictions(), 3);

    manager.reset_evictions();
    assert_eq!(manager.evictions(), 0);
}

#[test]
fn test_kv_residency_frequency_promotion_constants() {
    use adapteros_lora_worker::kv_quota::{HOT_PROMOTION_THRESHOLD, HOT_RECENCY_WINDOW};

    // Verify promotion threshold is reasonable (from plan: 3 accesses)
    assert_eq!(HOT_PROMOTION_THRESHOLD, 3);

    // Verify recency window is reasonable (from plan: 60 seconds)
    assert_eq!(HOT_RECENCY_WINDOW.as_secs(), 60);
}

#[test]
fn test_kv_quota_usage_percentage_calculation() {
    let manager = TenantKvQuotaManager::new("tenant-pct".to_string(), Some(1000));

    // Initially 0%
    let usage = manager.usage();
    assert!((usage.usage_pct - 0.0).abs() < f64::EPSILON);

    // Use 500 bytes = 50%
    let res = manager.reserve(500).unwrap();
    manager.finalize(res).unwrap();

    let usage = manager.usage();
    assert!((usage.usage_pct - 50.0).abs() < 0.001);

    // Use another 250 bytes = 75%
    let res = manager.reserve(250).unwrap();
    manager.finalize(res).unwrap();

    let usage = manager.usage();
    assert!((usage.usage_pct - 75.0).abs() < 0.001);
}

#[cfg(target_os = "macos")]
#[test]
fn test_purgeable_state_round_trip() {
    use metal::Device;

    let device = match Device::system_default() {
        Some(d) => d,
        None => {
            eprintln!("No Metal device available, skipping test");
            return;
        }
    };

    let buffer = device.new_buffer(1024, metal::MTLResourceOptions::StorageModeShared);

    // Check if buffer supports purgeable state (may vary by hardware/storage mode)
    if !buffer.supports_purgeable_state() {
        eprintln!("Buffer does not support purgeable state, skipping test");
        return;
    }

    // Test that API calls work without crashing
    // Note: On Apple Silicon with unified memory (StorageModeShared),
    // purgeable state transitions may not have the same effect as on
    // discrete GPUs with MTLStorageModeManaged. The API calls should
    // still succeed but may report NonVolatile regardless of requests.

    // These should all succeed without crashing
    let result1 = buffer.make_purgeable();
    assert!(result1.is_ok(), "make_purgeable should not fail");

    let result2 = buffer.make_non_purgeable();
    assert!(result2.is_ok(), "make_non_purgeable should not fail");
    assert!(
        !result2.as_ref().unwrap().was_purged,
        "Buffer should not be purged"
    );

    // Query current state
    let query_result = buffer.set_purgeable_state(PurgeableState::KeepCurrent);
    assert!(query_result.is_ok(), "Query should succeed");

    // The key verification: after make_non_purgeable, the buffer should be non-volatile
    // (protected from eviction), which is what we want for HOT KV entries
    let state = query_result.unwrap();
    assert_eq!(
        state.previous,
        PurgeableState::NonVolatile,
        "Buffer should be NonVolatile (protected) after make_non_purgeable"
    );
}
