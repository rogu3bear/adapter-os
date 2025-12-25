# Unified Memory Management System

**Author:** James KC Auchterlonie
**Date:** 2025-11-19
**Status:** Implemented

## Overview

The unified memory management system provides centralized memory tracking, pressure detection, and eviction coordination across Metal, CoreML, and MLX backends.

## Architecture

### Components

1. **UnifiedMemoryTracker** - Multi-backend memory accounting
   - Tracks allocations per adapter per backend
   - GPU buffer fingerprint verification
   - Adaptive memory footprint baseline detection

2. **BufferPool** - Buffer reuse and tensor format conversion
   - IoBuffer pooling with size bucketing
   - Tensor format conversion cache (Metal ↔ CoreML ↔ MLX)
   - Memory pressure-aware eviction

3. **MemoryPressureManager** - Pressure detection and eviction coordination
   - 15% headroom enforcement (per Policy Pack #12)
   - Priority-based eviction (pinned adapters never evicted)
   - Cross-backend eviction coordination

4. **MemoryTelemetryWriter** - Structured event logging
   - Allocation/deallocation events
   - Pressure and eviction events
   - Fingerprint verification events
   - Buffer pool statistics

## Memory Limits

```rust
use adapteros_memory::{MemoryLimits, UnifiedMemoryTracker};

let limits = MemoryLimits::new(
    8 * 1024 * 1024 * 1024,  // max_vram: 8GB
    16 * 1024 * 1024 * 1024, // max_system_ram: 16GB
    0.15                      // headroom_pct: 15%
);

let tracker = UnifiedMemoryTracker::new(limits);
```

### Headroom Calculation

- **VRAM headroom:** `(max_vram - used_vram) / max_vram * 100`
- **System RAM headroom:** `(max_system_ram - used_ram) / max_system_ram * 100`
- **Effective limit:** `max_vram - (max_vram * headroom_pct)`

## Pressure Levels

| Level | Headroom Range | Action |
|-------|----------------|--------|
| **Low** | ≥ 25% | None |
| **Medium** | 15-25% | Evict low priority adapters |
| **High** | 10-15% | Cross-backend eviction (Metal → MLX → CoreML) |
| **Critical** | < 10% | Emergency eviction (all unpinned adapters) |

## Eviction Strategies

### 1. EvictLowPriority
Evict unpinned adapters with lowest priority scores (LRU-based).

### 2. EvictCrossBackend
Coordinated eviction across backends:
1. **Metal first** - Evict Metal adapters (priority 1.0)
2. **MLX second** - Evict MLX adapters (priority 0.75)
3. **CoreML last** - Evict CoreML adapters (priority 0.5)

Rationale: Preserve CoreML/ANE allocations for efficiency.

### 3. ReduceK
Reduce K value to decrease active adapters (handled by lifecycle manager).

### 4. EmergencyEvict
Evict all unpinned adapters until headroom restored.

## Buffer Pooling

### Configuration

```rust
use adapteros_memory::{BufferPool, BufferPoolConfig};

let config = BufferPoolConfig {
    max_pool_size: 64,
    max_buffer_size: 128 * 1024 * 1024, // 128 MB
    enable_conversion_cache: true,
    max_conversion_cache_size: 32,
};

let pool = BufferPool::new(config);
```

### Usage

```rust
// Acquire buffer from pool
let buffer = pool.acquire_buffer(1024)?;

// Use buffer...

// Return to pool (automatic reuse)
pool.release_buffer(buffer);
```

### Tensor Format Conversion

```rust
use adapteros_memory::TensorFormat;

let metal_data: &[u8] = /* ... */;
let shape = (256, 256, 3); // H, W, C

// Convert Metal (f32) → CoreML (f16, channel-first)
let coreml_data = pool.convert_tensor_format(
    metal_data,
    TensorFormat::Metal,
    TensorFormat::CoreML,
    shape
)?;
```

**Supported conversions:**
- Metal ↔ CoreML (f32 row-major ↔ f16 channel-first)
- Metal ↔ MLX (same format, unified memory)
- CoreML ↔ MLX (f16 channel-first ↔ f32 row-major)

## Memory Tracking

### Track Adapter Allocation

```rust
use adapteros_memory::{BackendType, UnifiedMemoryTracker};

let tracker = UnifiedMemoryTracker::new(limits);

// Track Metal allocation
tracker.track_adapter(
    1,                     // adapter_id
    BackendType::Metal,    // backend
    32 * 1024 * 1024,      // buffer_bytes (32MB)
    16 * 1024 * 1024       // kv_cache_bytes (16MB)
);

// Track CoreML allocation for same adapter
tracker.track_adapter(
    1,
    BackendType::CoreML,
    8 * 1024 * 1024,       // buffer_bytes (8MB)
    0                       // kv_cache_bytes
);
```

### Query Memory Usage

```rust
// Total memory across all backends
let total = tracker.get_total_memory();

// Backend-specific memory
let metal_mem = tracker.get_backend_memory(BackendType::Metal);
let coreml_mem = tracker.get_backend_memory(BackendType::CoreML);
let mlx_mem = tracker.get_backend_memory(BackendType::MLX);

// Adapter-specific memory (all backends)
let adapter_mem = tracker.get_adapter_memory(1);
let allocations = tracker.get_adapter_allocations(1);
// Returns: Vec<(BackendType, u64)>
```

## Pressure Management

### Basic Usage

```rust
use adapteros_memory::{MemoryPressureManager, UnifiedMemoryTracker};
use std::sync::Arc;

let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
let manager = MemoryPressureManager::new(Arc::clone(&tracker));

// Pin critical adapter (never evict)
manager.pin_adapter(1);

// Check pressure and handle automatically
let report = manager.check_and_handle_pressure()?;

println!("Pressure: {:?}", report.pressure_level);
println!("Action: {:?}", report.action_taken);
println!("Freed: {} bytes", report.bytes_freed);
println!("Headroom: {:.2}% → {:.2}%",
    report.headroom_before,
    report.headroom_after
);
```

### Manual Eviction

```rust
// Get eviction candidates (sorted by priority)
let pinned = vec![1, 2]; // Adapter IDs to protect
let candidates = tracker.get_eviction_candidates(&pinned);

for (adapter_id, backend, bytes, priority) in candidates {
    if priority == f32::MAX {
        // Skip pinned adapters
        continue;
    }

    // Evict adapter
    tracker.untrack_adapter(adapter_id);
    println!("Evicted adapter {} from {} ({} bytes)",
        adapter_id, backend.as_str(), bytes);
}
```

## GPU Integrity Verification

### Store Fingerprint

```rust
use adapteros_memory::GpuBufferFingerprint;

// Sample GPU buffer at checkpoints
let first_4kb: &[u8] = /* sample first 4KB */;
let last_4kb: &[u8] = /* sample last 4KB */;
let mid_4kb: &[u8] = /* sample midpoint 4KB */;

let fingerprint = GpuBufferFingerprint::new(
    buffer_size,
    first_4kb,
    last_4kb,
    mid_4kb
);

tracker.store_fingerprint(adapter_id, fingerprint);
```

### Verify Fingerprint

```rust
// Compute current fingerprint
let current_fp = GpuBufferFingerprint::new(
    current_buffer_size,
    first_4kb,
    last_4kb,
    mid_4kb
);

// Verify against stored baseline
match tracker.verify_fingerprint(adapter_id, &current_fp) {
    Ok(true) => println!("Fingerprint verified"),
    Ok(false) => println!("No baseline (first load)"),
    Err(e) => eprintln!("Fingerprint mismatch: {}", e),
}
```

### Memory Footprint Anomaly Detection

```rust
// Check if memory footprint is within 2σ tolerance
let (within_tolerance, z_score, stats) =
    tracker.check_memory_footprint(adapter_id, buffer_bytes);

if !within_tolerance {
    eprintln!(
        "Memory anomaly detected: {} bytes (z-score: {:.2})",
        buffer_bytes, z_score
    );

    if let Some((mean, stddev, samples)) = stats {
        eprintln!(
            "Baseline: mean={:.0}, stddev={:.0}, samples={}",
            mean, stddev, samples
        );
    }
}
```

## Telemetry Integration

### Setup

```rust
use adapteros_memory::{MemoryTelemetryWriter, TelemetryEventSink};
use std::sync::Arc;

struct MyTelemetrySink;

impl TelemetryEventSink for MyTelemetrySink {
    fn emit_event<T: serde::Serialize>(&self, event_type: &str, event: &T) {
        let json = serde_json::to_string(event).unwrap();
        println!("[{}] {}", event_type, json);
    }
}

let sink = Arc::new(MyTelemetrySink);
let telemetry = MemoryTelemetryWriter::new(Some(sink));
```

### Emit Events

```rust
// Allocation event
telemetry.emit_allocation(adapter_id, BackendType::Metal, buffer_bytes, kv_cache_bytes);

// Deallocation event
telemetry.emit_deallocation(adapter_id, bytes_freed);

// Pressure event
telemetry.emit_pressure(&report);

// Eviction event
telemetry.emit_eviction(&evicted_adapter);

// Stats snapshot
telemetry.emit_stats(&stats);

// Buffer pool stats
telemetry.emit_buffer_pool_stats(&pool_stats);

// Fingerprint verification
telemetry.emit_fingerprint_verification(adapter_id, verified, buffer_bytes);

// Footprint anomaly
telemetry.emit_footprint_anomaly(adapter_id, buffer_bytes, z_score, within_tolerance);
```

## Event Types

### memory.allocation
```json
{
  "adapter_id": 1,
  "backend": "Metal",
  "buffer_bytes": 33554432,
  "kv_cache_bytes": 16777216,
  "total_bytes": 50331648,
  "timestamp": 1700000000
}
```

### memory.deallocation
```json
{
  "adapter_id": 1,
  "bytes_freed": 50331648,
  "timestamp": 1700000100
}
```

### memory.pressure.warn
```json
{
  "pressure_level": "High",
  "action_taken": "EvictCrossBackend",
  "adapters_evicted_count": 2,
  "bytes_freed": 100663296,
  "headroom_before": 12.5,
  "headroom_after": 18.3,
  "timestamp": 1700000200
}
```

### memory.eviction
```json
{
  "adapter_id": 3,
  "backend": "Metal",
  "bytes_freed": 50331648,
  "timestamp": 1700000200
}
```

### memory.stats
```json
{
  "total_memory_used": 8589934592,
  "metal_memory_used": 6442450944,
  "coreml_memory_used": 1073741824,
  "mlx_memory_used": 1073741824,
  "pressure_level": "Medium",
  "headroom_pct": 20.5,
  "pinned_adapter_count": 2,
  "total_adapter_count": 16,
  "timestamp": 1700000300
}
```

## Integration with Lifecycle Manager

```rust
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_memory::{
    BackendType, MemoryLimits, MemoryPressureManager,
    UnifiedMemoryTracker
};
use std::sync::Arc;

// Create unified tracker
let limits = MemoryLimits::new(8_000_000_000, 16_000_000_000, 0.15);
let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
let pressure_mgr = Arc::new(MemoryPressureManager::new(Arc::clone(&tracker)));

// Create lifecycle manager with database
let lifecycle = LifecycleManager::new_with_db(
    adapter_names,
    adapter_hashes,
    &policies,
    adapters_path,
    telemetry,
    initial_k,
    db
);

// On adapter load
let adapter_id = 1u32;
tracker.track_adapter(
    adapter_id,
    BackendType::Metal,
    buffer_bytes,
    kv_cache_bytes
);

// Periodic pressure check (background task)
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;

        if let Ok(report) = pressure_mgr.check_and_handle_pressure() {
            if report.pressure_level != PressureLevel::Low {
                println!("Memory pressure: {:?}", report);

                // Notify lifecycle manager to evict adapters
                for evicted in &report.adapters_evicted {
                    lifecycle.evict_adapter(evicted.adapter_id as u16).await;
                }
            }
        }
    }
});
```

## Best Practices

### 1. Pin Critical Adapters
```rust
// Pin production adapters
manager.pin_adapter(base_model_adapter_id);
manager.pin_adapter(active_stack_adapter_id);
```

### 2. Monitor Headroom
```rust
let stats = manager.get_stats();
if stats.headroom_pct < 20.0 {
    warn!("Low memory headroom: {:.2}%", stats.headroom_pct);
}
```

### 3. Clear Buffer Pool on Pressure
```rust
if pressure.level == PressureLevel::High {
    pool.clear(); // Free pooled buffers and conversion cache
}
```

### 4. Emit Telemetry Events
```rust
// Always emit allocation events
telemetry.emit_allocation(adapter_id, backend, buffer_bytes, kv_cache_bytes);

// Emit stats snapshots periodically
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let stats = manager.get_stats();
        telemetry.emit_stats(&stats);
    }
});
```

### 5. Verify GPU Integrity
```rust
// Store fingerprint on load
let fp = GpuBufferFingerprint::new(buffer_size, first, last, mid);
tracker.store_fingerprint(adapter_id, fp);

// Verify before inference
let current_fp = GpuBufferFingerprint::new(buffer_size, first, last, mid);
tracker.verify_fingerprint(adapter_id, &current_fp)?;
```

## Performance Characteristics

### UnifiedMemoryTracker
- **Allocation tracking:** O(1) insert
- **Eviction candidates:** O(N log N) sort
- **Memory queries:** O(1) for single adapter, O(N) for all adapters

### BufferPool
- **Buffer acquire:** O(1) if pooled, O(N) allocation if new
- **Buffer release:** O(1) insert
- **Conversion cache:** O(1) lookup, O(N) eviction if full

### MemoryPressureManager
- **Pressure check:** O(N) for candidate sorting
- **Eviction:** O(N × M) where M is eviction count

## Determinism Guarantees

### Eviction Order
Eviction order is deterministic via:
1. **Priority score** (backend-based)
2. **Bytes** (largest first)
3. **BLAKE3 hash** of adapter ID (tiebreaker)

```rust
// Deterministic sort in eviction
candidates.sort_by(|a, b| {
    a.3.partial_cmp(&b.3)  // Priority
        .unwrap()
        .then_with(|| b.2.cmp(&a.2))  // Bytes (descending)
        .then_with(|| {
            let hash_a = blake3::hash(a.0.to_string().as_bytes());
            let hash_b = blake3::hash(b.0.to_string().as_bytes());
            hash_a.as_bytes().cmp(hash_b.as_bytes())
        })
});
```

### Fingerprint Verification
- Uses BLAKE3 hash of checkpoint samples
- Deterministic across runs for same buffer contents
- 2σ tolerance for adaptive baseline

## Citations

- [source: crates/adapteros-memory/src/unified_tracker.rs]
- [source: crates/adapteros-memory/src/buffer_pool.rs]
- [source: crates/adapteros-memory/src/pressure_manager.rs]
- [source: crates/adapteros-memory/src/telemetry.rs]
- [AGENTS.md L140: "Memory management: Adapter eviction with headroom maintenance"]
- [Policy Pack #12: "Memory - MUST maintain ≥ 15 percent unified memory headroom"]

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
