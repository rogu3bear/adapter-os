# MLX Memory Management Usage Guide

**Quick Reference Guide for MLX Memory Functions**

## Quick Start

### Basic Usage

```rust
use adapteros_lora_mlx_ffi::MLXMemoryManager;

let manager = MLXMemoryManager::new();

// Check current memory
let usage = manager.memory_usage()?;
let count = manager.allocation_count()?;
println!("Memory: {} bytes, Allocations: {}", usage, count);

// Get detailed stats
let stats = manager.memory_stats()?;
println!("Current: {:.2} MB, Peak: {:.2} MB",
    stats.total_mb(), stats.peak_mb());

// Trigger GC if needed
if stats.total_mb() > 2048.0 {
    manager.gc_collect()?;
}
```

### Module-Level API

```rust
use adapteros_lora_mlx_ffi::memory;

// Direct function calls
let bytes = memory::memory_usage();
let count = memory::allocation_count();

// Get formatted output
let stats = memory::stats();
println!("{}", memory::format_stats(&stats));

// Check threshold
if memory::exceeds_threshold(2048.0) {
    memory::gc_collect();
}
```

## Common Patterns

### Pattern 1: Memory Pressure Monitoring

Monitor memory during long-running operations:

```rust
use adapteros_lora_mlx_ffi::MLXMemoryManager;
use std::time::Duration;
use tokio::time::sleep;

async fn monitor_memory(manager: &MLXMemoryManager) -> Result<()> {
    loop {
        let stats = manager.memory_stats()?;

        // Log current state
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
                manager.synchronize()?;  // Sync to get accurate count
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
        tracing::warn!(
            current = stats.total_mb(),
            needed = adapter_size_mb,
            "Insufficient memory, triggering GC"
        );
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

### Pattern 3: Memory Leak Detection

Detect potential memory leaks during testing:

```rust
#[cfg(test)]
mod memory_leak_tests {
    use adapteros_lora_mlx_ffi::MLXMemoryManager;

    #[test]
    fn test_no_memory_leak_on_adapter_load_unload() -> Result<()> {
        let manager = MLXMemoryManager::new();

        // Reset and capture baseline
        manager.reset()?;
        let baseline = manager.memory_stats()?;
        assert_eq!(baseline.total_bytes, 0);

        // Load adapter
        let adapter = load_test_adapter()?;
        let after_load = manager.memory_stats()?;
        let loaded_size = after_load.total_bytes;

        // Unload adapter
        drop(adapter);
        manager.gc_collect()?;

        // Check if memory was released
        let after_unload = manager.memory_stats()?;
        let leaked = after_unload.total_bytes;

        // Allow small leak (metadata)
        assert!(leaked < loaded_size / 10,
            "Potential memory leak: {:.2}MB not freed",
            manager.bytes_to_mb(leaked)
        );

        Ok(())
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

### Pattern 5: Periodic Cleanup

Regular cleanup during training:

```rust
async fn training_loop_with_cleanup(
    manager: &MLXMemoryManager,
    trainer: &mut Trainer,
) -> Result<()> {
    for epoch in 0..100 {
        // Training step
        trainer.train_step()?;

        // Periodic cleanup (every 10 epochs)
        if epoch % 10 == 0 {
            let stats = manager.memory_stats()?;
            tracing::info!(
                epoch,
                memory_mb = stats.total_mb() as u32,
                allocations = stats.allocation_count,
                "Epoch checkpoint"
            );

            // GC if needed
            if stats.exceeds_mb_threshold(3000.0) {
                tracing::warn!("Triggering GC at epoch {}", epoch);
                manager.gc_collect()?;

                // Verify cleanup
                let post_gc = manager.memory_stats()?;
                tracing::info!(
                    before_gc = stats.total_mb() as u32,
                    after_gc = post_gc.total_mb() as u32,
                    "GC completed"
                );
            }
        }
    }

    Ok(())
}
```

## Memory Pressure Handling

### Analyzing Pressure Levels

```rust
use adapteros_lora_mlx_ffi::memory_management::integration;

let manager = MLXMemoryManager::new();
let stats = manager.memory_stats()?;

let recommendation = integration::analyze_memory_pressure(&stats, 8192); // 8GB

match recommendation {
    integration::MemoryPressureRecommendation::Normal { current_mb, available_mb } => {
        tracing::debug!(
            "Normal: {} MB of {} MB used ({:.0}%)",
            current_mb,
            available_mb,
            (current_mb as f32 / available_mb as f32) * 100.0
        );
    }
    integration::MemoryPressureRecommendation::Moderate { current_mb, available_mb } => {
        tracing::warn!(
            "Moderate pressure: {} MB of {} MB used ({:.0}%)",
            current_mb,
            available_mb,
            (current_mb as f32 / available_mb as f32) * 100.0
        );
        manager.gc_collect()?;
    }
    integration::MemoryPressureRecommendation::High { current_mb, available_mb } => {
        tracing::warn!(
            "High pressure: {} MB of {} MB used ({:.0}%)",
            current_mb,
            available_mb,
            (current_mb as f32 / available_mb as f32) * 100.0
        );
        manager.gc_collect()?;
        // Unload low-priority adapters
    }
    integration::MemoryPressureRecommendation::Critical { current_mb, available_mb } => {
        tracing::error!(
            "Critical pressure: {} MB of {} MB used ({:.0}%)",
            current_mb,
            available_mb,
            (current_mb as f32 / available_mb as f32) * 100.0
        );
        manager.synchronize()?;
        // Force evict adapters immediately
    }
}
```

## Integration with Lifecycle Management

### Before Adapter Loading

```rust
async fn load_adapter_lifecycle(
    manager: &MLXMemoryManager,
    lifecycle: &mut LifecycleManager,
    adapter_id: &str,
) -> Result<()> {
    // Check memory before promotion
    let stats = manager.memory_stats()?;
    if stats.exceeds_mb_threshold(3500.0) {
        tracing::info!("Pre-cleanup before adapter load");
        manager.gc_collect()?;
    }

    // Update lifecycle
    lifecycle.promote(adapter_id).await?;

    // Verify memory constraints
    let post_stats = manager.memory_stats()?;
    lifecycle.record_memory(adapter_id, post_stats.total_bytes).await?;

    Ok(())
}
```

### During Adapter Eviction

```rust
async fn evict_with_memory_check(
    manager: &MLXMemoryManager,
    lifecycle: &mut LifecycleManager,
    adapter_id: &str,
) -> Result<()> {
    let before = manager.memory_stats()?;

    // Evict from lifecycle
    lifecycle.evict(adapter_id).await?;

    // GC to reclaim memory
    manager.gc_collect()?;

    let after = manager.memory_stats()?;
    let freed = before.total_mb() - after.total_mb();

    tracing::info!(
        adapter_id,
        freed_mb = freed as i32,
        "Adapter evicted and memory reclaimed"
    );

    Ok(())
}
```

## Telemetry Integration

### Recording Memory Metrics

```rust
use adapteros_telemetry::TelemetryEventSink;

async fn record_memory_metrics(
    manager: &MLXMemoryManager,
    telemetry: &TelemetryEventSink,
) -> Result<()> {
    let stats = manager.memory_stats()?;

    telemetry.record_metric("mlx.memory.current_mb", stats.total_mb() as f64).await;
    telemetry.record_metric("mlx.memory.peak_mb", stats.peak_mb() as f64).await;
    telemetry.record_metric("mlx.memory.allocations", stats.allocation_count as f64).await;

    // Record pressure level
    use adapteros_lora_mlx_ffi::memory_management::integration;
    let recommendation = integration::analyze_memory_pressure(&stats, 8192);

    let pressure_level = match recommendation {
        integration::MemoryPressureRecommendation::Normal { .. } => "normal",
        integration::MemoryPressureRecommendation::Moderate { .. } => "moderate",
        integration::MemoryPressureRecommendation::High { .. } => "high",
        integration::MemoryPressureRecommendation::Critical { .. } => "critical",
    };

    telemetry.record_tag("mlx.memory.pressure_level", pressure_level).await;

    Ok(())
}
```

## Error Handling

### Handling Memory Operations

```rust
use adapteros_core::AosError;

fn safe_gc_collect(manager: &MLXMemoryManager) {
    match manager.gc_collect() {
        Ok(()) => {
            tracing::debug!("GC completed successfully");
        }
        Err(e) => {
            // GC shouldn't fail, but if it does, log and continue
            tracing::warn!(error = %e, "GC operation failed");
            // Continue operation - memory will be managed by system
        }
    }
}

fn safe_memory_stats(manager: &MLXMemoryManager) -> MemoryManagementStats {
    match manager.memory_stats() {
        Ok(stats) => stats,
        Err(e) => {
            // Return zero stats and log
            tracing::error!(error = %e, "Failed to get memory stats");
            MemoryManagementStats {
                total_bytes: 0,
                allocation_count: 0,
                peak_bytes: 0,
            }
        }
    }
}
```

## Performance Tips

1. **Minimize synchronization calls:** `synchronize()` is expensive
2. **Batch memory checks:** Don't query stats every operation
3. **Use lock-free APIs:** `memory_usage()` and `allocation_count()` are fast
4. **GC periodically:** Don't wait for critical pressure
5. **Check thresholds:** Use `exceeds_threshold()` for quick checks

## Testing Memory Functions

```rust
#[cfg(test)]
mod memory_tests {
    use adapteros_lora_mlx_ffi::MLXMemoryManager;

    #[test]
    fn test_memory_tracking_basic() -> Result<()> {
        let manager = MLXMemoryManager::new();

        let before = manager.memory_usage()?;
        // Do memory-using operations
        let after = manager.memory_usage()?;

        assert!(after >= before);
        Ok(())
    }

    #[test]
    fn test_gc_reduces_memory() -> Result<()> {
        let manager = MLXMemoryManager::new();

        let before = manager.memory_stats()?;
        manager.gc_collect()?;
        let after = manager.memory_stats()?;

        // Memory should decrease or stay same
        assert!(after.total_bytes <= before.total_bytes);
        Ok(())
    }

    #[test]
    fn test_peak_memory_tracking() -> Result<()> {
        let manager = MLXMemoryManager::new();
        manager.reset()?;

        // Simulate memory allocation
        let tracker = manager.tracker();
        tracker.record_memory(1000);
        tracker.record_memory(500);  // Decrease
        tracker.record_memory(2000); // New peak

        assert_eq!(tracker.peak_memory(), 2000);
        Ok(())
    }
}
```

## Troubleshooting

### High Memory Usage

**Symptoms:** Continuous memory growth, exceeds peak

**Solutions:**
1. Check for memory leaks in adapter loading/unloading
2. Reduce model batch size
3. Enable more frequent GC
4. Check allocation count for exponential growth

### Frequent GC Triggers

**Symptoms:** GC called every few operations

**Solutions:**
1. Increase memory threshold
2. Reduce adapter count
3. Use smaller models
4. Profile allocation patterns

### Memory Measurement Inconsistency

**Symptoms:** Stats vary wildly between calls

**Solutions:**
1. Always call `synchronize()` before critical measurements
2. Allow time between measurements
3. Verify no concurrent operations
4. Check for pending GPU operations
