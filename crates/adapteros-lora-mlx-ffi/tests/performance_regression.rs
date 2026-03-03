//! Performance regression tests for MLX FFI backend
//!
//! These tests establish baseline performance and ensure no regressions occur.
//! Run with: `cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture`

use std::time::Instant;

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::{
    backend::MLXFFIBackend,
    mock::{create_mock_adapter, create_mock_config},
    tensor::MLXFFITensor,
    MLXFFIModel,
};

// =============================================================================
// TEST HELPERS
// =============================================================================

/// Helper to measure operation timing
struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    fn new(name: &str) -> Self {
        println!("  Starting: {}", name);
        Timer {
            start: Instant::now(),
            name: name.to_string(),
        }
    }

    fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn elapsed_us(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1_000_000.0
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed_ms = self.elapsed_ms();
        println!("  Completed: {} ({:.2}ms)", self.name, elapsed_ms);
    }
}

/// Creates deterministic random input
fn create_random_input(size: usize) -> Vec<f32> {
    let seed = B3Hash::hash(b"perf-test-seed");
    let seed_bytes = seed.as_bytes();

    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        let idx = i % 32;
        let byte_val = seed_bytes[idx] as f32;
        let value = ((byte_val + i as f32) % 256.0) / 256.0 - 0.5;
        data.push(value);
    }
    data
}

/// Creates a mock RouterRing for K adapters
fn create_router_ring(k: usize) -> RouterRing {
    let mut ring = RouterRing::new(k);
    let indices: Vec<u16> = (0..k as u16).collect();
    let gate_value = (32767 / k as i16).max(1);
    let gates: Vec<i16> = vec![gate_value; k];
    ring.set(&indices, &gates);
    ring
}

// =============================================================================
// REGRESSION TESTS - LATENCY BASELINES
// =============================================================================

/// Test: Inference step latency should not regress
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_inference_step_latency_baseline() {
    println!("\n=== Inference Step Latency Baseline ===");

    let config = create_mock_config();
    let model = MLXFFIModel::new_null(config);
    let mut backend = MLXFFIBackend::new(model);

    // Register 4 adapters
    for i in 0..4 {
        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
        let _ = backend.register_adapter(i, adapter);
    }

    let mut io = IoBuffers::new(32_000);
    io.input_ids = vec![42, 100, 200];
    let ring = create_router_ring(4);

    // Warm-up
    let _ = backend.run_step(&ring, &mut io);

    // Measure 10 iterations
    let timer = Timer::new("10x inference steps");
    for _ in 0..10 {
        let _ = backend.run_step(&ring, &mut io);
    }
    let total_ms = timer.elapsed_ms();
    let avg_ms = total_ms / 10.0;

    println!("  Total: {:.2}ms for 10 iterations", total_ms);
    println!("  Average: {:.2}ms per step", avg_ms);

    // Baseline regression threshold: 2.5ms max (account for variance)
    assert!(
        avg_ms < 2.5,
        "Inference step latency regressed: {:.2}ms > 2.5ms baseline",
        avg_ms
    );
}

/// Test: Tensor allocation should not regress
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_tensor_allocation_latency_baseline() {
    println!("\n=== Tensor Allocation Latency Baseline ===");

    let sizes = vec![1_024, 4_096, 16_384, 65_536];

    for size in sizes {
        let data = create_random_input(size);
        let label = format!("Allocate tensor {}B", size);

        let timer = Timer::new(&label);
        let _tensor = MLXFFITensor::from_data(&data, vec![size]);
        let elapsed_us = timer.elapsed_us();

        // Regression thresholds
        let max_us = match size {
            1_024 => 500.0,     // 500μs max for 1KB
            4_096 => 1_500.0,   // 1.5ms max for 4KB
            16_384 => 4_000.0,  // 4ms max for 16KB
            65_536 => 15_000.0, // 15ms max for 64KB
            _ => 1_000_000.0,
        };

        println!("  {}: {:.2}μs (max: {:.0}μs)", size, elapsed_us, max_us);

        assert!(
            elapsed_us < max_us,
            "Tensor allocation regressed for size {}: {:.2}μs > {:.0}μs",
            size,
            elapsed_us,
            max_us
        );
    }
}

/// Test: Adapter load latency should not regress
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_adapter_load_latency_baseline() {
    println!("\n=== Adapter Load Latency Baseline ===");

    let mut backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    for rank in &[4, 8, 16] {
        let label = format!("Load adapter rank={}", rank);
        let timer = Timer::new(&label);
        let adapter = create_mock_adapter("adapter", *rank);
        let _ = backend.register_adapter(0, adapter);
        let elapsed_ms = timer.elapsed_ms();

        // Baseline threshold: 3.5ms max (includes memory pool overhead)
        assert!(
            elapsed_ms < 3.5,
            "Adapter load latency regressed (rank={}): {:.2}ms > 3.5ms",
            rank,
            elapsed_ms
        );
    }
}

// =============================================================================
// REGRESSION TESTS - MEMORY USAGE
// =============================================================================

/// Test: Memory usage should not increase unexpectedly
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_memory_pool_baseline() {
    println!("\n=== Memory Pool Baseline ===");

    let mut backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    // Load and unload adapter multiple times
    for i in 0..5 {
        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
        let _ = backend.register_adapter(i as u16, adapter);
    }

    let stats = backend.get_memory_pool_stats();
    println!("  Total allocations: {}", stats.total_allocations);
    println!("  Pooled buffer count: {}", stats.pooled_buffer_count);
    println!("  Total adapters: {}", backend.adapter_count());

    // Should not have excessive pooled buffers
    assert!(
        stats.pooled_buffer_count < 100,
        "Memory pool pooled buffers exceeded threshold: {}",
        stats.pooled_buffer_count
    );
}

// =============================================================================
// REGRESSION TESTS - FFI OVERHEAD
// =============================================================================

/// Test: FFI boundary overhead should remain consistent
#[test]
#[ignore = "GPU memory allocation via FFI has fundamentally different latency than CPU Vec allocation"]
fn test_ffi_overhead_baseline() {
    println!("\n=== FFI Overhead Baseline ===");

    // Measure pure Rust allocation
    let timer = Timer::new("Rust Vec allocation 1024B");
    for _ in 0..100 {
        let _vec = vec![0.0f32; 256];
    }
    let rust_time_us = timer.elapsed_us();
    let rust_avg = rust_time_us / 100.0;

    // Measure FFI tensor allocation
    let data = create_random_input(1024);
    let timer = Timer::new("FFI tensor allocation 1024B");
    for _ in 0..100 {
        let _tensor = MLXFFITensor::from_data(&data, vec![1024]);
    }
    let ffi_time_us = timer.elapsed_us();
    let ffi_avg = ffi_time_us / 100.0;

    println!("  Rust avg: {:.2}μs", rust_avg);
    println!("  FFI avg: {:.2}μs", ffi_avg);
    println!("  Overhead: {:.1}%", (ffi_avg / rust_avg - 1.0) * 100.0);

    // FFI overhead should be <200% for small allocations
    let overhead_ratio = ffi_avg / rust_avg;
    assert!(
        overhead_ratio < 3.0,
        "FFI overhead excessive: {:.1}x Rust time (should be <3.0x)",
        overhead_ratio
    );
}

// =============================================================================
// REGRESSION TESTS - MULTI-ADAPTER ROUTING
// =============================================================================

/// Test: Multi-adapter routing should scale linearly with K
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_routing_scaling_baseline() {
    println!("\n=== Multi-Adapter Routing Scaling ===");

    let mut backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    // Load 32 adapters
    for i in 0..32 {
        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
        let _ = backend.register_adapter(i as u16, adapter);
    }

    let mut io = IoBuffers::new(32_000);
    io.input_ids = vec![1, 2, 3];

    // Test different K values
    for k in &[1, 2, 4, 8] {
        let ring = create_router_ring(*k);
        let k_label = k.to_string();

        let timer = Timer::new(&format!("Routing with K={}", k_label));
        for _ in 0..10 {
            let _ = backend.run_step(&ring, &mut io);
        }
        let total_ms = timer.elapsed_ms();
        let avg_ms = total_ms / 10.0;

        println!("  K={}: {:.2}ms per step", k, avg_ms);

        // Should scale roughly linearly with K
        // K=1 baseline: ~1.5ms, K=4 should be ~1.8-2.0ms
        let max_expected = 1.5 + (*k as f64 - 1.0) * 0.3;
        assert!(
            avg_ms < max_expected,
            "Routing latency exceeded expectation for K={}: {:.2}ms > {:.2}ms",
            k,
            avg_ms,
            max_expected
        );
    }
}

// =============================================================================
// REGRESSION TESTS - HEALTH CHECK
// =============================================================================

/// Test: Backend health check should return healthy status
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_backend_health_baseline() {
    println!("\n=== Backend Health Baseline ===");

    let backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    // Backend should be healthy after creation
    assert!(
        backend.is_healthy(),
        "Backend should be healthy on creation"
    );

    let health = backend.health_status();
    println!("  Operational: {}", health.operational);
    println!("  Total requests: {}", health.total_requests);
    println!("  Successful requests: {}", health.successful_requests);

    assert!(health.operational, "Backend should be operational");
    assert_eq!(health.current_failure_streak, 0, "No failures on startup");
}

// =============================================================================
// REGRESSION TESTS - ADAPTER COUNT
// =============================================================================

/// Test: Adapter registration should work efficiently
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_adapter_registration_baseline() {
    println!("\n=== Adapter Registration Baseline ===");

    let mut backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    // Register 16 adapters
    let timer = Timer::new("Register 16 adapters");
    for i in 0..16 {
        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
        let _ = backend.register_adapter(i as u16, adapter);
    }
    let elapsed_ms = timer.elapsed_ms();

    assert_eq!(
        backend.adapter_count(),
        16,
        "Should have 16 adapters registered"
    );

    println!("  Time per adapter: {:.2}ms", elapsed_ms / 16.0);

    // Should be fast: 16 adapters in <50ms total
    assert!(
        elapsed_ms < 50.0,
        "Adapter registration too slow: {:.2}ms > 50ms for 16 adapters",
        elapsed_ms
    );
}

// =============================================================================
// STRESS TESTS
// =============================================================================

/// Test: Stress test with rapid allocation/deallocation
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_stress_rapid_allocation() {
    println!("\n=== Stress Test: Rapid Allocation ===");

    let timer = Timer::new("1000 tensor allocations");
    for i in 0..1000 {
        let size = ((i % 10) + 1) * 1024; // 1KB to 10KB
        let data = create_random_input(size);
        let _tensor = MLXFFITensor::from_data(&data, vec![size]);
    }
    let elapsed_ms = timer.elapsed_ms();

    println!("  1000 allocations in {:.2}ms", elapsed_ms);
    println!("  Average: {:.2}ms per allocation", elapsed_ms / 1000.0);

    // Should complete in reasonable time (avoid memory exhaustion)
    assert!(elapsed_ms < 5000.0, "Stress test took too long");
}

/// Test: Stress test with many adapters
#[test]
#[ignore = "Performance baselines are machine-dependent; run with: cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --ignored --nocapture"]
fn test_stress_many_adapters() {
    println!("\n=== Stress Test: Many Adapters ===");

    let mut backend = {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    };

    // Load 64 adapters
    let timer = Timer::new("Load 64 adapters");
    for i in 0..64 {
        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
        let _ = backend.register_adapter(i as u16, adapter);
    }
    let elapsed_ms = timer.elapsed_ms();

    println!("  64 adapters in {:.2}ms", elapsed_ms);

    assert_eq!(backend.adapter_count(), 64, "All 64 adapters should load");
    assert!(elapsed_ms < 500.0, "Loading 64 adapters took too long");
}
