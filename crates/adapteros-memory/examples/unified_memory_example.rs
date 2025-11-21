//! Unified memory management example
//!
//! Demonstrates:
//! - Multi-backend memory tracking
//! - Buffer pooling and tensor conversion
//! - Pressure detection and eviction
//! - Telemetry integration

use adapteros_memory::{
    BackendType, BufferPool, BufferPoolConfig, MemoryLimits, MemoryPressureManager,
    MemoryTelemetryWriter, TelemetryEventSink, TensorFormat, UnifiedMemoryTracker,
};

use std::sync::{Arc, Mutex};

/// Mock telemetry sink that prints events
struct NoOpTelemetrySink {
    events: Arc<Mutex<Vec<String>>>,
}

impl NoOpTelemetrySink {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl TelemetryEventSink for NoOpTelemetrySink {
    fn emit_event(&self, event_type: &str, event: serde_json::Value) {
        let json = serde_json::to_string_pretty(&event).unwrap();
        let message = format!("[{}]\n{}\n", event_type, json);
        println!("{}", message);
        self.events.lock().unwrap().push(message);
    }
}

fn main() {
    println!("=== Unified Memory Management Example ===\n");

    // Create telemetry sink
    let telemetry_sink = Arc::new(NoOpTelemetrySink::new());
    let telemetry =
        MemoryTelemetryWriter::new(Some(telemetry_sink.clone() as Arc<dyn TelemetryEventSink>));

    // Define memory limits
    let limits = MemoryLimits::new(
        1 * 1024 * 1024 * 1024, // 1 GB VRAM
        2 * 1024 * 1024 * 1024, // 2 GB system RAM
        0.15,                   // 15% headroom
    );

    println!("Memory Limits:");
    println!("  Max VRAM: {} GB", limits.max_vram / (1024 * 1024 * 1024));
    println!(
        "  Max System RAM: {} GB",
        limits.max_system_ram / (1024 * 1024 * 1024)
    );
    println!("  Headroom: {:.0}%\n", limits.headroom_pct * 100.0);

    // Create unified tracker
    let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
    let manager = MemoryPressureManager::new(Arc::clone(&tracker));

    // Create buffer pool
    let pool_config = BufferPoolConfig {
        max_pool_size: 32,
        max_buffer_size: 64 * 1024 * 1024, // 64 MB
        enable_conversion_cache: true,
        max_conversion_cache_size: 16,
    };
    let pool = BufferPool::new(pool_config);

    println!("=== Scenario 1: Track adapters across backends ===\n");

    // Load adapters on different backends
    println!("Loading adapter 1 on Metal...");
    tracker.track_adapter(
        1,
        BackendType::Metal,
        32 * 1024 * 1024, // 32 MB buffer
        16 * 1024 * 1024, // 16 MB KV cache
    );
    telemetry.emit_allocation(1, BackendType::Metal, 32 * 1024 * 1024, 16 * 1024 * 1024);

    println!("Loading adapter 2 on CoreML...");
    tracker.track_adapter(
        2,
        BackendType::CoreML,
        16 * 1024 * 1024, // 16 MB buffer
        8 * 1024 * 1024,  // 8 MB KV cache
    );
    telemetry.emit_allocation(2, BackendType::CoreML, 16 * 1024 * 1024, 8 * 1024 * 1024);

    println!("Loading adapter 3 on MLX...");
    tracker.track_adapter(
        3,
        BackendType::Mlx,
        24 * 1024 * 1024, // 24 MB buffer
        12 * 1024 * 1024, // 12 MB KV cache
    );
    telemetry.emit_allocation(3, BackendType::Mlx, 24 * 1024 * 1024, 12 * 1024 * 1024);

    let stats = manager.get_stats();
    telemetry.emit_stats(&stats);

    println!("\nMemory Statistics:");
    println!("  Total: {} MB", stats.total_memory_used / (1024 * 1024));
    println!("  Metal: {} MB", stats.metal_memory_used / (1024 * 1024));
    println!("  CoreML: {} MB", stats.coreml_memory_used / (1024 * 1024));
    println!("  MLX: {} MB", stats.mlx_memory_used / (1024 * 1024));
    println!("  Headroom: {:.2}%", stats.headroom_pct);
    println!("  Pressure: {:?}\n", stats.pressure_level);

    println!("=== Scenario 2: Buffer pooling ===\n");

    println!("Acquiring buffer from pool...");
    let buffer1 = pool.acquire_buffer(4 * 1024).unwrap();
    println!("  Acquired {} bytes", buffer1.len());

    println!("Releasing buffer to pool...");
    pool.release_buffer(buffer1);

    println!("Acquiring buffer again (should reuse)...");
    let buffer2 = pool.acquire_buffer(4 * 1024).unwrap();
    println!("  Acquired {} bytes (reused)\n", buffer2.len());
    pool.release_buffer(buffer2);

    let pool_stats = pool.stats();
    telemetry.emit_buffer_pool_stats(&pool_stats);

    println!("Buffer Pool Statistics:");
    println!("  Pooled buffers: {}", pool_stats.buffer_count);
    println!(
        "  Pooled memory: {} KB",
        pool_stats.total_pooled_bytes / 1024
    );
    println!("  Cache entries: {}", pool_stats.cache_entries);
    println!(
        "  Cache memory: {} KB\n",
        pool_stats.total_cache_bytes / 1024
    );

    println!("=== Scenario 3: Tensor format conversion ===\n");

    let metal_data = vec![0u8; 256 * 256 * 3 * 4]; // 256x256 RGB image (f32)
    let shape = (256, 256, 3);

    println!("Converting Metal → CoreML...");
    let coreml_data = pool
        .convert_tensor_format(
            &metal_data,
            TensorFormat::Metal,
            TensorFormat::CoreML,
            shape,
        )
        .unwrap();
    println!(
        "  Converted {} bytes → {} bytes",
        metal_data.len(),
        coreml_data.len()
    );

    println!("Converting CoreML → MLX...");
    let mlx_data = pool
        .convert_tensor_format(&coreml_data, TensorFormat::CoreML, TensorFormat::Mlx, shape)
        .unwrap();
    println!(
        "  Converted {} bytes → {} bytes\n",
        coreml_data.len(),
        mlx_data.len()
    );

    println!("=== Scenario 4: Memory pressure and eviction ===\n");

    // Pin adapter 1 (production-critical)
    println!("Pinning adapter 1 (production-critical)...");
    manager.pin_adapter(1);

    // Load more adapters to trigger pressure
    println!("Loading adapter 4 on Metal (large allocation)...");
    tracker.track_adapter(
        4,
        BackendType::Metal,
        512 * 1024 * 1024, // 512 MB (large)
        256 * 1024 * 1024, // 256 MB KV cache
    );
    telemetry.emit_allocation(4, BackendType::Metal, 512 * 1024 * 1024, 256 * 1024 * 1024);

    println!("Loading adapter 5 on Metal...");
    tracker.track_adapter(
        5,
        BackendType::Metal,
        128 * 1024 * 1024, // 128 MB
        64 * 1024 * 1024,  // 64 MB KV cache
    );
    telemetry.emit_allocation(5, BackendType::Metal, 128 * 1024 * 1024, 64 * 1024 * 1024);

    println!("\nChecking memory pressure...");
    let report = manager.check_and_handle_pressure().unwrap();
    telemetry.emit_pressure(&report);

    println!("\nPressure Report:");
    println!("  Level: {:?}", report.pressure_level);
    println!("  Action: {:?}", report.action_taken);
    println!("  Adapters evicted: {}", report.adapters_evicted.len());
    println!("  Bytes freed: {} MB", report.bytes_freed / (1024 * 1024));
    println!(
        "  Headroom: {:.2}% → {:.2}%",
        report.headroom_before, report.headroom_after
    );

    if !report.adapters_evicted.is_empty() {
        println!("\nEvicted adapters:");
        for evicted in &report.adapters_evicted {
            println!(
                "  - Adapter {} from {} ({} MB freed)",
                evicted.adapter_id,
                evicted.backend.as_str(),
                evicted.bytes_freed / (1024 * 1024)
            );
            telemetry.emit_eviction(evicted);
        }
    }

    let final_stats = manager.get_stats();
    println!("\nFinal Memory Statistics:");
    println!(
        "  Total: {} MB",
        final_stats.total_memory_used / (1024 * 1024)
    );
    println!("  Headroom: {:.2}%", final_stats.headroom_pct);
    println!("  Adapters: {}", final_stats.total_adapter_count);
    println!("  Pinned: {}", final_stats.pinned_adapter_count);

    println!("\n=== Scenario 5: Eviction candidates ===\n");

    let pinned = vec![1];
    let candidates = tracker.get_eviction_candidates(&pinned);

    println!("Eviction candidates (sorted by priority):");
    for (adapter_id, backend, bytes, priority) in &candidates {
        let status = if *priority == f32::MAX {
            "PINNED"
        } else {
            "evictable"
        };
        println!(
            "  - Adapter {} ({}) on {}: {} MB (priority: {:.2}) [{}]",
            adapter_id,
            status,
            backend.as_str(),
            bytes / (1024 * 1024),
            priority,
            status
        );
    }

    println!("\n=== Summary ===");
    println!(
        "Total telemetry events emitted: {}",
        telemetry_sink.events.lock().unwrap().len()
    );
    println!("All scenarios completed successfully!");
}
