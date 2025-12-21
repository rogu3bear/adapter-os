# Runtime Resilience Patch Plan

## Overview

This plan addresses the critical runtime issues identified in the architectural review:
1. No circuit breaker for worker health tracking
2. O(n×m) allowlist validation on every request (no caching)
3. Model cache can exceed memory limits (no eviction enforcement)
4. Fixed 60s timeout regardless of model size/token budget

---

## Patch 1: Worker Health Circuit Breaker

**Problem**: Control plane routes to slow/failing workers indefinitely. No backoff, no health tracking.

**Location**: `crates/adapteros-server-api/src/`

**Solution**: Add `WorkerHealthTracker` with circuit breaker pattern.

```rust
// New file: worker_health.rs
pub struct WorkerHealthTracker {
    workers: RwLock<HashMap<String, WorkerHealth>>,
    circuit_breaker_threshold: u32,      // failures before open
    circuit_breaker_recovery_secs: u64,  // time before half-open
}

pub struct WorkerHealth {
    pub consecutive_failures: u32,
    pub last_failure: Option<Instant>,
    pub last_success: Option<Instant>,
    pub circuit_state: CircuitState,
    pub avg_latency_ms: f64,
}

pub enum CircuitState {
    Closed,      // normal operation
    Open,        // failing, don't route
    HalfOpen,    // testing recovery
}
```

**Integration**:
- In `select_worker_for_tenant()`: skip workers with `Open` circuit
- After UDS call: `tracker.record_success(worker_id)` or `tracker.record_failure(worker_id)`
- Exponential backoff on retries

---

## Patch 2: Tenant Allowlist Cache

**Problem**: `adapter_allowlist_for_tenant()` hits DB on every inference request.

**Location**: `crates/adapteros-server-api/src/inference_core.rs`

**Solution**: Add in-memory cache with TTL.

```rust
// Add to AppState or InferenceCore
pub struct AllowlistCache {
    cache: RwLock<HashMap<String, CachedAllowlist>>,
    ttl: Duration,
}

struct CachedAllowlist {
    adapter_ids: HashSet<String>,
    cached_at: Instant,
}

impl AllowlistCache {
    pub async fn get_or_fetch(&self, tenant_id: &str, db: &Database) -> Result<HashSet<String>> {
        // Check cache first
        if let Some(cached) = self.get_if_valid(tenant_id) {
            return Ok(cached);
        }
        // Fetch from DB and cache
        let allowlist = db.adapter_allowlist_for_tenant(tenant_id).await?;
        self.insert(tenant_id, allowlist.clone());
        Ok(allowlist)
    }
}
```

**TTL**: 30 seconds (balances freshness vs DB load)

---

## Patch 3: Model Cache Memory Enforcement

**Problem**: `get_or_load()` can exceed `max_memory_bytes` without eviction.

**Location**: `crates/adapteros-lora-worker/src/model_handle_cache.rs`

**Solution**: Add pre-load eviction check.

```rust
impl ModelHandleCache {
    pub fn get_or_load<F>(&self, key: &ModelKey, loader: F) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        // Existing: check cache first
        if let Some(entry) = self.cache.read().get(key) {
            return Ok(entry.handle.clone());
        }

        // NEW: Estimate memory needed and evict if necessary
        let estimated_size = self.estimate_model_size(key);
        self.ensure_headroom(estimated_size)?;

        // Existing: load model
        let (handle, actual_size) = loader()?;

        // NEW: Verify we didn't exceed limit after load
        if self.total_memory_bytes() + actual_size > self.max_memory_bytes {
            // Evict LRU entries until we have headroom
            self.evict_until_headroom(actual_size)?;
        }

        self.insert(key, handle, actual_size);
        Ok(handle)
    }

    fn evict_until_headroom(&self, needed_bytes: u64) -> Result<()> {
        let mut cache = self.cache.write();
        let mut evicted = 0u64;

        // Sort by last access time (LRU)
        let mut entries: Vec<_> = cache.iter().collect();
        entries.sort_by_key(|(_, e)| e.loaded_at);

        for (key, entry) in entries {
            if self.is_pinned(key) || self.is_active(key) {
                continue; // Skip protected entries
            }
            evicted += entry.memory_bytes;
            cache.remove(key);

            if self.total_memory_bytes() - evicted + needed_bytes <= self.max_memory_bytes {
                break;
            }
        }

        if self.total_memory_bytes() - evicted + needed_bytes > self.max_memory_bytes {
            return Err(AosError::ResourceExhausted("Model cache at capacity"));
        }
        Ok(())
    }
}
```

---

## Patch 4: Adaptive Timeout Calculation

**Problem**: Fixed 60s timeout for all inference regardless of model size or token budget.

**Location**: `crates/adapteros-server-api/src/inference_core.rs` line 847

**Solution**: Calculate timeout based on request parameters.

```rust
fn calculate_inference_timeout(
    max_tokens: u32,
    is_replay: bool,
    estimated_model_params: u64, // in billions, e.g., 7 for 7B
) -> Duration {
    // Base latency per token (varies by model size)
    let ms_per_token = match estimated_model_params {
        0..=3 => 10,      // Small models: ~10ms/token
        4..=13 => 25,     // Medium models: ~25ms/token
        14..=34 => 50,    // Large models: ~50ms/token
        _ => 100,         // XL models: ~100ms/token
    };

    // Calculate expected generation time
    let generation_ms = (max_tokens as u64) * ms_per_token;

    // Add fixed overhead for prompt processing, routing, etc.
    let overhead_ms = 5000; // 5s overhead

    // Safety multiplier (3x for variance)
    let timeout_ms = (generation_ms + overhead_ms) * 3;

    // Apply bounds
    let min_timeout = if is_replay { 30_000 } else { 15_000 }; // 15-30s minimum
    let max_timeout = if is_replay { 300_000 } else { 180_000 }; // 3-5min maximum

    Duration::from_millis(timeout_ms.clamp(min_timeout, max_timeout))
}
```

---

## Execution Order

1. **Worker Health Circuit Breaker** (highest impact)
   - Create `worker_health.rs`
   - Integrate into `InferenceCore`
   - Add metrics/telemetry

2. **Allowlist Cache** (quick win)
   - Add `AllowlistCache` struct
   - Modify `validate_ids_against_allowlist()` to use cache

3. **Model Cache Memory Enforcement** (prevents OOM)
   - Add `evict_until_headroom()` method
   - Modify `get_or_load()` to call it

4. **Adaptive Timeout** (improves UX)
   - Add `calculate_inference_timeout()` function
   - Replace fixed 60/120s with calculated value

---

## Testing Plan

1. **Unit tests** for each new component
2. **Integration test**: Simulate slow worker, verify circuit opens
3. **Load test**: 1000 adapters, verify allowlist cache reduces DB load
4. **Memory test**: Load models to capacity, verify eviction works
5. **Timeout test**: Verify small model gets short timeout, large model gets long timeout

---

## Rollback Plan

Each patch is independent. If issues arise:
1. Circuit breaker: Set threshold to u32::MAX (never opens)
2. Allowlist cache: Set TTL to 0 (always fetch from DB)
3. Memory enforcement: Set max_memory to u64::MAX (never evict)
4. Adaptive timeout: Replace with original fixed values

---

## Implementation Status

### ✅ Patch 1: Worker Health Circuit Breaker - COMPLETED

Existing `WorkerHealthMonitor` in `state.rs` (lines 450-580) already implements:
- `record_response(worker_id, latency_ms)` - tracks successes and latencies
- `record_failure(worker_id, error_msg)` - tracks failures
- Circuit breaker state management with configurable thresholds

**Integration added** in `inference_core.rs` (line ~920):
- Recording success/failure after each UDS call
- Latency tracking for worker health decisions

### ✅ Patch 2: Tenant Allowlist Cache - COMPLETED

**Location**: `state.rs` (lines 75-120)

Added `AllowlistCache` struct with:
- `HashMap<String, CachedAllowlist>` cache storage
- Configurable TTL (default: 30s)
- `get()` - returns cached allowlist if valid
- `insert()` - caches new allowlist with timestamp

**Integration** in `state.rs`:
- `adapter_allowlist_for_tenant()` now checks cache first
- Falls back to DB query on miss
- Auto-caches results

### ✅ Patch 3: Model Cache Memory Enforcement - ALREADY IMPLEMENTED

**Location**: `crates/adapteros-lora-worker/src/model_handle_cache.rs`

Existing implementation already provides:
- `evict_for_size_locked()` - LRU eviction before insertion (line 610)
- Pinned entry protection (line 627)
- Active entry protection (line 628-629)
- Budget enforcement with error reporting (line 702-720)
- Comprehensive test coverage (lines 929-1723)

### ✅ Patch 4: Adaptive Timeout Calculation - COMPLETED

**Location**: `inference_core.rs` (lines 206-245)

Added `calculate_inference_timeout()` function:
- Model size awareness (0-3B, 4-13B, 14-34B, 35B+)
- Token count scaling
- Replay request multiplier
- Configurable bounds via `InferenceConfig`

**Config added** in `config.rs` (lines 329-356):
- `InferenceConfig` struct with:
  - `model_params_billions: Option<u32>`
  - `min_timeout_ms: u64` (default: 15000)
  - `max_timeout_ms: u64` (default: 180000)
  - `replay_timeout_multiplier: f64` (default: 2.0)

**Integration** in `state.rs`:
- Added `inference: InferenceConfig` to `ApiConfig`

---

## Summary

All four patches from the runtime resilience plan have been implemented:

| Patch | Status | Files Modified |
|-------|--------|----------------|
| Circuit Breaker | ✅ Integrated | `inference_core.rs` |
| Allowlist Cache | ✅ Implemented | `state.rs` |
| Memory Enforcement | ✅ Existing | `model_handle_cache.rs` |
| Adaptive Timeout | ✅ Implemented | `inference_core.rs`, `config.rs`, `state.rs` |

**Implementation Date**: 2024-12-15
