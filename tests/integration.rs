use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct KvCache {
    is_warm: bool,
}

struct HotSwap {
    kv_cache: Arc<Mutex<KvCache>>,
}

impl HotSwap {
    async fn swap(&self, _new_adapters: Vec<&str>, _old_adapters: Vec<&str>) -> TestResult<()> {
        let mut cache = self.kv_cache.lock().await;
        cache.is_warm = false;
        Ok(())
    }
}

struct TestWorker {
    kv_cache: Arc<Mutex<KvCache>>,
    pub hotswap: HotSwap,
}

impl TestWorker {
    async fn infer(&self, prompt: String) -> TestResult<String> {
        let mut cache = self.kv_cache.lock().await;
        if !cache.is_warm {
            // Simulate a cold-path inference cost
            sleep(Duration::from_millis(30)).await;
            cache.is_warm = true;
        } else {
            // Simulate a warm cache hit
            sleep(Duration::from_millis(5)).await;
        }

        Ok(format!("response: {}", prompt))
    }
}

async fn create_test_worker() -> TestResult<TestWorker> {
    let kv_cache = Arc::new(Mutex::new(KvCache { is_warm: false }));

    Ok(TestWorker {
        kv_cache: kv_cache.clone(),
        hotswap: HotSwap { kv_cache },
    })
}

// Assume test setup with mock worker

#[tokio::test]
async fn test_kv_cache_reset_on_swap() {
    // Setup: Create worker with KV cache, load initial adapters
    let worker = create_test_worker().await.unwrap();
    let prompt = "Hello world"; // Simple prompt

    // First inference: Cold start
    let start_time = Instant::now();
    let _result1 = worker.infer(prompt.to_string()).await.unwrap();
    let cold_time = start_time.elapsed();

    // Second inference: Should use cache, faster
    let start_time = Instant::now();
    let _result2 = worker.infer(prompt.to_string() + " again").await.unwrap(); // Extend to use cache
    let cached_time = start_time.elapsed();
    assert!(cached_time < cold_time, "Cached should be faster");

    // Swap adapters (simulate hash change)
    worker
        .hotswap
        .swap(vec!["new_adapter"], vec!["old_adapter"])
        .await
        .unwrap();

    // Third inference: After swap, should be cold again
    let start_time = Instant::now();
    let _result3 = worker.infer(prompt.to_string()).await.unwrap();
    let post_swap_time = start_time.elapsed();
    assert!(
        post_swap_time > cached_time,
        "Post-swap should reset cache, slower like cold"
    );

    // Verify KV allocations cleared
    // assert!(worker.kv_cache.allocations.is_empty()); // If accessible
}
