# MLX Memory Management

**Last Updated:** 2025-11-21  
**Status:** Implemented and integrated  
**Coverage:** 6 core functions + unified memory infrastructure integration

---

## Overview

This document describes the MLX memory management functions implemented in the `adapteros-lora-mlx-ffi` crate. These functions provide comprehensive memory control capabilities including garbage collection, memory usage tracking, and GPU operation synchronization.

## Quick Reference

### Import

```rust
use adapteros_lora_mlx_ffi::{MLXMemoryManager, MemoryManagementStats};
use adapteros_lora_mlx_ffi::memory;  // Direct module access
```

### Basic Operations

```rust
// Create manager
let manager = MLXMemoryManager::new();

// Get memory usage
let bytes = manager.memory_usage()?;
let mb = bytes as f32 / (1024.0 * 1024.0);

// Or via module
let bytes = memory::memory_usage();

// Get allocation count
let count = manager.allocation_count()?;

// Get full stats
let stats = manager.memory_stats()?;
println!("Current: {:.2} MB, Peak: {:.2} MB, Allocations: {}",
    stats.total_mb(), stats.peak_mb(), stats.allocation_count);

// Trigger garbage collection
manager.gc_collect()?;

// Synchronize GPU
manager.synchronize()?;
```

### Module-Level Functions

```rust
use adapteros_lora_mlx_ffi::memory;

memory::gc_collect()                  // Trigger GC
memory::memory_usage()                // Bytes
memory::allocation_count()            // Count
memory::memory_stats()                // Tuple (bytes, count)
memory::reset()                       // Reset tracking
memory::stats()                       // Struct stats
memory::bytes_to_mb(bytes)           // Conversion
memory::format_stats(&stats)         // Formatting
memory::exceeds_threshold(mb)        // Quick check
```

---

## Architecture

### Components

```
MLXMemoryManager (High-level API)
    ├── MemoryTracker (Atomic tracking)
    ├── FFI Layer (C/C++ interface)
    │   ├── mlx_gc_collect()
    │   ├── mlx_memory_usage()
    │   ├── mlx_allocation_count()
    │   ├── mlx_memory_stats()
    │   ├── mlx_eval()
    │   └── mlx_synchronize()
    └── Integration Layer
        ├── adapteros-memory infrastructure
        ├── Telemetry system
        └── Lifecycle management
```

### Implementation Locations

| Component | Location | Purpose |
|-----------|----------|---------|
| **Rust API** | `crates/adapteros-lora-mlx-ffi/src/memory_management.rs` | High-level Rust interface |
| **Memory Module** | `crates/adapteros-lora-mlx-ffi/src/lib.rs` (module `memory`) | Public API wrapper |
| **Real C++** | `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp` | MLX FFI implementation |
| **Stub C++** | `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper.cpp` | Fallback stub implementation |
| **Tests** | `crates/adapteros-lora-mlx-ffi/tests/memory_management_integration.rs` | Comprehensive test suite |

---

## Core Functions

### 1. Garbage Collection: `mlx_gc_collect()`

**Purpose:** Trigger garbage collection in MLX unified memory

**Signature (Rust):**
```rust
pub fn gc_collect() -> Result<()>
```

**Behavior:**
- Flushes pending GPU operations
- Allows memory manager to reclaim unused buffers
- Records GC event in tracker
- Non-blocking but can be slow (10-100ms)

**Usage Example:**
```rust
let manager = MLXMemoryManager::new();
manager.gc_collect()?;
```

### 2. Memory Usage Query: `mlx_memory_usage()`

**Purpose:** Get current memory usage in bytes

**Signature (Rust):**
```rust
pub fn memory_usage(&self) -> Result<usize>
```

**Behavior:**
- Returns total bytes allocated to MLX arrays/models
- Uses atomic load (lock-free)
- Updates peak memory tracker
- Fast O(1) operation (<1µs)

**Usage Example:**
```rust
let bytes = manager.memory_usage()?;
let mb = bytes as f32 / (1024.0 * 1024.0);
println!("Current: {:.2} MB", mb);
```

### 3. Allocation Counting: `mlx_allocation_count()`

**Purpose:** Get number of tracked allocations

**Signature (Rust):**
```rust
pub fn allocation_count(&self) -> Result<usize>
```

**Behavior:**
- Returns count of active allocations
- Useful for leak detection
- Uses atomic load (lock-free)
- Fast O(1) operation (<1µs)

**Usage Example:**
```rust
let count = manager.allocation_count()?;
if count > 1000 {
    warn!("High allocation count: {}", count);
    manager.gc_collect()?;
}
```

### 4. Memory Statistics: `mlx_memory_stats()`

**Purpose:** Get detailed memory statistics snapshot

**Signature (Rust):**
```rust
pub fn memory_stats(&self) -> Result<MemoryManagementStats>
```

**Return Structure:**
```rust
#[derive(Debug, Clone, Copy)]
pub struct MemoryManagementStats {
    pub total_bytes: usize,      // Current allocation
    pub allocation_count: usize, // Active allocations
    pub peak_bytes: u64,         // Peak seen this session
}

// Convenience methods
stats.total_mb()                      // Bytes to MB
stats.peak_mb()                       // Peak to MB
stats.exceeds_mb_threshold(2048.0)   // Threshold check
```

**Behavior:**
- Atomically reads both counters
- Includes peak memory tracking
- Comprehensive snapshot at point in time (<10µs)

**Usage Example:**
```rust
let stats = manager.memory_stats()?;
println!("Current: {:.2} MB, Peak: {:.2} MB, Allocations: {}",
    stats.total_mb(), stats.peak_mb(), stats.allocation_count);
```

### 5. GPU Synchronization: `mlx_synchronize()`

**Purpose:** Synchronize all GPU operations

**Signature (Rust):**
```rust
pub fn synchronize(&self) -> Result<()>
```

**Behavior:**
- Blocks until all GPU operations complete
- Ensures memory is committed and visible
- Expensive - use sparingly (1-10ms)
- Important for accurate memory measurements

**Usage Example:**
```rust
manager.synchronize()?;
let stats = manager.memory_stats()?;  // Accurate after sync
```

---

## Usage Patterns

### Pattern 1: Memory Pressure Monitoring

Monitor memory during long-running operations:

```rust
use adapteros_lora_mlx_ffi::MLXMemoryManager;
use std::time::Duration;
use tokio::time::sleep;

async fn monitor_memory(manager: &MLXMemoryManager) -> Result<()> {
    loop {
        let stats = manager.memory_stats()?;

        tracing::debug!(
            current_mb = stats.total_mb(),
            peak_mb = stats.peak_mb(),
            allocations = stats.allocation_count,
            "Memory snapshot"
        );

        // React to pressure
        match stats.total_mb() as u32 {
            0..=1024 => {}, // Normal
            1025..=2048 => {
                tracing::warn!("Moderate memory pressure");
            }
            2049..=3072 => {
                tracing::warn!("High memory pressure, triggering GC");
                manager.gc_collect()?;
            }
            _ => {
                tracing::error!("Critical memory pressure!");
                manager.synchronize()?;
                // Force eviction of adapters
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}
```

### Pattern 2: Pre-Operation Memory Check

Check memory before heavy operations:

```rust
fn load_adapter_with_check(
    manager: &MLXMemoryManager,
    adapter_id: &str,
    adapter_size_mb: u32,
) -> Result<()> {
    let stats = manager.memory_stats()?;
    let available_mb = 8192 - stats.total_mb() as u32; // Assume 8GB total

    if available_mb < adapter_size_mb * 2 {  // Need 2x headroom
        tracing::warn!("Insufficient memory, triggering GC");
        manager.gc_collect()?;

        // Re-check after GC
        let stats = manager.memory_stats()?;
        if stats.total_mb() as u32 + adapter_size_mb > 8192 {
            return Err(AosError::MemoryPressure(
                format!("Insufficient memory even after GC: need {}MB", adapter_size_mb)
            ).into());
        }
    }

    // Load adapter...
    Ok(())
}
```

### Pattern 3: Memory Pressure Analysis

```rust
use adapteros_lora_mlx_ffi::memory_management::integration;

let stats = manager.memory_stats()?;
let recommendation = integration::analyze_memory_pressure(&stats, 8192); // 8GB available

match recommendation {
    MemoryPressureRecommendation::Normal { .. } => {
        // Normal operation
    }
    MemoryPressureRecommendation::Moderate { .. } => {
        // Consider GC or adapter eviction
        manager.gc_collect()?;
    }
    MemoryPressureRecommendation::High { .. } => {
        // GC and adapter unloading recommended
        manager.gc_collect()?;
        // Unload least-used adapters
    }
    MemoryPressureRecommendation::Critical { .. } => {
        // Immediate action required
        manager.gc_collect()?;
        manager.synchronize()?;
        // Force eviction of adapters
    }
}
```

### Pattern 4: Synchronization Before Measurement

Get accurate readings with synchronization:

```rust
fn get_accurate_memory_stats(
    manager: &MLXMemoryManager
) -> Result<MemoryManagementStats> {
    // Sync GPU to ensure all operations complete
    manager.synchronize()?;

    // Now get accurate stats
    let stats = manager.memory_stats()?;

    // Verify consistency
    debug_assert!(stats.total_bytes <= stats.peak_bytes as usize,
        "Current memory exceeds peak (impossible!)");

    Ok(stats)
}
```

---

## Integration with Unified Memory Infrastructure

### Memory Pressure Analysis

```rust
use adapteros_lora_mlx_ffi::memory_management::integration;

let stats = manager.memory_stats()?;
let recommendation = integration::analyze_memory_pressure(&stats, 10); // 10 GB available

match recommendation {
    MemoryPressureRecommendation::Normal { .. } => {
        // Normal operation
    }
    MemoryPressureRecommendation::Moderate { .. } => {
        // Consider GC or adapter eviction
        manager.gc_collect()?;
    }
    MemoryPressureRecommendation::High { .. } => {
        // GC and adapter unloading recommended
        manager.gc_collect()?;
        // Unload least-used adapters
    }
    MemoryPressureRecommendation::Critical { .. } => {
        // Immediate action required
        manager.gc_collect()?;
        manager.synchronize()?;
        // Force eviction of adapters
    }
}
```

### Unified Memory Conversion

```rust
use adapteros_lora_mlx_ffi::memory_management::integration;

let stats = manager.memory_stats()?;
let (bytes, count) = integration::mlx_stats_to_unified(&stats);

// Can now be used with adapteros-memory infrastructure
```

---

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| `gc_collect()` | 10-100ms | GPU-blocking, depends on buffer fragmentation |
| `memory_usage()` | <1µs | Atomic load, lock-free |
| `allocation_count()` | <1µs | Atomic load, lock-free |
| `memory_stats()` | <10µs | Two atomic loads + peak tracking |
| `synchronize()` | 1-10ms | GPU-blocking, expensive |

## Thread Safety

All functions are thread-safe:
- **Memory tracking:** Atomic operations (lock-free)
- **FFI calls:** Safe across threads
- **GPU operations:** MLX handles GPU synchronization
- **Error handling:** Thread-local error state

## Error Handling

Errors are converted to `Result<T>`:

```rust
pub fn gc_collect(&self) -> Result<()>
pub fn memory_usage(&self) -> Result<usize>
pub fn memory_stats(&self) -> Result<MemoryManagementStats>
pub fn synchronize(&self) -> Result<()>
```

FFI failures are logged but don't propagate when possible.

---

## API Reference

### MLXMemoryManager

```rust
pub fn new() -> Self
pub fn gc_collect(&self) -> Result<()>
pub fn memory_usage(&self) -> Result<usize>
pub fn allocation_count(&self) -> Result<usize>
pub fn memory_stats(&self) -> Result<MemoryManagementStats>
pub fn synchronize(&self) -> Result<()>
pub fn check_and_gc(&self, threshold_mb: f32) -> Result<bool>
pub fn reset(&self) -> Result<()>
pub fn tracker(&self) -> &Arc<MemoryTracker>
```

### MemoryTracker

```rust
pub fn new() -> Arc<Self>
pub fn record_memory(&self, bytes: usize)
pub fn peak_memory(&self) -> u64
pub fn record_gc(&self)
pub fn collection_count(&self) -> u64
pub fn reset(&self)
```

---

## Testing

Comprehensive test suite in `tests/memory_management_integration.rs`:

- Unit tests for memory tracking
- Integration tests with memory module
- Concurrent access tests
- Threshold checking tests
- Pressure analysis tests

Run tests:
```bash
cargo test -p adapteros-lora-mlx-ffi memory_management
```

---

## Known Limitations

1. **Partial Determinism:** MLX execution order can vary (GC doesn't guarantee FIFO)
2. **No Explicit Allocation:** Direct allocation API not exposed (only tracking)
3. **GPU Blocking:** `synchronize()` blocks entire GPU pipeline
4. **No Memory Pools:** Pooled allocation not integrated (separate from MLXMemoryPool)

## Future Enhancements

1. **Adaptive GC:** Auto-trigger GC based on pressure thresholds
2. **Memory Budgets:** Per-adapter memory allocation limits
3. **Fragmentation Analysis:** Detailed fragmentation metrics
4. **Custom Allocators:** Integration with custom memory allocators
5. **GPU-native Sync:** Use GPU fence primitives instead of dummy eval

---

## Related Documentation

- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Memory management patterns
- [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) - Memory event telemetry
- [crates/adapteros-memory/src/lib.rs](../crates/adapteros-memory/src/lib.rs) - Unified memory infrastructure

