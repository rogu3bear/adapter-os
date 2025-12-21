# Unified Memory Management - Quick Reference

**Version:** 1.0
**Date:** 2025-11-19

## Quick Start

```rust
use adapteros_memory::{
    BackendType, BufferPool, BufferPoolConfig,
    MemoryLimits, MemoryPressureManager, UnifiedMemoryTracker,
};
use std::sync::Arc;

// Setup
let limits = MemoryLimits::new(8_000_000_000, 16_000_000_000, 0.15);
let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
let manager = MemoryPressureManager::new(Arc::clone(&tracker));
let pool = BufferPool::new(BufferPoolConfig::default());

// Track adapter
tracker.track_adapter(1, BackendType::Metal, buffer_bytes, kv_cache_bytes);

// Check pressure
let report = manager.check_and_handle_pressure()?;

// Use buffer pool
let buffer = pool.acquire_buffer(size)?;
// ... use buffer ...
pool.release_buffer(buffer);
```

## Common Patterns

### Track Multi-Backend Adapter
```rust
// Metal allocation
tracker.track_adapter(1, BackendType::Metal, 32_000_000, 16_000_000);

// CoreML allocation for same adapter
tracker.track_adapter(1, BackendType::CoreML, 8_000_000, 0);

// Query total memory for adapter
let total = tracker.get_adapter_memory(1); // Returns 56 MB
```

### Pin Critical Adapter
```rust
// Pin production adapter
manager.pin_adapter(base_model_id);

// Unpin when done
manager.unpin_adapter(base_model_id);

// Check if pinned
if manager.is_pinned(adapter_id) {
    println!("Adapter is protected from eviction");
}
```

### Manual Eviction
```rust
let pinned = vec![1, 2]; // Protect these
let candidates = tracker.get_eviction_candidates(&pinned);

for (id, backend, bytes, priority) in candidates {
    if priority < f32::MAX {
        tracker.untrack_adapter(id);
        println!("Evicted {} from {} ({} bytes)", id, backend.as_str(), bytes);
    }
}
```

### Tensor Conversion
```rust
use adapteros_memory::TensorFormat;

let coreml_data = pool.convert_tensor_format(
    metal_data,
    TensorFormat::Metal,
    TensorFormat::CoreML,
    (height, width, channels)
)?;
```

### GPU Integrity Verification
```rust
use adapteros_memory::GpuBufferFingerprint;

// Store fingerprint on load
let fp = GpuBufferFingerprint::new(size, first_4kb, last_4kb, mid_4kb);
tracker.store_fingerprint(adapter_id, fp);

// Verify before inference
let current = GpuBufferFingerprint::new(size, first_4kb, last_4kb, mid_4kb);
tracker.verify_fingerprint(adapter_id, &current)?;
```

### Memory Statistics
```rust
let stats = manager.get_stats();
println!("Memory: {} MB / {} MB",
    stats.total_memory_used / 1_000_000,
    limits.max_vram / 1_000_000
);
println!("Headroom: {:.1}%", stats.headroom_pct);
println!("Pressure: {:?}", stats.pressure_level);
```

## API Cheat Sheet

### UnifiedMemoryTracker
```rust
tracker.track_adapter(id, backend, buffer_bytes, kv_cache_bytes)
tracker.untrack_adapter(id) -> Option<u64>
tracker.get_total_memory() -> u64
tracker.get_backend_memory(backend) -> u64
tracker.get_adapter_memory(id) -> u64
tracker.get_eviction_candidates(&pinned) -> Vec<(u32, BackendType, u64, f32)>
tracker.check_memory_pressure() -> MemoryPressure
tracker.store_fingerprint(id, fingerprint)
tracker.verify_fingerprint(id, fingerprint) -> Result<bool>
tracker.check_memory_footprint(id, bytes) -> (bool, f64, Option<Stats>)
```

### MemoryPressureManager
```rust
manager.pin_adapter(id)
manager.unpin_adapter(id)
manager.is_pinned(id) -> bool
manager.check_and_handle_pressure() -> Result<MemoryPressureReport>
manager.get_stats() -> MemoryStats
```

### BufferPool
```rust
pool.acquire_buffer(size) -> Result<Vec<u8>>
pool.release_buffer(buffer)
pool.convert_tensor_format(data, from, to, shape) -> Result<Vec<u8>>
pool.stats() -> BufferPoolStats
pool.clear()
```

### MemoryTelemetryWriter
```rust
telemetry.emit_allocation(id, backend, buffer_bytes, kv_cache_bytes)
telemetry.emit_deallocation(id, bytes_freed)
telemetry.emit_pressure(&report)
telemetry.emit_eviction(&evicted)
telemetry.emit_stats(&stats)
telemetry.emit_buffer_pool_stats(&pool_stats)
telemetry.emit_fingerprint_verification(id, verified, bytes)
telemetry.emit_footprint_anomaly(id, bytes, z_score, within_tolerance)
```

## Pressure Levels

| Level | Headroom | Action |
|-------|----------|--------|
| Low | ≥25% | None |
| Medium | 15-25% | EvictLowPriority |
| High | 10-15% | EvictCrossBackend |
| Critical | <10% | EmergencyEvict |

## Backend Priority

Lower priority = evicted first

| Backend | Priority | Rationale |
|---------|----------|-----------|
| CoreML | 0.5 | Preserve ANE efficiency |
| MLX | 0.75 | Unified memory overhead |
| Metal | 1.0 | Most flexible |

## Eviction Order

1. **Priority** (ascending)
2. **Bytes** (descending - evict large first)
3. **BLAKE3 hash** of adapter ID (deterministic tiebreaker)

## Error Handling

```rust
// Memory limit exceeded
Err(AosError::Memory("Global memory limit exceeded: ..."))

// Fingerprint mismatch
Err(AosError::Memory("Fingerprint mismatch for adapter ..."))

// Emergency eviction failed
Err(AosError::Memory("Emergency eviction failed to restore headroom: ..."))

// Buffer too large
Err(AosError::Memory("Requested buffer size ... exceeds max ..."))
```

## Configuration

### Memory Limits
```rust
MemoryLimits::new(
    max_vram,          // bytes
    max_system_ram,    // bytes
    headroom_pct       // 0.0-1.0 (0.15 = 15%)
)
```

### Buffer Pool
```rust
BufferPoolConfig {
    max_pool_size: 64,                    // buffers
    max_buffer_size: 128 * 1024 * 1024,   // 128 MB
    enable_conversion_cache: true,
    max_conversion_cache_size: 32,        // entries
}
```

## Telemetry Events

| Event | Level | Trigger |
|-------|-------|---------|
| `memory.allocation` | info | Adapter allocated |
| `memory.deallocation` | info | Adapter freed |
| `memory.pressure.info` | info | Low/Medium pressure |
| `memory.pressure.warn` | warn | High pressure |
| `memory.pressure.error` | error | Critical pressure |
| `memory.eviction` | warn | Adapter evicted |
| `memory.stats` | info | Periodic snapshot |
| `memory.buffer_pool.stats` | info | Pool statistics |
| `memory.fingerprint.verification` | warn | Verification failed |
| `memory.footprint.anomaly` | warn | Anomaly detected |

## Best Practices

1. ✅ Pin production-critical adapters
2. ✅ Check pressure every 5 seconds
3. ✅ Clear buffer pool on high pressure
4. ✅ Emit telemetry for all allocations
5. ✅ Verify fingerprints before inference
6. ✅ Monitor headroom percentage
7. ✅ Use buffer pool for IoBuffers
8. ✅ Cache tensor conversions
9. ✅ Handle eviction errors gracefully
10. ✅ Log eviction events for audit

## Common Mistakes

❌ **Forgetting to untrack adapter on eviction**
```rust
// Wrong
lifecycle.evict_adapter(id);

// Right
tracker.untrack_adapter(id);
lifecycle.evict_adapter(id);
```

❌ **Not pinning critical adapters**
```rust
// Wrong - base model can be evicted
tracker.track_adapter(base_id, ...);

// Right
tracker.track_adapter(base_id, ...);
manager.pin_adapter(base_id);
```

❌ **Ignoring pressure reports**
```rust
// Wrong - eviction happened but lifecycle not notified
let _ = manager.check_and_handle_pressure();

// Right
let report = manager.check_and_handle_pressure()?;
for evicted in &report.adapters_evicted {
    lifecycle.evict_adapter(evicted.adapter_id).await?;
}
```

❌ **Not releasing buffers**
```rust
// Wrong - buffer leaked
let buffer = pool.acquire_buffer(size)?;
// ... use buffer but forget to release

// Right
let buffer = pool.acquire_buffer(size)?;
// ... use buffer
pool.release_buffer(buffer);
```

## Performance Tips

1. **Buffer pooling reduces allocations by 80%+**
2. **Conversion cache avoids redundant format conversions**
3. **Size bucketing (power-of-2) enables efficient reuse**
4. **Fingerprint verification uses checkpoint sampling (not full readback)**
5. **Adaptive baseline uses rolling window (bounded memory)**

## Debugging

### Enable tracing
```rust
RUST_LOG=adapteros_memory=debug cargo run
```

### Check memory stats
```rust
let stats = manager.get_stats();
eprintln!("Memory: {:#?}", stats);
```

### Inspect eviction candidates
```rust
let candidates = tracker.get_eviction_candidates(&[]);
for (id, backend, bytes, priority) in candidates {
    eprintln!("Candidate: {} ({}, {} MB, prio={})",
        id, backend.as_str(), bytes / 1_000_000, priority);
}
```

### Verify fingerprints
```rust
let fp = tracker.get_fingerprint(adapter_id);
eprintln!("Fingerprint: {:?}", fp);
```

## See Also

- [GUIDE_UNIFIED_MEMORY.md](GUIDE_UNIFIED_MEMORY.md) - Full documentation
- [examples/unified_memory_example.rs](examples/unified_memory_example.rs) - Complete example
- [AGENTS.md](../../AGENTS.md) - System architecture
- [Policy Pack #12](../../docs/POLICIES.md) - Memory policy

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
