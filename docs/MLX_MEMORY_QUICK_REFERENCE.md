# MLX Memory Management - Quick Reference Card

## Import

```rust
use adapteros_lora_mlx_ffi::{MLXMemoryManager, MemoryManagementStats};
use adapteros_lora_mlx_ffi::memory;  // Direct module access
```

## Basic Operations

### Create Manager
```rust
let manager = MLXMemoryManager::new();
```

### Get Memory Usage
```rust
let bytes = manager.memory_usage()?;
let mb = bytes as f32 / (1024.0 * 1024.0);

// Or via module
let bytes = memory::memory_usage();
```

### Get Allocation Count
```rust
let count = manager.allocation_count()?;

// Or via module
let count = memory::allocation_count();
```

### Get Full Stats
```rust
let stats = manager.memory_stats()?;
println!("Current: {:.2} MB, Peak: {:.2} MB, Allocations: {}",
    stats.total_mb(), stats.peak_mb(), stats.allocation_count);

// Or via module
let stats = memory::stats();
println!("{}", memory::format_stats(&stats));
```

### Trigger Garbage Collection
```rust
manager.gc_collect()?;

// Or via module
memory::gc_collect();
```

### Synchronize GPU
```rust
manager.synchronize()?;
```

## Common Patterns

### Check Memory Pressure
```rust
if stats.exceeds_mb_threshold(2048.0) {
    manager.gc_collect()?;
}
```

### Conditional GC
```rust
let did_gc = manager.check_and_gc(3000.0)?;
if did_gc {
    println!("GC triggered due to memory pressure");
}
```

### Accurate Measurement
```rust
manager.synchronize()?;
let stats = manager.memory_stats()?;  // Accurate after sync
```

### Memory Pressure Analysis
```rust
use adapteros_lora_mlx_ffi::memory_management::integration;

let rec = integration::analyze_memory_pressure(&stats, 8192); // 8GB available
match rec {
    integration::MemoryPressureRecommendation::Normal { .. } => {},
    integration::MemoryPressureRecommendation::Moderate { .. } => {
        manager.gc_collect()?;
    },
    integration::MemoryPressureRecommendation::High { .. } => {
        manager.gc_collect()?;
        // Unload adapters
    },
    integration::MemoryPressureRecommendation::Critical { .. } => {
        manager.synchronize()?;
        // Force evict adapters
    },
}
```

## Stats Structure

```rust
pub struct MemoryManagementStats {
    pub total_bytes: usize,      // Current allocation
    pub allocation_count: usize, // Active allocations
    pub peak_bytes: u64,         // Peak seen
}

// Convenience methods
stats.total_mb()                      // Bytes to MB
stats.peak_mb()                       // Peak to MB
stats.exceeds_mb_threshold(2048.0)   // Threshold check
```

## Module-Level Functions

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

## Performance Characteristics

| Function | Time | Notes |
|----------|------|-------|
| `memory_usage()` | <1µs | Lock-free atomic |
| `allocation_count()` | <1µs | Lock-free atomic |
| `memory_stats()` | <10µs | Two atomic loads |
| `gc_collect()` | 10-100ms | GPU-blocking |
| `synchronize()` | 1-10ms | GPU-blocking |

## Thread Safety

All functions are thread-safe and can be called from multiple threads:

```rust
use std::sync::Arc;
use std::thread;

let manager = Arc::new(MLXMemoryManager::new());

for _ in 0..4 {
    let m = Arc::clone(&manager);
    thread::spawn(move || {
        let stats = m.memory_stats().ok();
    });
}
```

## Error Handling

All methods return `Result<T>`:

```rust
match manager.memory_usage() {
    Ok(bytes) => println!("Memory: {}", bytes),
    Err(e) => eprintln!("Failed: {}", e),
}

// Or use ? operator
let bytes = manager.memory_usage()?;
```

## Logging

Operations integrate with `tracing`:

```rust
use tracing::{debug, warn};

manager.gc_collect()?;  // Logs at debug level
if stats.exceeds_mb_threshold(3000.0) {
    warn!("High memory usage");
}
```

## Testing

Basic test:

```rust
#[test]
fn test_memory_tracking() -> Result<()> {
    let manager = MLXMemoryManager::new();
    let stats = manager.memory_stats()?;
    
    assert!(stats.total_bytes >= 0);
    assert!(stats.peak_bytes >= stats.total_bytes as u64);
    
    Ok(())
}
```

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

### MemoryManagementStats

```rust
pub struct MemoryManagementStats {
    pub total_bytes: usize,
    pub allocation_count: usize,
    pub peak_bytes: u64,
}

pub fn total_mb(&self) -> f32
pub fn peak_mb(&self) -> f32
pub fn exceeds_mb_threshold(&self, threshold_mb: f32) -> bool
```

## Integration Module

```rust
pub fn mlx_stats_to_unified(stats: &MemoryManagementStats) -> (u64, usize)
pub fn analyze_memory_pressure(
    stats: &MemoryManagementStats,
    available_memory_mb: usize
) -> MemoryPressureRecommendation

pub enum MemoryPressureRecommendation {
    Normal { current_mb, available_mb },
    Moderate { current_mb, available_mb },
    High { current_mb, available_mb },
    Critical { current_mb, available_mb },
}

pub fn requires_immediate_action(&self) -> bool
```

## Common Issues & Solutions

### Memory keeps growing
- Check for allocation leaks in adapter loading
- Run `gc_collect()` more frequently
- Verify allocation count isn't exponential

### GC doesn't free memory
- Call `synchronize()` before checking
- Wait a moment after GC before re-measuring
- Check if GPU operations are still pending

### Stats vary between calls
- Always `synchronize()` before critical measurements
- Wait between measurements
- Verify no concurrent GPU operations

## Documentation

- **Full spec:** `/Users/star/Dev/aos/docs/MLX_MEMORY_MANAGEMENT.md`
- **Usage guide:** `/Users/star/Dev/aos/docs/MLX_MEMORY_USAGE_GUIDE.md`
- **Implementation:** `/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/src/memory_management.rs`

## Testing

```bash
# Run all tests
cargo test -p adapteros-lora-mlx-ffi memory_management

# Run specific test
cargo test -p adapteros-lora-mlx-ffi memory_management::tests::test_memory_tracker_peak

# Run with output
cargo test -p adapteros-lora-mlx-ffi memory_management -- --nocapture
```

## Examples

See `/Users/star/Dev/aos/docs/MLX_MEMORY_USAGE_GUIDE.md` for:
- Memory pressure monitoring
- Pre-operation memory checking
- Memory leak detection
- Synchronization patterns
- Periodic cleanup patterns
- Telemetry integration
- Error handling patterns
