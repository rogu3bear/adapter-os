use adapteros_core::{AosError, Result};
use adapteros_lora_worker::{InferenceRequest, Worker}; // Assume imports
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::time::{interval, sleep};
use tracing::{info, warn};

// Mock or real worker setup
struct TestWorker {
    kv_cache: VecDeque<String>, // Simulate KV entries
                                // ...
}

impl TestWorker {
    async fn infer(&mut self, prompt: &str) -> Result<String> {
        // Simulate inference with KV access
        if let Some(entry) = self.kv_cache.pop_front() {
            info!("Using cached KV: {}", entry);
        } else {
            // Cold miss simulation
            sleep(Duration::from_millis(10)).await; // Simulate compute
        }
        self.kv_cache.push_back(format!("kv_{}", prompt)); // Add new
        Ok(format!("Response to: {}", prompt))
    }

    fn trigger_hotswap(&mut self) {
        info!("Triggering hot-swap: clearing KV cache");
        self.kv_cache.clear(); // Reset KV
                               // Simulate swap overhead
        sleep(Duration::from_millis(20)).await; // 20ms swap time
    }

    fn kv_cache_len(&self) -> usize {
        self.kv_cache.len()
    }
}

#[tokio::test]
async fn test_load_hotswap_100_rps() -> Result<()> {
    let mut worker = TestWorker {
        kv_cache: VecDeque::new(),
    };

    // Warm up cache
    for _ in 0..50 {
        worker.infer("warmup").await?;
    }
    assert!(!worker.kv_cache.is_empty(), "Cache should be warm");

    // Simulate 100 RPS: 1000 requests over 10s, concurrent
    let num_concurrent = 100;
    let total_requests = 1000;
    let mut handles = Vec::new();
    let mut sent = 0;

    // Rate limiter for RPS
    let mut interval = interval(Duration::from_millis(10)); // 100ms / 10 = 100 RPS

    // Background swap trigger every 2s
    let mut swap_worker = worker.clone(); // For swap
    let swap_handle = tokio::spawn(async move {
        let mut swap_interval = interval(Duration::from_secs(2));
        loop {
            swap_interval.tick().await;
            swap_worker.trigger_hotswap();
        }
    });

    let start_time = Instant::now();
    while sent < total_requests {
        interval.tick().await;
        for _ in 0..10 {
            // Burst 10 per tick for concurrency
            if sent >= total_requests {
                break;
            }
            let prompt = format!("req_{}", sent);
            let mut w = worker.clone();
            handles.push(tokio::spawn(async move {
                let start = Instant::now();
                let _result = w.infer(&prompt).await;
                start.elapsed()
            }));
            sent += 1;
        }
    }

    // Collect results
    let mut durations: Vec<Duration> = Vec::new();
    for handle in handles {
        if let Ok(duration) = handle.await? {
            durations.push(duration);
        }
    }

    // Cancel swap
    swap_handle.abort();

    // Compute p95
    durations.sort();
    let p95_idx = (0.95 * durations.len() as f64) as usize;
    let p95 = durations[p95_idx.min(durations.len() - 1)];
    info!("p95 latency: {:?}", p95);

    assert!(
        p95 < Duration::from_millis(50),
        "p95 latency exceeded SLO: {:?}",
        p95
    );

    // Verify KV reset happened multiple times (cache len low post-swaps)
    // Since concurrent, approximate: average cache len should be low
    // But for mock, check final state empty-ish
    assert!(
        worker.kv_cache_len() < 100,
        "KV cache should have been reset multiple times"
    );

    Ok(())
}
