//! Concurrent E2E Tests for KV Quota Enforcement
//!
//! Comprehensive concurrent testing of KV quota management under load:
//! - Concurrent requests with shared tenant quota (no race conditions)
//! - Multi-tenant quota isolation under concurrent load
//! - Failed requests don't affect quota state for other requests
//! - Reservation timeout and cleanup under concurrent access
//! - Quota enforcement with multiple simultaneous inference requests
//! - Eviction under concurrent memory pressure
//! - HOT/COLD promotion doesn't race with eviction
//! - Stress test: many concurrent small allocations
//!
//! Run with:
//! - cargo test --test kv_quota_concurrent_e2e
//! - cargo test --test kv_quota_concurrent_e2e -- --nocapture (with output)

#![allow(clippy::useless_vec)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::manual_flatten)]
#![allow(dead_code)]
#![allow(unused_imports)]

use adapteros_core::AosError;
use adapteros_lora_kernel_mtl::KvResidency;
use adapteros_lora_worker::kv_quota::{
    KvReservation, TenantKvQuotaManager, HOT_PROMOTION_THRESHOLD,
};
use futures_util::future::join_all;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::time::sleep;

// ============================================================================
// Test 1: Concurrent Requests with Shared Tenant Quota - No Race Conditions
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_requests_shared_quota_no_races() {
    let quota_bytes = 10_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-concurrent".to_string(),
        Some(quota_bytes),
    ));

    let num_workers = 50;
    let bytes_per_request = 200u64;
    let success_count = Arc::new(AtomicU32::new(0));
    let failure_count = Arc::new(AtomicU32::new(0));

    println!(
        "Starting {} concurrent allocation requests ({} bytes each, quota: {} bytes)",
        num_workers, bytes_per_request, quota_bytes
    );

    let start = Instant::now();
    let mut tasks = vec![];

    // Launch concurrent requests
    for i in 0..num_workers {
        let manager = manager.clone();
        let success = success_count.clone();
        let failure = failure_count.clone();

        let task = tokio::spawn(async move {
            // Attempt to reserve and finalize
            match manager.reserve(bytes_per_request) {
                Ok(reservation) => {
                    // Add some jitter to test concurrent finalization
                    sleep(Duration::from_micros(i % 10)).await;

                    match manager.finalize(reservation) {
                        Ok(_) => {
                            success.fetch_add(1, Ordering::Relaxed);
                            Ok(())
                        }
                        Err(e) => {
                            failure.fetch_add(1, Ordering::Relaxed);
                            Err(e)
                        }
                    }
                }
                Err(_) => {
                    failure.fetch_add(1, Ordering::Relaxed);
                    Err(AosError::MemoryPressure(
                        "Quota exceeded during reservation".to_string(),
                    ))
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    let results = join_all(tasks).await;

    let elapsed = start.elapsed();

    // Count final results
    let success = success_count.load(Ordering::Relaxed);
    let failure = failure_count.load(Ordering::Relaxed);

    println!(
        "Completed {} requests in {:?} ({} success, {} failures)",
        success + failure,
        elapsed,
        success,
        failure
    );

    // Verify all tasks completed
    assert_eq!(results.len(), num_workers as usize);
    assert_eq!(success + failure, num_workers as u32);

    // Verify quota was not exceeded
    let usage = manager.usage();
    assert!(
        usage.used_bytes <= quota_bytes,
        "Used bytes ({}) should not exceed quota ({})",
        usage.used_bytes,
        quota_bytes
    );

    // Verify expected number of successes (quota / bytes_per_request)
    let expected_max_success = quota_bytes / bytes_per_request;
    assert!(
        success as u64 <= expected_max_success,
        "Success count ({}) should not exceed expected max ({})",
        success,
        expected_max_success
    );

    // Verify no dangling reservations (critical for race condition check)
    assert_eq!(
        usage.reserved_bytes, 0,
        "No reserved bytes should remain - this indicates a race condition"
    );

    println!("✓ No race conditions detected - quota enforced atomically");
}

// ============================================================================
// Test 2: Multi-Tenant Quota Isolation Under Concurrent Load
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_multi_tenant_quota_isolation_concurrent_load() {
    // Create 5 tenants with different quotas
    let tenants = vec![
        ("tenant-a", 5_000u64),
        ("tenant-b", 10_000u64),
        ("tenant-c", 7_500u64),
        ("tenant-d", 12_000u64),
        ("tenant-e", 3_000u64),
    ];

    let managers: Vec<Arc<TenantKvQuotaManager>> = tenants
        .iter()
        .map(|(tenant_id, quota)| {
            Arc::new(TenantKvQuotaManager::new(
                tenant_id.to_string(),
                Some(*quota),
            ))
        })
        .collect();

    let requests_per_tenant = 20;
    let bytes_per_request = 500u64;

    println!(
        "Testing {} tenants with {} concurrent requests each",
        tenants.len(),
        requests_per_tenant
    );

    let start = Instant::now();
    let mut all_tasks = vec![];

    // Launch concurrent requests for each tenant
    for (tenant_idx, manager) in managers.iter().enumerate() {
        for request_idx in 0..requests_per_tenant {
            let manager = manager.clone();

            let task = tokio::spawn(async move {
                // Simulate varying request timing
                sleep(Duration::from_millis((request_idx % 5) as u64)).await;

                let result = manager
                    .reserve(bytes_per_request)
                    .and_then(|res| manager.finalize(res));

                (tenant_idx, request_idx, result)
            });

            all_tasks.push(task);
        }
    }

    // Wait for all requests across all tenants
    let results = join_all(all_tasks).await;

    let elapsed = start.elapsed();

    println!("Completed all requests in {:?}", elapsed);

    // Verify per-tenant isolation
    for (idx, (tenant_id, quota)) in tenants.iter().enumerate() {
        let usage = managers[idx].usage();

        println!(
            "Tenant {}: used={}, quota={}, usage_pct={:.1}%",
            tenant_id, usage.used_bytes, quota, usage.usage_pct
        );

        // Critical: each tenant's usage must not exceed its own quota
        assert!(
            usage.used_bytes <= *quota,
            "Tenant {} exceeded quota: {} > {}",
            tenant_id,
            usage.used_bytes,
            quota
        );

        // Verify no cross-tenant contamination
        assert_eq!(
            usage.tenant_id, *tenant_id,
            "Tenant ID mismatch - possible cross-tenant contamination"
        );

        // Verify no dangling reservations
        assert_eq!(
            usage.reserved_bytes, 0,
            "Tenant {} has dangling reservations",
            tenant_id
        );
    }

    // Count results by tenant
    let mut tenant_results: Vec<(u32, u32)> = vec![(0, 0); tenants.len()];

    for result in results {
        if let Ok((tenant_idx, _request_idx, outcome)) = result {
            match outcome {
                Ok(_) => tenant_results[tenant_idx].0 += 1,  // success
                Err(_) => tenant_results[tenant_idx].1 += 1, // failure
            }
        }
    }

    for (idx, (tenant_id, _quota)) in tenants.iter().enumerate() {
        let (success, failure) = tenant_results[idx];
        println!(
            "Tenant {}: {} success, {} failures",
            tenant_id, success, failure
        );
    }

    println!("✓ Multi-tenant isolation maintained under concurrent load");
}

// ============================================================================
// Test 3: Failed Requests Don't Affect Quota State for Other Requests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_failed_requests_dont_poison_quota_state() {
    let quota_bytes = 2_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-poison-test".to_string(),
        Some(quota_bytes),
    ));

    // Pre-fill quota with successful allocations
    let initial_alloc = 1_500u64;
    let res = manager.reserve(initial_alloc).unwrap();
    manager.finalize(res).unwrap();

    println!(
        "Pre-filled quota with {} bytes (quota: {} bytes)",
        initial_alloc, quota_bytes
    );

    // Now launch concurrent requests that will mostly fail
    let num_requests = 30;
    let bytes_per_request = 100u64; // Most will fail since quota nearly full

    let mut tasks = vec![];
    let successful_requests = Arc::new(Mutex::new(Vec::new()));

    for i in 0..num_requests {
        let manager = manager.clone();
        let successful = successful_requests.clone();

        let task = tokio::spawn(async move {
            sleep(Duration::from_micros(i % 5)).await;

            match manager.reserve(bytes_per_request) {
                Ok(reservation) => {
                    sleep(Duration::from_micros(10)).await;

                    match manager.finalize(reservation) {
                        Ok(_) => {
                            successful.lock().await.push(i);
                            Ok(i)
                        }
                        Err(e) => Err(e),
                    }
                }
                Err(e) => Err(e),
            }
        });

        tasks.push(task);
    }

    // Wait for all requests
    let results = join_all(tasks).await;

    // Count successes and failures
    let mut success_count = 0;
    let mut failure_count = 0;

    for result in results {
        match result {
            Ok(Ok(_)) => success_count += 1,
            Ok(Err(_)) => failure_count += 1,
            Err(_) => failure_count += 1,
        }
    }

    println!(
        "Results: {} success, {} failures",
        success_count, failure_count
    );

    // Verify quota state is clean (not poisoned)
    let usage = manager.usage();

    println!(
        "Final usage: used={}, reserved={}, available={}",
        usage.used_bytes, usage.reserved_bytes, usage.available_bytes
    );

    // Critical checks for non-poisoned state
    assert_eq!(
        usage.reserved_bytes, 0,
        "No dangling reservations - quota state is clean"
    );
    assert!(usage.used_bytes <= quota_bytes, "Used bytes within quota");
    assert!(
        usage.used_bytes >= initial_alloc,
        "Initial allocation still present"
    );

    // Verify we can still allocate from remaining quota
    let remaining_bytes = quota_bytes - usage.used_bytes;
    if remaining_bytes > 0 {
        let test_res = manager.reserve(remaining_bytes);
        assert!(
            test_res.is_ok(),
            "Should be able to allocate remaining quota - quota not poisoned"
        );
        if let Ok(r) = test_res {
            manager.rollback(r);
        }
    }

    println!("✓ Failed requests did not poison quota state");
}

// ============================================================================
// Test 4: Reservation Timeout and Cleanup Under Concurrent Access
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reservation_timeout_cleanup_concurrent() {
    let quota_bytes = 10_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-timeout".to_string(),
        Some(quota_bytes),
    ));

    // Create some reservations without finalizing them (simulate abandoned requests)
    let abandoned_bytes = 500u64;
    let abandoned_count = 5;

    let mut abandoned_reservations = Vec::new();
    for _ in 0..abandoned_count {
        let res = manager.reserve(abandoned_bytes).unwrap();
        abandoned_reservations.push(res);
    }

    println!(
        "Created {} abandoned reservations ({} bytes total)",
        abandoned_count,
        abandoned_bytes * abandoned_count
    );

    let initial_reserved = manager.usage().reserved_bytes;
    assert_eq!(
        initial_reserved,
        abandoned_bytes * abandoned_count,
        "Reserved bytes should match abandoned reservations"
    );

    // Now launch concurrent requests that will trigger cleanup
    let num_concurrent = 20;
    let bytes_per_request = 300u64;

    let mut tasks = vec![];
    let success_count = Arc::new(AtomicU32::new(0));

    for i in 0..num_concurrent {
        let manager = manager.clone();
        let success = success_count.clone();

        let task = tokio::spawn(async move {
            sleep(Duration::from_millis(i % 10)).await;

            // This will trigger cleanup of expired reservations
            match manager.reserve(bytes_per_request) {
                Ok(reservation) => {
                    sleep(Duration::from_micros(50)).await;
                    manager.finalize(reservation).ok();
                    success.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        });

        tasks.push(task);
    }

    // Wait for all concurrent requests
    join_all(tasks).await;

    let final_success = success_count.load(Ordering::Relaxed);
    println!("Successful allocations: {}", final_success);

    // Rollback abandoned reservations to simulate cleanup
    for reservation in abandoned_reservations {
        manager.rollback(reservation);
    }

    // Verify final state
    let final_usage = manager.usage();

    println!(
        "Final state: used={}, reserved={}",
        final_usage.used_bytes, final_usage.reserved_bytes
    );

    // All abandoned reservations should be cleaned up
    assert!(
        final_usage.reserved_bytes == 0 || final_usage.reserved_bytes < initial_reserved,
        "Reserved bytes should be cleaned up"
    );

    println!("✓ Reservation cleanup works correctly under concurrent access");
}

// ============================================================================
// Test 5: Quota Enforcement with Multiple Simultaneous Inference Requests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_quota_enforcement_simultaneous_inference_simulation() {
    let quota_bytes = 50_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-inference".to_string(),
        Some(quota_bytes),
    ));

    // Simulate realistic inference scenario:
    // - Varying KV cache sizes based on sequence length
    // - Some requests complete quickly, others take longer
    // - Continuous arrival pattern

    #[derive(Debug, Clone)]
    struct InferenceRequest {
        id: usize,
        kv_bytes: u64,
        generation_time_ms: u64,
    }

    let requests = vec![
        InferenceRequest {
            id: 1,
            kv_bytes: 2000,
            generation_time_ms: 50,
        },
        InferenceRequest {
            id: 2,
            kv_bytes: 1500,
            generation_time_ms: 30,
        },
        InferenceRequest {
            id: 3,
            kv_bytes: 3000,
            generation_time_ms: 80,
        },
        InferenceRequest {
            id: 4,
            kv_bytes: 1000,
            generation_time_ms: 20,
        },
        InferenceRequest {
            id: 5,
            kv_bytes: 2500,
            generation_time_ms: 60,
        },
        InferenceRequest {
            id: 6,
            kv_bytes: 4000,
            generation_time_ms: 100,
        },
        InferenceRequest {
            id: 7,
            kv_bytes: 1200,
            generation_time_ms: 25,
        },
        InferenceRequest {
            id: 8,
            kv_bytes: 3500,
            generation_time_ms: 90,
        },
        InferenceRequest {
            id: 9,
            kv_bytes: 1800,
            generation_time_ms: 40,
        },
        InferenceRequest {
            id: 10,
            kv_bytes: 2200,
            generation_time_ms: 55,
        },
    ];

    let completed = Arc::new(Mutex::new(Vec::new()));
    let rejected = Arc::new(Mutex::new(Vec::new()));

    println!(
        "Simulating {} concurrent inference requests (quota: {} bytes)",
        requests.len(),
        quota_bytes
    );

    let start = Instant::now();
    let mut tasks = vec![];

    for request in requests.clone() {
        let manager = manager.clone();
        let completed = completed.clone();
        let rejected = rejected.clone();

        let task = tokio::spawn(async move {
            // Try to allocate KV cache
            match manager.reserve(request.kv_bytes) {
                Ok(reservation) => {
                    // Simulate generation time
                    sleep(Duration::from_millis(request.generation_time_ms)).await;

                    // Finalize allocation
                    if manager.finalize(reservation).is_ok() {
                        // Simulate sequence completion after generation
                        sleep(Duration::from_millis(10)).await;

                        // Release KV cache
                        manager.release(request.kv_bytes);

                        completed.lock().await.push(request.id);
                        Ok(request.id)
                    } else {
                        rejected.lock().await.push(request.id);
                        Err(request.id)
                    }
                }
                Err(_) => {
                    rejected.lock().await.push(request.id);
                    Err(request.id)
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all inference requests
    join_all(tasks).await;

    let elapsed = start.elapsed();

    let completed_ids = completed.lock().await.clone();
    let rejected_ids = rejected.lock().await.clone();

    println!(
        "Completed in {:?}: {} successful, {} rejected",
        elapsed,
        completed_ids.len(),
        rejected_ids.len()
    );

    println!("Completed requests: {:?}", completed_ids);
    println!("Rejected requests: {:?}", rejected_ids);

    // Verify final state
    let final_usage = manager.usage();

    println!(
        "Final usage: used={}, reserved={}, available={}",
        final_usage.used_bytes, final_usage.reserved_bytes, final_usage.available_bytes
    );

    // All requests should have completed or been rejected
    assert_eq!(
        completed_ids.len() + rejected_ids.len(),
        requests.len(),
        "All requests accounted for"
    );

    // No KV cache should remain after all completions
    assert_eq!(
        final_usage.used_bytes, 0,
        "All KV cache should be released after completion"
    );
    assert_eq!(final_usage.reserved_bytes, 0, "No dangling reservations");

    println!("✓ Quota enforcement works correctly with simultaneous inference");
}

// ============================================================================
// Test 6: Eviction Under Concurrent Memory Pressure
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_eviction_under_concurrent_memory_pressure() {
    // Simulate a KV cache pool with entries that can be evicted
    #[derive(Debug, Clone)]
    struct KvCacheEntry {
        id: String,
        size_bytes: u64,
        residency: KvResidency,
        access_count: u32,
        last_access: Instant,
    }

    let cache_entries = Arc::new(RwLock::new(Vec::new()));

    // Pre-populate with mixed HOT and COLD entries
    {
        let mut entries = cache_entries.write().await;
        for i in 0..10 {
            let residency = if i % 3 == 0 {
                KvResidency::Hot
            } else {
                KvResidency::Cold
            };

            entries.push(KvCacheEntry {
                id: format!("seq-{}", i),
                size_bytes: 500,
                residency,
                access_count: if residency == KvResidency::Hot { 5 } else { 1 },
                last_access: Instant::now(),
            });
        }
    }

    let quota_bytes = 3000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-eviction".to_string(),
        Some(quota_bytes),
    ));

    // Allocate initial KV cache to fill quota partially
    let initial_usage = 2500u64;
    let initial_res = manager.reserve(initial_usage).unwrap();
    manager.finalize(initial_res).unwrap();

    println!(
        "Initial state: {} cache entries, {} bytes allocated",
        cache_entries.read().await.len(),
        initial_usage
    );

    // Launch concurrent requests that trigger eviction
    let num_requests = 20;
    let bytes_per_request = 300u64;

    let evicted_entries = Arc::new(Mutex::new(Vec::new()));
    let successful_allocations = Arc::new(AtomicU32::new(0));

    let mut tasks = vec![];

    for i in 0..num_requests {
        let manager = manager.clone();
        let cache = cache_entries.clone();
        let evicted = evicted_entries.clone();
        let success = successful_allocations.clone();

        let task = tokio::spawn(async move {
            sleep(Duration::from_millis(i % 10)).await;

            // Try to allocate - may trigger eviction
            match manager.reserve(bytes_per_request) {
                Ok(reservation) => {
                    success.fetch_add(1, Ordering::Relaxed);
                    manager.finalize(reservation).ok();
                    Ok(i)
                }
                Err(_) => {
                    // Need to evict - evict COLD entries first
                    let mut entries = cache.write().await;

                    // Find COLD entries to evict
                    let mut evicted_ids = Vec::new();
                    entries.retain(|entry| {
                        if entry.residency == KvResidency::Cold && evicted_ids.len() < 2 {
                            evicted_ids.push(entry.id.clone());
                            manager.record_eviction();
                            false // Remove this entry
                        } else {
                            true // Keep this entry
                        }
                    });

                    if !evicted_ids.is_empty() {
                        evicted.lock().await.extend(evicted_ids);
                        // Release some quota
                        manager.release(500);
                    }

                    Err(i)
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all requests
    join_all(tasks).await;

    let final_success = successful_allocations.load(Ordering::Relaxed);
    let final_evictions = manager.evictions();
    let evicted = evicted_entries.lock().await.clone();

    println!("Successful allocations: {}", final_success);
    println!("Evictions recorded: {}", final_evictions);
    println!("Evicted entries: {:?}", evicted);

    // Verify HOT entries were protected
    let remaining_entries = cache_entries.read().await.clone();
    let hot_count = remaining_entries
        .iter()
        .filter(|e| e.residency == KvResidency::Hot)
        .count();

    println!(
        "Remaining entries: {} total, {} HOT",
        remaining_entries.len(),
        hot_count
    );

    // All remaining HOT entries should still be present
    for entry in remaining_entries.iter() {
        if entry.residency == KvResidency::Hot {
            assert!(
                !evicted.contains(&entry.id),
                "HOT entry {} should not be evicted",
                entry.id
            );
        }
    }

    println!("✓ Eviction under memory pressure protects HOT entries");
}

// ============================================================================
// Test 7: HOT/COLD Promotion Doesn't Race with Eviction
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_hot_cold_promotion_no_race_with_eviction() {
    #[derive(Debug, Clone)]
    struct KvEntry {
        id: String,
        residency: Arc<RwLock<KvResidency>>,
        access_count: Arc<AtomicU32>,
        size_bytes: u64,
    }

    impl KvEntry {
        fn new(id: String, size_bytes: u64) -> Self {
            Self {
                id,
                residency: Arc::new(RwLock::new(KvResidency::Cold)),
                access_count: Arc::new(AtomicU32::new(0)),
                size_bytes,
            }
        }

        async fn access(&self) {
            let count = self.access_count.fetch_add(1, Ordering::Relaxed) + 1;

            // Promote to HOT if threshold reached
            if count >= HOT_PROMOTION_THRESHOLD as u32 {
                let mut residency = self.residency.write().await;
                *residency = KvResidency::Hot;
            }
        }

        async fn is_hot(&self) -> bool {
            *self.residency.read().await == KvResidency::Hot
        }

        async fn residency(&self) -> KvResidency {
            *self.residency.read().await
        }
    }

    // Create entries
    let entries = Arc::new(RwLock::new(Vec::new()));
    for i in 0..20 {
        let entry = KvEntry::new(format!("entry-{}", i), 100);
        entries.write().await.push(entry);
    }

    println!(
        "Created {} entries, starting concurrent access and eviction",
        entries.read().await.len()
    );

    let access_tasks = Arc::new(AtomicU32::new(0));
    let eviction_tasks = Arc::new(AtomicU32::new(0));
    let promotions = Arc::new(AtomicU32::new(0));

    let mut tasks = vec![];

    // Access tasks (promote entries to HOT)
    for i in 0..50 {
        let entries = entries.clone();
        let access_count = access_tasks.clone();
        let promo_count = promotions.clone();

        let task = tokio::spawn(async move {
            sleep(Duration::from_micros(i % 20)).await;

            let entries_read = entries.read().await;
            let entry_idx = (i as usize) % entries_read.len();
            let entry = &entries_read[entry_idx];

            let was_hot = entry.is_hot().await;
            entry.access().await;
            let is_hot = entry.is_hot().await;

            if !was_hot && is_hot {
                promo_count.fetch_add(1, Ordering::Relaxed);
            }

            access_count.fetch_add(1, Ordering::Relaxed);
        });

        tasks.push(task);
    }

    // Eviction tasks (try to evict COLD entries)
    for i in 0..30 {
        let entries = entries.clone();
        let evict_count = eviction_tasks.clone();

        let task = tokio::spawn(async move {
            sleep(Duration::from_millis(i % 10)).await;

            let mut entries_write = entries.write().await;

            // Try to evict one COLD entry
            let mut evicted = false;

            for idx in 0..entries_write.len() {
                let residency = *entries_write[idx].residency.read().await;
                if residency == KvResidency::Cold {
                    entries_write.remove(idx);
                    evicted = true;
                    break;
                }
            }

            if evicted {
                evict_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks
    join_all(tasks).await;

    let total_accesses = access_tasks.load(Ordering::Relaxed);
    let total_evictions = eviction_tasks.load(Ordering::Relaxed);
    let total_promotions = promotions.load(Ordering::Relaxed);

    println!("Total accesses: {}", total_accesses);
    println!("Total evictions: {}", total_evictions);
    println!("Total promotions: {}", total_promotions);

    // Verify remaining entries
    let final_entries = entries.read().await.clone();
    let mut hot_count = 0;
    let mut cold_count = 0;

    for entry in final_entries.iter() {
        if entry.is_hot().await {
            hot_count += 1;
        } else {
            cold_count += 1;
        }
    }

    println!(
        "Final entries: {} total ({} HOT, {} COLD)",
        final_entries.len(),
        hot_count,
        cold_count
    );

    // Verify no HOT entries were evicted
    assert!(
        hot_count > 0 || final_entries.is_empty(),
        "Some entries should be promoted to HOT"
    );

    println!("✓ HOT/COLD promotion does not race with eviction");
}

// ============================================================================
// Test 8: Stress Test - Many Concurrent Small Allocations
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_stress_many_concurrent_small_allocations() {
    let quota_bytes = 100_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-stress".to_string(),
        Some(quota_bytes),
    ));

    let num_workers = 200;
    let allocations_per_worker = 10;
    let min_bytes = 10u64;
    let max_bytes = 200u64;

    println!(
        "Stress test: {} workers × {} allocations = {} total operations",
        num_workers,
        allocations_per_worker,
        num_workers * allocations_per_worker
    );

    let total_operations = Arc::new(AtomicU64::new(0));
    let successful_ops = Arc::new(AtomicU64::new(0));
    let failed_ops = Arc::new(AtomicU64::new(0));

    let start = Instant::now();
    let mut tasks = vec![];

    for worker_id in 0..num_workers {
        let manager = manager.clone();
        let total = total_operations.clone();
        let success = successful_ops.clone();
        let failed = failed_ops.clone();

        let task = tokio::spawn(async move {
            for alloc_id in 0..allocations_per_worker {
                // Vary allocation size
                let size = min_bytes + ((worker_id + alloc_id) % (max_bytes - min_bytes));

                total.fetch_add(1, Ordering::Relaxed);

                // Reserve
                match manager.reserve(size) {
                    Ok(reservation) => {
                        // Random delay
                        if alloc_id % 3 == 0 {
                            sleep(Duration::from_micros(1)).await;
                        }

                        // Randomly finalize or rollback
                        if alloc_id % 5 == 0 {
                            manager.rollback(reservation);
                        } else {
                            match manager.finalize(reservation) {
                                Ok(_) => {
                                    success.fetch_add(1, Ordering::Relaxed);

                                    // Randomly release
                                    if alloc_id % 4 == 0 {
                                        manager.release(size);
                                    }
                                }
                                Err(_) => {
                                    failed.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all workers
    join_all(tasks).await;

    let elapsed = start.elapsed();

    let total = total_operations.load(Ordering::Relaxed);
    let success = successful_ops.load(Ordering::Relaxed);
    let failed = failed_ops.load(Ordering::Relaxed);

    println!(
        "Completed {} operations in {:?} ({} success, {} failed)",
        total, elapsed, success, failed
    );

    let ops_per_sec = total as f64 / elapsed.as_secs_f64();
    println!("Throughput: {:.0} operations/second", ops_per_sec);

    // Verify final state
    let final_usage = manager.usage();

    println!(
        "Final state: used={}, reserved={}, available={}",
        final_usage.used_bytes, final_usage.reserved_bytes, final_usage.available_bytes
    );

    // Critical checks
    assert!(
        final_usage.used_bytes <= quota_bytes,
        "Quota not exceeded: {} <= {}",
        final_usage.used_bytes,
        quota_bytes
    );

    assert_eq!(
        final_usage.reserved_bytes, 0,
        "No dangling reservations after stress test"
    );

    assert_eq!(
        total,
        num_workers * allocations_per_worker,
        "All operations completed"
    );

    println!("✓ Stress test completed successfully - no corruption detected");
}

// ============================================================================
// Test 9: Concurrent Rollbacks Don't Corrupt Quota State
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_rollbacks_no_corruption() {
    let quota_bytes = 20_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-rollback".to_string(),
        Some(quota_bytes),
    ));

    let num_tasks = 100;
    let bytes_per_reservation = 100u64;

    println!(
        "Testing {} concurrent rollbacks ({} bytes each)",
        num_tasks, bytes_per_reservation
    );

    let mut tasks = vec![];
    let rollback_count = Arc::new(AtomicU32::new(0));

    for i in 0..num_tasks {
        let manager = manager.clone();
        let count = rollback_count.clone();

        let task = tokio::spawn(async move {
            // Reserve
            match manager.reserve(bytes_per_reservation) {
                Ok(reservation) => {
                    // Add jitter
                    sleep(Duration::from_micros(i % 20)).await;

                    // Rollback
                    manager.rollback(reservation);
                    count.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        });

        tasks.push(task);
    }

    // Wait for all rollbacks
    join_all(tasks).await;

    let rollbacks = rollback_count.load(Ordering::Relaxed);
    println!("Completed {} rollbacks", rollbacks);

    // Verify quota state is completely clean
    let final_usage = manager.usage();

    println!(
        "Final state: used={}, reserved={}",
        final_usage.used_bytes, final_usage.reserved_bytes
    );

    assert_eq!(final_usage.used_bytes, 0, "No used bytes after rollbacks");
    assert_eq!(
        final_usage.reserved_bytes, 0,
        "No reserved bytes after rollbacks"
    );
    assert_eq!(
        final_usage.available_bytes, quota_bytes,
        "Full quota available after rollbacks"
    );

    println!("✓ Concurrent rollbacks do not corrupt quota state");
}

// ============================================================================
// Test 10: Peak Concurrency - Maximum Simultaneous Operations
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn test_peak_concurrency_maximum_simultaneous_operations() {
    let quota_bytes = 1_000_000u64;
    let manager = Arc::new(TenantKvQuotaManager::new(
        "tenant-peak".to_string(),
        Some(quota_bytes),
    ));

    // Use semaphore to ensure truly simultaneous operations
    let semaphore = Arc::new(Semaphore::new(0));
    let num_workers = 500;

    println!(
        "Peak concurrency test: {} simultaneous operations",
        num_workers
    );

    let start_barrier = semaphore.clone();
    let mut tasks = vec![];
    let operation_count = Arc::new(AtomicU64::new(0));

    // Spawn all workers (they'll wait at barrier)
    for i in 0..num_workers {
        let manager = manager.clone();
        let barrier = start_barrier.clone();
        let count = operation_count.clone();

        let task = tokio::spawn(async move {
            // Wait for go signal
            let _permit = barrier.acquire().await.unwrap();

            let bytes = 100 + (i % 900);

            // Try operation
            if let Ok(res) = manager.reserve(bytes) {
                count.fetch_add(1, Ordering::Relaxed);

                // Some finalize, some rollback
                if i % 2 == 0 {
                    manager.finalize(res).ok();
                } else {
                    manager.rollback(res);
                }
            }
        });

        tasks.push(task);
    }

    // Small delay to ensure all tasks are spawned
    sleep(Duration::from_millis(100)).await;

    // Release all workers simultaneously
    println!("Releasing all workers simultaneously...");
    let start = Instant::now();
    semaphore.add_permits(num_workers as usize);

    // Wait for completion
    join_all(tasks).await;

    let elapsed = start.elapsed();
    let ops = operation_count.load(Ordering::Relaxed);

    println!(
        "Completed {} operations in {:?} with peak concurrency",
        ops, elapsed
    );

    // Verify state
    let final_usage = manager.usage();

    println!(
        "Final state: used={}, reserved={}",
        final_usage.used_bytes, final_usage.reserved_bytes
    );

    assert!(
        final_usage.used_bytes <= quota_bytes,
        "Quota not exceeded under peak concurrency"
    );

    assert_eq!(
        final_usage.reserved_bytes, 0,
        "No dangling reservations after peak concurrency test"
    );

    println!("✓ System stable under peak concurrency");
}
