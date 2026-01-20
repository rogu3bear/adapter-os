# adapteros-system-metrics

System metrics collection for adapterOS (macOS optimized).

## Features

- **CPU & Memory**: standard system-wide metrics.
- **GPU (Metal)**: Utilization and VRAM tracking.
- **ANE (Apple Neural Engine)**: Real-time memory and throttling tracking.

## ANE Memory Tracking

The crate provides real-time Apple Neural Engine memory monitoring on macOS 15+ with Apple Silicon.

### Accuracy Tiers

1. **Direct Hardware Query**: Uses IOKit and a model registry to provide accurate allocation, usage, and peak bytes. (Source: `direct`)
2. **Estimation Fallback**: Uses system memory heuristics and compression metrics to estimate ANE impact. (Source: `estimated`)
3. **Unavailable**: Graceful degradation on non-ANE systems. (Source: `unavailable`)

### Example

```rust
use adapteros_system_metrics::ane::AneMetricsCollector;

let collector = AneMetricsCollector::new();
let stats = collector.collect_metrics();

if stats.available {
    println!("ANE Memory: {}/{} MB ({}%)",
             stats.used_mb, stats.allocated_mb, stats.usage_percent);
    println!("Data source: {}", stats.source);

    if stats.throttled {
        println!("WARNING: ANE is thermally throttled");
    }
}
```

## Integration with CoreML

The metrics are automatically populated when using the `adapteros-lora-kernel-coreml` backend, which tracks model load and unload events via internal FFI hooks.
