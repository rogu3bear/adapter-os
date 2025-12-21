#![cfg(all(test, feature = "extended-tests"))]

//! Soak test framework for extended load testing
//!
//! Run with: cargo test --test soak_test --release -- --ignored --nocapture
//!
//! Configure via environment variables:
//! - SOAK_DURATION_SECS: Test duration (default: 3600 = 1 hour)
//! - SOAK_QPS: Queries per second (default: 10)
//! - SOAK_MAX_MEMORY_MB: Maximum memory growth allowed (default: 100)

use adapteros_deterministic_exec::{init_global_executor, spawn_deterministic, ExecutorConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::test]
async fn soak_test_memory_stability() {
    let duration = std::env::var("SOAK_DURATION_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600); // 1 hour default

    let qps = std::env::var("SOAK_QPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    println!("\n[FIRE] Starting soak test");
    println!(
        "   Duration: {} seconds ({} minutes)",
        duration,
        duration / 60
    );
    println!("   Target QPS: {}", qps);
    println!("");

    let request_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(duration);

    // Measure baseline memory
    let baseline_memory = get_process_memory_mb();
    println!("   Baseline memory: {} MB\n", baseline_memory);

    let mut interval = tokio::time::interval(Duration::from_millis(1000 / qps as u64));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Main test loop
    while Instant::now() < end_time {
        interval.tick().await;

        let count = request_count.clone();
        let errors = error_count.clone();

        // Spawn request task
        spawn_deterministic("Soak test request", async move {
            match simulate_inference_request().await {
                Ok(_) => {
                    count.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!("Request error: {}", e);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        })?;

        // Print stats every 60 seconds
        let elapsed = start_time.elapsed().as_secs();
        if elapsed > 0 && elapsed % 60 == 0 {
            let total_requests = request_count.load(Ordering::Relaxed);
            let total_errors = error_count.load(Ordering::Relaxed);
            let current_memory = get_process_memory_mb();
            let memory_growth = current_memory - baseline_memory;

            println!(
                "[{}min] Requests: {}, Errors: {}, Memory: {} MB (+{} MB)",
                elapsed / 60,
                total_requests,
                total_errors,
                current_memory,
                memory_growth
            );

            // Check for memory leak
            let max_growth = std::env::var("SOAK_MAX_MEMORY_MB")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);

            if memory_growth > max_growth {
                panic!(
                    "Memory growth {} MB exceeds threshold {} MB - potential leak!",
                    memory_growth, max_growth
                );
            }
        }
    }

    // Final statistics
    let total_requests = request_count.load(Ordering::Relaxed);
    let total_errors = error_count.load(Ordering::Relaxed);
    let final_memory = get_process_memory_mb();
    let actual_qps = total_requests as f64 / duration as f64;

    println!("\n✓ Soak test completed");
    println!("   Total requests: {}", total_requests);
    println!("   Total errors: {}", total_errors);
    println!("   Actual QPS: {:.2}", actual_qps);
    println!("   Memory growth: {} MB", final_memory - baseline_memory);
    println!("");

    assert!(total_requests > 0, "No requests completed during soak test");
    assert!(
        total_errors as f64 / total_requests as f64 < 0.01,
        "Error rate too high: {} errors out of {} requests",
        total_errors,
        total_requests
    );
}

/// Simulate an inference request
async fn simulate_inference_request() -> Result<(), Box<dyn std::error::Error>> {
    // In production, this would call the actual inference API
    // For now, simulate work
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Simulate occasional errors (1% error rate)
    if rand::random::<f64>() < 0.01 {
        return Err("Simulated error".into());
    }

    Ok(())
}

/// Get current process memory usage in MB (simplified)
fn get_process_memory_mb() -> usize {
    // On macOS/Linux, read from /proc/self/status or use system APIs
    // For now, return a stub value

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("ps")
            .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
        {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if let Ok(kb) = s.trim().parse::<usize>() {
                    return kb / 1024; // Convert KB to MB
                }
            }
        }
    }

    // Fallback
    100
}
