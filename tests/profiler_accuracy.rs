//! Tests for profiler metric collection accuracy

use adapteros_profiler::{AdapterProfiler, AdapterMetrics};
use std::time::Duration;

#[test]
fn test_activation_tracking() {
    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Record some routing decisions
    profiler.record_routing_decision(&[0, 1]); // Adapters 0 and 1 active
    profiler.record_routing_decision(&[0, 2]); // Adapters 0 and 2 active
    profiler.record_routing_decision(&[0, 1]); // Adapters 0 and 1 active
    profiler.record_routing_decision(&[1, 2]); // Adapters 1 and 2 active

    let metrics = profiler.get_all_metrics();

    // Adapter 0 should have 3 activations (75%)
    assert_eq!(metrics[0].activation_count, 3);
    assert!((metrics[0].activation_pct - 75.0).abs() < 0.1);

    // Adapter 1 should have 3 activations (75%)
    assert_eq!(metrics[1].activation_count, 3);
    assert!((metrics[1].activation_pct - 75.0).abs() < 0.1);

    // Adapter 2 should have 2 activations (50%)
    assert_eq!(metrics[2].activation_count, 2);
    assert!((metrics[2].activation_pct - 50.0).abs() < 0.1);
}

#[test]
fn test_latency_tracking() {
    let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
    let profiler = AdapterProfiler::new(adapter_names, None);

    // Record latencies for adapter 0
    profiler.record_step_latency(0, Duration::from_micros(100));
    profiler.record_step_latency(0, Duration::from_micros(200));
    profiler.record_step_latency(0, Duration::from_micros(300));

    // Record latencies for adapter 1
    profiler.record_step_latency(1, Duration::from_micros(150));
    profiler.record_step_latency(1, Duration::from_micros(250));

    let metrics = profiler.get_all_metrics();

    // Adapter 0 average: (100 + 200 + 300) / 3 = 200
    assert!((metrics[0].avg_latency_us - 200.0).abs() < 1.0);

    // Adapter 1 average: (150 + 250) / 2 = 200
    assert!((metrics[1].avg_latency_us - 200.0).abs() < 1.0);
}

#[test]
fn test_memory_tracking() {
    let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];
    let profiler = AdapterProfiler::new(adapter_names, None);

    // Update memory usage
    profiler.update_memory_usage(0, 16 * 1024 * 1024); // 16 MB
    profiler.update_memory_usage(1, 32 * 1024 * 1024); // 32 MB

    let metrics = profiler.get_all_metrics();

    assert_eq!(metrics[0].memory_bytes, 16 * 1024 * 1024);
    assert_eq!(metrics[1].memory_bytes, 32 * 1024 * 1024);
}

#[test]
fn test_quality_delta_tracking() {
    let adapter_names = vec!["adapter_0".to_string()];
    let profiler = AdapterProfiler::new(adapter_names, None);

    // Update quality delta
    profiler.update_quality_delta(0, 0.75);

    let metrics = profiler.get_all_metrics();
    assert!((metrics[0].quality_delta - 0.75).abs() < 0.01);
}

#[test]
fn test_ranking() {
    let adapter_names = vec![
        "low".to_string(),
        "high".to_string(),
        "medium".to_string(),
    ];

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Set up different activation patterns
    for _ in 0..10 {
        profiler.record_routing_decision(&[1]); // High activation for adapter 1
    }
    for _ in 0..5 {
        profiler.record_routing_decision(&[2]); // Medium activation for adapter 2
    }
    for _ in 0..1 {
        profiler.record_routing_decision(&[0]); // Low activation for adapter 0
    }

    let ranked = profiler.get_ranked_adapters();

    // Should be ranked by score: high (1) > medium (2) > low (0)
    assert_eq!(ranked[0].0, 1); // Highest ranked is adapter 1
    assert_eq!(ranked[1].0, 2); // Second is adapter 2
    assert_eq!(ranked[2].0, 0); // Lowest is adapter 0

    // Scores should be descending
    assert!(ranked[0].1 > ranked[1].1);
    assert!(ranked[1].1 > ranked[2].1);
}

#[test]
fn test_rolling_window() {
    let adapter_names = vec!["adapter_0".to_string()];
    let profiler = AdapterProfiler::new(adapter_names, None);

    // Fill the window (default 1000 activations)
    for _ in 0..1500 {
        profiler.record_routing_decision(&[0]);
    }

    let metrics = profiler.get_all_metrics();

    // Should have window size activations (1000), not 1500
    assert_eq!(metrics[0].total_tokens, 1000);
    assert_eq!(metrics[0].activation_count, 1000);
}

