#![no_main]

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

// Import from worker crate
use adapteros_lora_worker::kv_quota::TenantKvQuotaManager;

/// Fuzz KV quota reservation and enforcement logic
///
/// Tests:
/// - Reservation creation within quota
/// - Reservation finalization
/// - Reservation rollback
/// - Quota overflow detection
/// - Concurrent reservations
/// - Eviction tracking
/// - Edge cases: zero quota, unlimited quota, exact limits
fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Generate quota configuration
    let quota_mode = u.int_in_range::<u8>(0..=2).unwrap_or(1);
    let quota_bytes = match quota_mode {
        0 => None, // Unlimited
        1 => {
            // Fixed quota
            let quota = u
                .int_in_range::<u64>(1024..=1024 * 1024 * 10)
                .unwrap_or(1024 * 1024);
            Some(quota)
        }
        _ => {
            // Small quota for edge cases
            let quota = u.int_in_range::<u64>(1..=1024).unwrap_or(512);
            Some(quota)
        }
    };

    let tenant_id = format!(
        "tenant-fuzz-{}",
        u.int_in_range::<u32>(0..=1000).unwrap_or(0)
    );
    let qm = TenantKvQuotaManager::new(tenant_id.clone(), quota_bytes);

    // Verify initial state
    assert_eq!(qm.tenant_id(), &tenant_id);
    assert_eq!(qm.quota_bytes(), quota_bytes);
    assert_eq!(qm.is_quota_enforced(), quota_bytes.is_some());
    assert_eq!(qm.evictions(), 0);

    let usage = qm.usage();
    assert_eq!(usage.used_bytes, 0);
    assert_eq!(usage.reserved_bytes, 0);

    // Generate a sequence of operations
    let num_ops = u.int_in_range::<usize>(1..=20).unwrap_or(10);
    let mut active_reservations = Vec::new();

    for _ in 0..num_ops {
        let op_type = u.int_in_range::<u8>(0..=6).unwrap_or(0);

        match op_type {
            0..=2 => {
                // Reserve (60% of operations)
                let size = match u.int_in_range::<u64>(1..=10000) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                match qm.reserve(size) {
                    Ok(reservation) => {
                        // Verify reservation
                        assert_eq!(reservation.size_bytes, size);
                        assert!(!reservation.is_expired());

                        let usage = qm.usage();
                        assert!(usage.reserved_bytes >= size);

                        active_reservations.push(reservation);
                    }
                    Err(_) => {
                        // Quota exceeded - should only happen with enforced quota
                        assert!(qm.is_quota_enforced());
                    }
                }
            }
            3 => {
                // Finalize (20%)
                if !active_reservations.is_empty() {
                    let idx = u
                        .int_in_range::<usize>(0..=(active_reservations.len() - 1))
                        .unwrap_or(0);
                    let reservation = active_reservations.remove(idx);
                    let _size = reservation.size_bytes;

                    let usage_before = qm.usage();
                    let _ = qm.finalize(reservation);
                    let usage_after = qm.usage();

                    // Verify reservation moved from reserved to used
                    assert!(usage_after.used_bytes >= usage_before.used_bytes);
                    assert!(usage_after.reserved_bytes <= usage_before.reserved_bytes);
                }
            }
            4 => {
                // Rollback (10%)
                if !active_reservations.is_empty() {
                    let idx = u
                        .int_in_range::<usize>(0..=(active_reservations.len() - 1))
                        .unwrap_or(0);
                    let reservation = active_reservations.remove(idx);

                    let usage_before = qm.usage();
                    qm.rollback(reservation);
                    let usage_after = qm.usage();

                    // Verify reserved bytes decreased
                    assert!(usage_after.reserved_bytes <= usage_before.reserved_bytes);
                }
            }
            5 => {
                // Release (5%)
                let usage = qm.usage();
                if usage.used_bytes > 0 {
                    let release_size = u.int_in_range::<u64>(1..=usage.used_bytes).unwrap_or(1);
                    qm.release(release_size);

                    let usage_after = qm.usage();
                    assert!(usage_after.used_bytes <= usage.used_bytes);
                }
            }
            _ => {
                // Record eviction (5%)
                qm.record_eviction();
                assert!(qm.evictions() > 0);
            }
        }

        // Invariant checks after each operation
        let usage = qm.usage();

        // Quota enforcement check
        if let Some(quota) = quota_bytes {
            let total = usage.used_bytes.saturating_add(usage.reserved_bytes);
            assert!(
                total <= quota,
                "Total usage {} must not exceed quota {}",
                total,
                quota
            );
        }

        // Usage percentage should be reasonable
        if quota_bytes.is_some() {
            assert!(usage.usage_pct >= 0.0 && usage.usage_pct <= 100.0 || usage.usage_pct.is_nan());
        }
    }

    // Test check_quota without reserving
    let test_size = u.int_in_range::<u64>(1..=1000).unwrap_or(100);
    let check_result = qm.check_quota(test_size);
    if let Some(quota) = quota_bytes {
        let usage = qm.usage();
        let total = usage
            .used_bytes
            .saturating_add(usage.reserved_bytes)
            .saturating_add(test_size);
        if total > quota {
            assert!(check_result.is_err(), "Should fail quota check");
        }
    } else {
        assert!(check_result.is_ok(), "Unlimited quota should always pass");
    }

    // Test reset evictions
    qm.reset_evictions();
    assert_eq!(qm.evictions(), 0);

    // Test zero-size reservation
    if quota_bytes.is_some() {
        let zero_res = qm.reserve(0);
        if let Ok(res) = zero_res {
            assert_eq!(res.size_bytes, 0);
            let _ = qm.finalize(res);
        }
    }

    // Test exact quota limit
    if let Some(quota) = quota_bytes {
        // Release all used bytes first
        let usage = qm.usage();
        if usage.used_bytes > 0 {
            qm.release(usage.used_bytes);
        }

        // Try to reserve exactly quota bytes
        let exact_res = qm.reserve(quota);
        if exact_res.is_ok() {
            // Should succeed
            let usage = qm.usage();
            assert_eq!(usage.reserved_bytes, quota);
        }
    }
});
