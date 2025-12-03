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
