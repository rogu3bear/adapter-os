# KV Cache Coherence with Adapter Hot-Swap

## Overview

This document describes the KV cache coherence mechanism implemented to prevent non-deterministic behavior when adapters are hot-swapped during inference.

## Problem Statement

When adapters are hot-swapped (generation counter changes), the KV cache may contain stale computations from the previous adapter state. This leads to:

1. **Non-deterministic behavior**: Different outputs for the same input depending on when the swap occurred
2. **Incorrect inference**: Cached attention states computed with old adapters used with new adapters
3. **Difficult debugging**: Subtle bugs that only manifest during concurrent inference and hot-swap operations

## Solution: Generation-Tracked Cache Coherence

### Components

#### 1. SequenceGuard (RAII Type)

```rust
pub struct SequenceGuard {
    /// Sequence ID being guarded
    pub sequence_id: SequenceId,
    /// Stack generation captured at sequence start
    pub generation: u64,
    /// Whether the guard is still active
    active: bool,
}
```

The `SequenceGuard` captures the adapter stack generation at sequence allocation time and tracks it throughout the sequence lifetime.

#### 2. Generation Tracking in KvCache

The `KvCache` now includes:

```rust
/// Current adapter stack generation
stack_generation: u64,
/// Active sequence guards
active_guards: HashMap<SequenceId, u64>,
```

#### 3. Coherence Checking

```rust
pub fn ensure_cache_coherence(&mut self, current_generation: u64) -> Result<bool> {
    if current_generation != self.stack_generation {
        // Reset cache to ensure coherence
        self.reset_all();
        self.stack_generation = current_generation;
        Ok(true)
    } else {
        Ok(false)
    }
}
```

### Integration with Hot-Swap

The coherence mechanism integrates with the existing RCU hot-swap infrastructure:

```rust
// In Worker::infer_internal()

// 1. Snapshot current stack generation
let stack_handle = self.hotswap.table().get_current_stack_handle();
let current_generation = stack_handle.generation;

// 2. Ensure cache coherence before inference
{
    let mut kv_cache = self.kv_cache.lock().unwrap();
    if let Ok(reset) = kv_cache.ensure_cache_coherence(current_generation) {
        if reset {
            tracing::info!(
                old_generation = kv_cache.generation(),
                new_generation = current_generation,
                "KV cache reset due to adapter stack generation change"
            );
        }
    }
}

// 3. Proceed with inference using coherent cache
```

## API Usage

### Basic Usage

```rust
use adapteros_lora_worker::kvcache::KvCache;

let mut cache = KvCache::new(1024 * 1024);

// Set initial generation
cache.set_generation(1);

// Allocate sequence with guard
let guard = cache.allocate_with_guard(128, 1)?;

// On generation change, cache is automatically reset
cache.ensure_cache_coherence(2)?; // Resets if generation changed
```

### Draining Active Sequences Before Hot-Swap

For graceful adapter swaps, drain active sequences first:

```rust
// Check if swap would invalidate active sequences
if cache.has_active_sequences_with_generation(new_generation) {
    // Drain all active sequences
    let drained_count = cache.drain_active_sequences();
    tracing::info!("Drained {} sequences before hot-swap", drained_count);
}

// Now safe to swap adapters
hotswap.swap(&add_ids, &remove_ids).await?;
```

### Integration with Worker

The `Worker` automatically ensures coherence on every inference:

```rust
impl<K: FusedKernels + Send + Sync> Worker<K> {
    async fn infer_internal(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        // Coherence is automatically checked here
        let stack_handle = self.hotswap.table().get_current_stack_handle();
        let current_generation = stack_handle.generation;

        let mut kv_cache = self.kv_cache.lock().unwrap();
        kv_cache.ensure_cache_coherence(current_generation)?;

        // ... proceed with inference
    }
}
```

## Thread Safety

The coherence mechanism is thread-safe through:

1. **Arc<Mutex<KvCache>>**: The KV cache is wrapped in `Arc<StdMutex<>>` for interior mutability
2. **RCU Hot-Swap**: Generation changes are coordinated through the existing RCU mechanism
3. **Atomic Generation Counter**: The `AdapterTable::current_stack` uses `AtomicUsize` for generation tracking

## Performance Impact

### Overhead

- **Coherence check**: O(1) generation comparison per inference
- **Cache reset**: Only occurs on generation change (rare in production)
- **Guard tracking**: O(1) HashMap operations for allocation/free

### Optimization Opportunities

1. **Batch draining**: Group sequence draining during maintenance windows
2. **Lazy reset**: Defer cache reset until first allocation after generation change
3. **Partial invalidation**: Reset only affected cache regions (future work)

## Testing

Comprehensive tests validate the coherence mechanism:

```rust
#[test]
fn test_cache_coherence_reset() {
    let mut cache = KvCache::new(4 * 1024 * 1024);
    cache.set_generation(1);

    let _seq_id = cache.allocate(128).expect("Allocation should succeed");
    assert_eq!(cache.active_sequences(), 1);

    // Same generation - no reset
    let reset = cache.ensure_cache_coherence(1).unwrap();
    assert!(!reset);
    assert_eq!(cache.active_sequences(), 1);

    // Different generation - should reset
    let reset = cache.ensure_cache_coherence(2).unwrap();
    assert!(reset);
    assert_eq!(cache.active_sequences(), 0);
}
```

## Future Enhancements

1. **Determinism Report Integration**: Include cache coherence state in `DeterminismReport`
2. **Request-Scoped Guards**: Extend guards to track full request lifecycle
3. **Cache Versioning**: Track cache version separately from stack generation
4. **Partial Invalidation**: Invalidate only cache entries affected by swapped adapters

## Related Files

- `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-worker/src/kvcache.rs` - KV cache implementation
- `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-worker/src/adapter_hotswap.rs` - Hot-swap mechanism
- `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-worker/src/lib.rs` - Worker integration

## References

- [docs/ARCHITECTURE_PATTERNS.md](/Users/mln-dev/Dev/adapter-os/docs/ARCHITECTURE_PATTERNS.md) - RCU hot-swap pattern
- [docs/DETERMINISTIC_EXECUTION.md](/Users/mln-dev/Dev/adapter-os/docs/DETERMINISTIC_EXECUTION.md) - Determinism guarantees
- [CLAUDE.md](/Users/mln-dev/Dev/adapter-os/CLAUDE.md) - Core standards and patterns
