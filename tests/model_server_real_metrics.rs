//! Model Server Real Metrics Integration Test
//!
//! Measures actual GPU/unified memory usage and inference latency for the
//! Model Server architecture. Run with:
//!
//! ```bash
//! # Quick test (mock model, memory overhead only)
//! cargo test --test model_server_real_metrics -- --ignored --nocapture
//!
//! # Full test with real model (requires AOS_MODEL_PATH)
//! AOS_MODEL_PATH=/var/models/Llama-3.2-3B-Instruct-4bit \
//!   cargo test --test model_server_real_metrics --features mlx -- --ignored --nocapture
//! ```

use std::process::Command;
use std::time::{Duration, Instant};

/// Memory snapshot from the system
#[derive(Debug, Clone)]
struct MemorySnapshot {
    /// Resident memory in bytes (from ps)
    rss_bytes: u64,
    /// Virtual memory in bytes
    vsz_bytes: u64,
    /// Metal GPU memory (if available via ioreg)
    gpu_bytes: Option<u64>,
    /// Unified memory pressure
    memory_pressure: Option<String>,
}

impl MemorySnapshot {
    fn capture() -> Self {
        let pid = std::process::id();

        // Get RSS and VSZ via ps
        let (rss_bytes, vsz_bytes) = Self::get_process_memory(pid);

        // Try to get GPU memory via ioreg (Metal)
        let gpu_bytes = Self::get_metal_memory();

        // Get memory pressure
        let memory_pressure = Self::get_memory_pressure();

        Self {
            rss_bytes,
            vsz_bytes,
            gpu_bytes,
            memory_pressure,
        }
    }

    fn get_process_memory(pid: u32) -> (u64, u64) {
        let output = Command::new("ps")
            .args(["-o", "rss=,vsz=", "-p", &pid.to_string()])
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.split_whitespace().collect();
                if parts.len() >= 2 {
                    let rss = parts[0].parse::<u64>().unwrap_or(0) * 1024; // KB to bytes
                    let vsz = parts[1].parse::<u64>().unwrap_or(0) * 1024;
                    return (rss, vsz);
                }
            }
        }
        (0, 0)
    }

    fn get_metal_memory() -> Option<u64> {
        // Try to get Metal GPU memory via ioreg
        let output = Command::new("ioreg")
            .args(["-l", "-w0", "-r", "-c", "IOAccelerator"])
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Look for "PerformanceStatistics" -> "Alloc system memory"
            for line in stdout.lines() {
                if line.contains("Alloc system memory") {
                    // Extract the number
                    if let Some(start) = line.find("= ") {
                        let num_str = &line[start + 2..];
                        if let Ok(bytes) = num_str.trim().parse::<u64>() {
                            return Some(bytes);
                        }
                    }
                }
            }
        }
        None
    }

    fn get_memory_pressure() -> Option<String> {
        let output = Command::new("memory_pressure").output().ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Extract the pressure level line
            for line in stdout.lines() {
                if line.contains("System-wide memory free percentage") {
                    return Some(line.to_string());
                }
            }
        }
        None
    }

    fn format_bytes(bytes: u64) -> String {
        if bytes >= 1024 * 1024 * 1024 {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes >= 1024 * 1024 {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:.2} KB", bytes as f64 / 1024.0)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// Latency statistics
#[derive(Debug, Clone)]
struct LatencyStats {
    count: usize,
    min_ms: f64,
    max_ms: f64,
    avg_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

impl LatencyStats {
    fn from_samples(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self {
                count: 0,
                min_ms: 0.0,
                max_ms: 0.0,
                avg_ms: 0.0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
            };
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let count = samples.len();
        let sum: f64 = samples.iter().sum();

        Self {
            count,
            min_ms: samples[0],
            max_ms: samples[count - 1],
            avg_ms: sum / count as f64,
            p50_ms: samples[count / 2],
            p95_ms: samples[(count as f64 * 0.95) as usize],
            p99_ms: samples[((count as f64 * 0.99) as usize).min(count - 1)],
        }
    }
}

/// Benchmark results for a scenario
#[derive(Debug)]
struct BenchmarkResults {
    name: String,
    memory_before: MemorySnapshot,
    memory_after: MemorySnapshot,
    memory_delta_bytes: i64,
    latency: LatencyStats,
    throughput_rps: f64,
}

#[test]
#[ignore] // Run with: cargo test --test model_server_real_metrics -- --ignored --nocapture
fn test_model_server_data_structure_memory() {
    println!("\n=== Model Server Data Structure Memory Test ===\n");

    let before = MemorySnapshot::capture();
    println!(
        "Before: RSS={}, VSZ={}",
        MemorySnapshot::format_bytes(before.rss_bytes),
        MemorySnapshot::format_bytes(before.vsz_bytes)
    );

    // Create model server data structures with realistic sizes
    use adapteros_model_server::{
        activation_tracker::ActivationTracker, adapter_cache::AdapterCache,
        kv_cache::KvCacheManager,
    };

    // KV Cache: 4GB limit, 4096 hidden, 32 layers
    let kv_cache = KvCacheManager::new(4 * 1024 * 1024 * 1024, 4096, 32);

    // Pre-populate with sessions
    let session_count = 100;
    for i in 0..session_count {
        kv_cache.get_or_create(&format!("session-{}", i), 2048);
    }

    // Adapter cache: 256 max adapters
    let adapter_cache = AdapterCache::new(256, None);

    // Load some adapters (rank=8, hidden=4096)
    let lora_a = vec![0.01f32; 8 * 4096]; // 128KB per adapter
    let lora_b = vec![0.01f32; 4096 * 8];
    for i in 0..32u32 {
        let _ = adapter_cache.load(
            i,
            format!("adapter-{}", i),
            lora_a.clone(),
            lora_b.clone(),
            1.0,
        );
    }

    // Activation tracker: 128 adapters
    let tracker = ActivationTracker::new(0.10);
    for i in 0..128u32 {
        tracker.register_adapter(i, format!("adapter-{}", i));
    }

    // Simulate traffic
    for _ in 0..10000 {
        tracker.record_request(&[0, 1, 2, 3]); // 4 adapters per request
    }

    let after = MemorySnapshot::capture();
    let delta = after.rss_bytes as i64 - before.rss_bytes as i64;

    println!(
        "After:  RSS={}, VSZ={}",
        MemorySnapshot::format_bytes(after.rss_bytes),
        MemorySnapshot::format_bytes(after.vsz_bytes)
    );
    println!(
        "Delta:  {} ({})",
        MemorySnapshot::format_bytes(delta.unsigned_abs()),
        if delta >= 0 { "+" } else { "-" }
    );

    println!("\n--- Data Structure Sizes ---");
    println!("KV Cache: {} sessions", session_count);
    println!("Adapter Cache: 32 adapters loaded");
    println!("Activation Tracker: 128 adapters, 10k requests");

    // Report stats
    let stats = kv_cache.stats();
    println!("\nKV Cache Stats:");
    println!("  Active sessions: {}", stats.active_sessions);
    println!(
        "  Memory used: {}",
        MemorySnapshot::format_bytes(stats.used_bytes)
    );
    println!("  Hit rate: {:.2}%", stats.hit_rate() * 100.0);

    let adapter_stats = adapter_cache.stats();
    println!("\nAdapter Cache Stats:");
    println!("  Cached: {}", adapter_stats.cached_adapters);
    println!("  Loads: {}", adapter_stats.loads);
    println!("  Fusions: {}", adapter_stats.fusions);

    let hot = tracker.hot_adapters();
    println!("\nActivation Tracker Stats:");
    println!("  Hot adapters: {}", hot.len());
    println!("  Total requests: {}", tracker.total_requests());

    // Keep structures alive for measurement
    drop(kv_cache);
    drop(adapter_cache);
    drop(tracker);
}

#[test]
#[ignore]
fn test_forward_pass_latency() {
    println!("\n=== Forward Pass Latency Test ===\n");

    use adapteros_model_server::{
        adapter_cache::AdapterCache, forward::ForwardExecutor, forward::ForwardPassRequest,
        kv_cache::KvCacheManager,
    };
    use std::sync::Arc;

    let kv_cache = Arc::new(KvCacheManager::new(4 * 1024 * 1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());

    let executor = ForwardExecutor::new(
        kv_cache.clone(),
        adapter_cache.clone(),
        128000, // 128K vocab (Llama 3.2)
        4096,   // hidden size
        32,     // layers
    );

    // Warm up
    for i in 0..10 {
        let request = ForwardPassRequest {
            session_id: format!("warmup-{}", i),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };
        let _ = executor.forward(request);
    }

    // Benchmark cold start (new sessions)
    let mut cold_latencies = Vec::new();
    let cold_iterations = 100;
    let cold_start = Instant::now();

    for i in 0..cold_iterations {
        let request = ForwardPassRequest {
            session_id: format!("cold-{}", i),
            input_ids: vec![1, 2, 3, 4, 5],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let start = Instant::now();
        let _ = executor.forward(request);
        cold_latencies.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let cold_total = cold_start.elapsed();
    let cold_stats = LatencyStats::from_samples(cold_latencies);

    println!("Cold Start (new sessions):");
    println!("  Iterations: {}", cold_iterations);
    println!("  Total time: {:?}", cold_total);
    println!("  Min: {:.3} ms", cold_stats.min_ms);
    println!("  Avg: {:.3} ms", cold_stats.avg_ms);
    println!("  P50: {:.3} ms", cold_stats.p50_ms);
    println!("  P95: {:.3} ms", cold_stats.p95_ms);
    println!("  P99: {:.3} ms", cold_stats.p99_ms);
    println!("  Max: {:.3} ms", cold_stats.max_ms);
    println!(
        "  Throughput: {:.0} req/s",
        cold_iterations as f64 / cold_total.as_secs_f64()
    );

    // Benchmark warm (cached sessions)
    let mut warm_latencies = Vec::new();
    let warm_iterations = 1000;
    let warm_start = Instant::now();

    for i in 0..warm_iterations {
        let session_id = format!("warm-{}", i % 10); // Reuse 10 sessions
        let request = ForwardPassRequest {
            session_id,
            input_ids: vec![1],
            position: (i * 5) as u32,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let start = Instant::now();
        let _ = executor.forward(request);
        warm_latencies.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let warm_total = warm_start.elapsed();
    let warm_stats = LatencyStats::from_samples(warm_latencies);

    println!("\nWarm (cached sessions):");
    println!("  Iterations: {}", warm_iterations);
    println!("  Total time: {:?}", warm_total);
    println!("  Min: {:.3} ms", warm_stats.min_ms);
    println!("  Avg: {:.3} ms", warm_stats.avg_ms);
    println!("  P50: {:.3} ms", warm_stats.p50_ms);
    println!("  P95: {:.3} ms", warm_stats.p95_ms);
    println!("  P99: {:.3} ms", warm_stats.p99_ms);
    println!("  Max: {:.3} ms", warm_stats.max_ms);
    println!(
        "  Throughput: {:.0} req/s",
        warm_iterations as f64 / warm_total.as_secs_f64()
    );

    // Report KV cache stats
    let stats = kv_cache.stats();
    println!("\nKV Cache Performance:");
    println!("  Hit rate: {:.2}%", stats.hit_rate() * 100.0);
    println!("  Active sessions: {}", stats.active_sessions);
}

#[test]
#[ignore]
fn test_concurrent_forward_throughput() {
    println!("\n=== Concurrent Forward Throughput Test ===\n");

    use adapteros_model_server::{
        adapter_cache::AdapterCache, forward::ForwardExecutor, forward::ForwardPassRequest,
        kv_cache::KvCacheManager,
    };
    use std::sync::Arc;
    use std::thread;

    let kv_cache = Arc::new(KvCacheManager::new(4 * 1024 * 1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());
    let executor: Arc<ForwardExecutor> = Arc::new(ForwardExecutor::new(
        kv_cache.clone(),
        adapter_cache.clone(),
        128000,
        4096,
        32,
    ));

    for num_threads in [1, 2, 4, 8] {
        let iterations_per_thread = 500;
        let start = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let executor: Arc<ForwardExecutor> = Arc::clone(&executor);
                thread::spawn(move || {
                    let mut latencies = Vec::with_capacity(iterations_per_thread);
                    for i in 0..iterations_per_thread {
                        let request = ForwardPassRequest {
                            session_id: format!("thread-{}-session-{}", t, i % 50),
                            input_ids: vec![1, 2, 3],
                            position: 0,
                            max_seq_len: 2048,
                            adapter_ids: vec![],
                            adapter_gates_q15: vec![],
                            include_hidden_states: false,
                            manifest_seed: None,
                        };

                        let req_start = Instant::now();
                        let _ = executor.forward(request);
                        latencies.push(req_start.elapsed().as_secs_f64() * 1000.0);
                    }
                    latencies
                })
            })
            .collect();

        let mut all_latencies = Vec::new();
        for handle in handles {
            all_latencies.extend(handle.join().unwrap());
        }

        let total_time = start.elapsed();
        let total_requests = num_threads * iterations_per_thread;
        let stats = LatencyStats::from_samples(all_latencies);

        println!("{} threads:", num_threads);
        println!("  Total requests: {}", total_requests);
        println!("  Total time: {:?}", total_time);
        println!(
            "  Throughput: {:.0} req/s",
            total_requests as f64 / total_time.as_secs_f64()
        );
        println!(
            "  Latency - P50: {:.3} ms, P95: {:.3} ms, P99: {:.3} ms",
            stats.p50_ms, stats.p95_ms, stats.p99_ms
        );
    }
}

#[test]
#[ignore]
fn test_memory_with_real_model() {
    println!("\n=== Real Model Memory Test ===\n");

    // Check for model path
    let model_path = std::env::var("AOS_MODEL_PATH").ok();
    if model_path.is_none() {
        println!("Skipping real model test - AOS_MODEL_PATH not set");
        println!("Set AOS_MODEL_PATH to a model directory to run this test");
        return;
    }

    let model_path = model_path.unwrap();
    println!("Model path: {}", model_path);

    let before = MemorySnapshot::capture();
    println!(
        "\nBefore model load: RSS={}, GPU={:?}",
        MemorySnapshot::format_bytes(before.rss_bytes),
        before.gpu_bytes.map(MemorySnapshot::format_bytes)
    );

    // This requires the mlx feature
    #[cfg(feature = "mlx")]
    {
        use adapteros_model_server::{
            adapter_cache::AdapterCache, forward::ForwardExecutor, kv_cache::KvCacheManager,
        };
        use std::path::Path;
        use std::sync::Arc;

        let kv_cache = Arc::new(KvCacheManager::new(4 * 1024 * 1024 * 1024, 4096, 32));
        let adapter_cache = Arc::new(AdapterCache::with_defaults());

        let mut executor =
            ForwardExecutor::new(kv_cache.clone(), adapter_cache.clone(), 128000, 4096, 32);

        println!("\nLoading model...");
        let load_start = Instant::now();

        match executor.load_model(Path::new(&model_path)) {
            Ok(()) => {
                let load_time = load_start.elapsed();
                println!("Model loaded in {:?}", load_time);

                let after = MemorySnapshot::capture();
                let delta = after.rss_bytes as i64 - before.rss_bytes as i64;

                println!(
                    "\nAfter model load: RSS={}, GPU={:?}",
                    MemorySnapshot::format_bytes(after.rss_bytes),
                    after.gpu_bytes.map(MemorySnapshot::format_bytes)
                );
                println!(
                    "Memory delta: {} {}",
                    MemorySnapshot::format_bytes(delta.unsigned_abs()),
                    if delta >= 0 {
                        "(increase)"
                    } else {
                        "(decrease)"
                    }
                );

                if let (Some(gpu_before), Some(gpu_after)) = (before.gpu_bytes, after.gpu_bytes) {
                    let gpu_delta = gpu_after as i64 - gpu_before as i64;
                    println!(
                        "GPU memory delta: {} {}",
                        MemorySnapshot::format_bytes(gpu_delta.unsigned_abs()),
                        if gpu_delta >= 0 {
                            "(increase)"
                        } else {
                            "(decrease)"
                        }
                    );
                }
            }
            Err(e) => {
                println!("Failed to load model: {}", e);
            }
        }
    }

    #[cfg(not(feature = "mlx"))]
    {
        println!("\nMLX feature not enabled. Rebuild with --features mlx");
    }
}

/// Print summary report
#[test]
#[ignore]
fn test_full_benchmark_suite() {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║          Model Server Real Metrics Benchmark Suite             ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    test_model_server_data_structure_memory();
    test_forward_pass_latency();
    test_concurrent_forward_throughput();
    test_memory_with_real_model();

    println!("\n");
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║                      Benchmark Complete                        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
}
