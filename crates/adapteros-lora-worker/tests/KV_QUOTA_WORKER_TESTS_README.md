# KV Quota Worker Integration Tests

## Overview

This test suite (`kv_quota_worker_integration.rs`) provides comprehensive worker-level integration tests for KV quota enforcement in the LORAX worker.

## Test Coverage

### 1. KV Cache with Quota Manager Integration
- ✅ Basic allocation with quota tracking
- ✅ Quota exceeded error handling
- ✅ Quota failure doesn't corrupt cache state
- ✅ Cumulative quota tracking across multiple allocations
- ✅ Quota reset between sessions
- ✅ Eviction counter tracking
- ✅ Reservation rollback on allocation failure

### 2. Residency Promotion Tests
- ✅ Frequency-based promotion (COLD -> HOT)
- ✅ Recency-based demotion (HOT -> COLD)
- ✅ Promotion threshold verification (3 accesses)
- ✅ Recency window validation (60 seconds)

### 3. Quota Usage Statistics
- ✅ Usage statistics calculation
- ✅ Percentage tracking
- ✅ Available bytes calculation
- ✅ Unlimited quota (None) handling

### 4. Mock Kernel Integration
- ✅ Mock inference with quota enforcement
- ✅ Sequential request quota tracking
- ✅ Cache coherence with quota preservation

### 5. Receipt Integration
- ✅ KV stats in receipt format
- ✅ Eviction counter in receipt
- ✅ Serialization for telemetry

### 6. Error Handling
- ✅ QuotaExceeded error messages
- ✅ Failure codes (KV_QUOTA_EXCEEDED)
- ✅ Reservation timeout cleanup

### 7. Stress Tests
- ✅ Rapid allocation/deallocation (100 iterations)
- ✅ Quota boundary conditions
- ✅ No memory leaks verification

## Test Patterns

### Basic Quota Test Pattern
```rust
let quota_manager = Arc::new(TenantKvQuotaManager::new(
    "tenant-id".to_string(),
    Some(4 * BYTES_PER_MB),
));

let mut cache = KvCache::new_with_quota(
    8 * BYTES_PER_MB,
    Some(quota_manager.clone()),
);

let seq_id = cache.allocate(128).expect("Allocation should succeed");
// ... perform operations ...
cache.free(seq_id).expect("Free should succeed");
```

### Quota Exceeded Test Pattern
```rust
let result = cache.allocate(large_size);
assert!(matches!(result, Err(AosError::QuotaExceeded { .. })));
```

### Receipt Integration Pattern
```rust
let kv_stats = quota_manager.usage();
let receipt_summary = format!(
    "KV Cache: {}/{} bytes ({:.1}%)",
    kv_stats.used_bytes,
    kv_stats.quota_bytes.unwrap_or(0),
    kv_stats.usage_pct
);
```

## Running Tests

### Run all KV quota worker tests:
```bash
cargo test -p adapteros-lora-worker --test kv_quota_worker_integration
```

### Run specific test:
```bash
cargo test -p adapteros-lora-worker --test kv_quota_worker_integration test_kv_cache_quota_manager_allocation
```

### Run with output:
```bash
cargo test -p adapteros-lora-worker --test kv_quota_worker_integration -- --nocapture
```

### Run stress tests only:
```bash
cargo test -p adapteros-lora-worker --test kv_quota_worker_integration test_stress
```

## Key Test Scenarios

### Scenario 1: Worker Enforces Quota During Inference
**Test:** `test_mock_inference_with_quota_enforcement`
- Simulates full inference request with quota manager
- Verifies allocation succeeds within quota
- Validates KV stats for receipt generation

### Scenario 2: Quota Failure Doesn't Corrupt KV Cache
**Test:** `test_quota_failure_no_cache_corruption`
- Allocates sequence successfully
- Attempts allocation that exceeds quota
- Verifies first allocation remains valid
- Confirms cache state integrity

### Scenario 3: Multiple Sequential Requests Respect Cumulative Quota
**Test:** `test_sequential_requests_quota_tracking`
- Issues multiple requests in sequence
- Tracks cumulative quota usage
- Verifies quota release on request completion
- Tests quota reuse after freeing

### Scenario 4: Quota Reset Between Sessions
**Test:** `test_quota_reset_between_sessions`
- Runs complete session lifecycle
- Frees all allocations
- Verifies quota returns to zero
- Validates fresh start for next session

### Scenario 5: Residency Promotion in Real Worker Context
**Tests:** `test_residency_promotion_frequency_threshold`, `test_residency_demotion_recency_window`
- Tracks adapter access patterns
- Validates promotion threshold (3 accesses)
- Verifies recency window (60 seconds)
- Tests COLD -> HOT -> COLD lifecycle

### Scenario 6: Receipt Includes KV Stats from Actual Inference
**Tests:** `test_receipt_kv_stats_integration`, `test_receipt_eviction_counter`
- Collects KV quota usage statistics
- Formats stats for receipt
- Includes eviction counters
- Validates serialization for telemetry

## Error Cases Tested

1. **KV_QUOTA_EXCEEDED**: Allocation exceeds tenant quota
2. **Cache corruption prevention**: Failed allocations don't affect existing state
3. **Reservation rollback**: Failed operations clean up reservations
4. **No memory leaks**: All allocations are properly tracked and freed

## Performance Considerations

- **Allocation size**: Tests use realistic token counts (32, 64, 128, 256 tokens)
- **Quota sizes**: Range from 1MB (tight) to 16MB (generous)
- **Stress iterations**: 100 rapid alloc/dealloc cycles
- **Boundary testing**: Fill quota to 90%+ capacity

## Integration Points

### With KV Cache (`kvcache.rs`)
- Uses `KvCache::new_with_quota()` constructor
- Calls `allocate()` and `free()` methods
- Validates `is_allocated()` state checks

### With Quota Manager (`kv_quota.rs`)
- Uses `TenantKvQuotaManager::new()`
- Calls `reserve()`, `finalize()`, `rollback()` lifecycle
- Queries `usage()` for statistics
- Tracks `evictions()` counter

### With Mock Kernels (`adapteros-lora-kernel-api`)
- Uses `MockKernels` for inference simulation
- Validates `IoBuffers` interaction
- Tests `RouterRing` integration

## Future Enhancements

1. **Full Worker Integration**: Tests using complete Worker instance (currently using mocks)
2. **Concurrent Quota Tests**: Multi-threaded quota enforcement
3. **Backend-Specific Tests**: CoreML/Metal/MLX quota behavior
4. **Policy Integration**: Quota policy enforcement tests
5. **Telemetry Validation**: End-to-end telemetry emission tests

## Maintenance Notes

- Tests use `Arc<TenantKvQuotaManager>` to match production usage
- All tests clean up allocations to prevent quota leaks
- Constants from `kv_quota.rs` are reused (HOT_PROMOTION_THRESHOLD, HOT_RECENCY_WINDOW)
- Error matching uses exact `AosError` variants for type safety

## Related Files

- `crates/adapteros-lora-worker/src/kv_quota.rs` - Quota manager implementation
- `crates/adapteros-lora-worker/src/kvcache.rs` - KV cache with quota integration
- `crates/adapteros-lora-worker/tests/worker_enforcement_tests.rs` - Related enforcement tests
- `tests/kv_residency_quota_integration.rs` - E2E quota tests (root level)

## Test Statistics

- **Total Tests**: 24
- **Lines of Code**: ~818
- **Coverage Areas**: 7 major categories
- **Mocking Strategy**: MockKernels for lightweight testing
- **Execution Time**: < 1 second for full suite
