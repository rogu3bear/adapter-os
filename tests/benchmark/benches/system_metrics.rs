#![cfg(all(test, feature = "extended-tests"))]
use adapteros_benchmarks::*;
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_policy::{PolicyContext, PolicyEngine};
use adapteros_system_metrics::{AlertingEngine, MetricsBuffer, SystemMetricsCollector};
use adapteros_telemetry::{TelemetryBuffer, TelemetryCollector};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Benchmark system metrics collection
fn bench_system_metrics_collection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let collector = Arc::new(SystemMetricsCollector::new().unwrap());
        let buffer = Arc::new(MetricsBuffer::new(1000));

        // Benchmark CPU metrics collection
        c.bench_function("cpu_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_cpu_metrics().unwrap();
                black_box(metrics);
            })
        });

        // Benchmark memory metrics collection
        c.bench_function("memory_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_memory_metrics().unwrap();
                black_box(metrics);
            })
        });

        // Benchmark disk I/O metrics collection
        c.bench_function("disk_io_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_disk_metrics().unwrap();
                black_box(metrics);
            })
        });

        // Benchmark network metrics collection
        c.bench_function("network_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_network_metrics().unwrap();
                black_box(metrics);
            })
        });

        // Benchmark comprehensive system metrics collection
        c.bench_function("comprehensive_system_metrics", |b| {
            b.iter(|| {
                let metrics = collector.collect_all_metrics().unwrap();
                black_box(metrics);
            })
        });

        // Benchmark metrics buffering
        c.bench_function("metrics_buffering_100_samples", |b| {
            b.iter(|| {
                for i in 0..100 {
                    let metrics = collector.collect_cpu_metrics().unwrap();
                    buffer.push(metrics).unwrap();
                }
                black_box(buffer.len());
            })
        });
    });
}

/// Benchmark telemetry collection and processing
fn bench_telemetry_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let telemetry = Arc::new(TelemetryCollector::new());
        let buffer = Arc::new(TelemetryBuffer::new(1000));

        // Benchmark telemetry event creation
        c.bench_function("telemetry_event_creation", |b| {
            b.iter(|| {
                let event = telemetry
                    .create_event("benchmark_test", "performance_measurement")
                    .with_metric("duration_ns", 1000000u64)
                    .with_tag("category", "kernel")
                    .build();
                black_box(event);
            })
        });

        // Benchmark telemetry batch processing
        c.bench_function("telemetry_batch_processing_100_events", |b| {
            b.iter(|| {
                let mut events = Vec::new();
                for i in 0..100 {
                    let event = telemetry
                        .create_event("batch_test", "performance_measurement")
                        .with_metric("iteration", i as u64)
                        .with_metric("timestamp", chrono::Utc::now().timestamp() as u64)
                        .build();
                    events.push(event);
                }

                let batch = telemetry.create_batch(events);
                black_box(batch);
            })
        });

        // Benchmark telemetry aggregation
        c.bench_function("telemetry_aggregation_1000_events", |b| {
            b.iter(|| {
                // Simulate collecting telemetry over time
                for i in 0..1000 {
                    let event = telemetry
                        .create_event("aggregation_test", "metric_collection")
                        .with_metric("value", (i % 100) as u64)
                        .with_tag("source", "benchmark")
                        .build();

                    buffer.push(event).unwrap();
                }

                // Aggregate by event type
                let aggregated = buffer.aggregate_by_type();
                black_box(aggregated);
            })
        });
    });
}

/// Benchmark policy evaluation performance
fn bench_policy_evaluation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let policy_engine = Arc::new(PolicyEngine::new());

        // Create sample policy context
        let context = PolicyContext {
            tenant_id: "benchmark_tenant".to_string(),
            resource_type: "inference".to_string(),
            action: "execute".to_string(),
            attributes: std::collections::HashMap::new(),
        };

        // Benchmark policy evaluation
        c.bench_function("policy_evaluation_single", |b| {
            b.iter(|| {
                let result = policy_engine.evaluate(&context).unwrap();
                black_box(result);
            })
        });

        // Benchmark batch policy evaluation
        c.bench_function("policy_evaluation_batch_10", |b| {
            b.iter(|| {
                let contexts = vec![context.clone(); 10];
                let results: Vec<_> = contexts
                    .iter()
                    .map(|ctx| policy_engine.evaluate(ctx).unwrap())
                    .collect();
                black_box(results);
            })
        });

        // Benchmark policy caching
        c.bench_function("policy_evaluation_with_cache", |b| {
            b.iter(|| {
                // First evaluation (cache miss)
                let result1 = policy_engine.evaluate(&context).unwrap();

                // Subsequent evaluations (cache hit)
                let result2 = policy_engine.evaluate(&context).unwrap();
                let result3 = policy_engine.evaluate(&context).unwrap();

                black_box((result1, result2, result3));
            })
        });
    });
}

/// Benchmark deterministic execution overhead
fn bench_deterministic_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let executor = Arc::new(DeterministicExecutor::new());

        // Benchmark deterministic execution setup
        c.bench_function("deterministic_execution_setup", |b| {
            b.iter(|| {
                let execution_id = executor.create_execution("benchmark_task").unwrap();
                black_box(execution_id);
            })
        });

        // Benchmark deterministic execution with simple computation
        c.bench_function("deterministic_execution_simple", |b| {
            b.iter(|| {
                let result = executor
                    .execute_deterministic(|| {
                        let mut sum = 0u64;
                        for i in 0..1000 {
                            sum = sum.wrapping_add(i);
                        }
                        sum
                    })
                    .unwrap();
                black_box(result);
            })
        });

        // Benchmark deterministic execution with complex computation
        c.bench_function("deterministic_execution_complex", |b| {
            b.iter(|| {
                let result = executor
                    .execute_deterministic(|| {
                        // Simulate complex computation with multiple steps
                        let mut data = vec![0u8; 10000];
                        for (i, val) in data.iter_mut().enumerate() {
                            *val = ((i * 7 + 13) % 256) as u8;
                        }

                        // Hash the result for determinism verification
                        let hash = adapteros_core::B3Hash::hash(&data);
                        hash
                    })
                    .unwrap();
                black_box(result);
            })
        });

        // Benchmark determinism verification
        c.bench_function("determinism_verification", |b| {
            b.iter(|| {
                let execution_id = executor.create_execution("verification_test").unwrap();

                // Execute same computation twice
                let result1 = executor.execute_with_id(execution_id, || 42u64).unwrap();
                let result2 = executor.execute_with_id(execution_id, || 42u64).unwrap();

                // Verify results are identical
                assert_eq!(result1, result2);

                black_box((result1, result2));
            })
        });
    });
}

/// Benchmark evidence processing and validation
fn bench_evidence_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Create sample evidence data
        let mut rng = utils::DeterministicRng::new(42);
        let evidence_scores: Vec<f32> = (0..1000).map(|_| rng.next_f32()).collect();

        // Benchmark evidence validation
        c.bench_function("evidence_validation_1000_scores", |b| {
            b.iter(|| {
                let threshold = 0.5f32;
                let mut valid_count = 0;
                let mut total_score = 0.0f32;
                let mut max_score = 0.0f32;

                for &score in &evidence_scores {
                    if score > threshold {
                        valid_count += 1;
                        total_score += score;
                        max_score = max_score.max(score);
                    }
                }

                let avg_valid_score = if valid_count > 0 {
                    total_score / valid_count as f32
                } else {
                    0.0
                };

                black_box((valid_count, avg_valid_score, max_score));
            })
        });

        // Benchmark evidence aggregation
        c.bench_function("evidence_aggregation_weighted", |b| {
            b.iter(|| {
                let mut weighted_sum = 0.0f32;
                let mut total_weight = 0.0f32;

                for (i, &score) in evidence_scores.iter().enumerate() {
                    let weight = (i as f32 + 1.0) / evidence_scores.len() as f32; // Increasing weight
                    weighted_sum += score * weight;
                    total_weight += weight;
                }

                let weighted_average = weighted_sum / total_weight;
                black_box(weighted_average);
            })
        });

        // Benchmark evidence ranking
        c.bench_function("evidence_ranking_top_k", |b| {
            b.iter(|| {
                let k = 100;
                let mut indexed_scores: Vec<(usize, f32)> = evidence_scores
                    .iter()
                    .enumerate()
                    .map(|(i, &score)| (i, score))
                    .collect();

                // Partial sort to find top K
                indexed_scores.select_nth_unstable_by(k, |a, b| {
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                });

                let top_k: Vec<_> = indexed_scores.into_iter().take(k).collect();
                black_box(top_k);
            })
        });
    });
}

/// Benchmark alerting engine performance
fn bench_alerting_engine(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let alerting = Arc::new(AlertingEngine::new());

        // Benchmark alert condition evaluation
        c.bench_function("alert_condition_evaluation", |b| {
            b.iter(|| {
                // Simulate CPU usage alert
                let cpu_usage = 85.0f32;
                let threshold = 80.0f32;

                let should_alert = cpu_usage > threshold;
                black_box(should_alert);
            })
        });

        // Benchmark alert rule processing
        c.bench_function("alert_rule_processing_10_rules", |b| {
            b.iter(|| {
                let metrics = vec![
                    ("cpu_usage", 85.0f32),
                    ("memory_usage", 90.0f32),
                    ("disk_usage", 75.0f32),
                    ("network_errors", 5.0f32),
                    ("response_time", 150.0f32),
                ];

                let rules = vec![
                    ("cpu_high", |v: f32| v > 80.0),
                    ("memory_high", |v: f32| v > 85.0),
                    ("disk_high", |v: f32| v > 90.0),
                    ("network_errors", |v: f32| v > 10.0),
                    ("response_slow", |v: f32| v > 100.0),
                ];

                let mut triggered_alerts = Vec::new();

                for (metric_name, value) in &metrics {
                    for (rule_name, condition) in &rules {
                        if condition(*value) {
                            triggered_alerts.push(format!("{}: {}", rule_name, metric_name));
                        }
                    }
                }

                black_box(triggered_alerts);
            })
        });

        // Benchmark alert deduplication
        c.bench_function("alert_deduplication_100_alerts", |b| {
            b.iter(|| {
                let mut alerts = Vec::new();

                // Generate duplicate alerts
                for i in 0..100 {
                    alerts.push(format!("CPU high: {}%", 80 + (i % 10)));
                }

                // Deduplicate based on pattern
                let mut deduplicated = std::collections::HashSet::new();
                let mut unique_alerts = Vec::new();

                for alert in alerts {
                    if deduplicated.insert(alert.clone()) {
                        unique_alerts.push(alert);
                    }
                }

                black_box(unique_alerts.len());
            })
        });
    });
}

criterion_group!(
    name = system_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(15))
        .noise_threshold(0.05);
    targets = bench_system_metrics_collection, bench_telemetry_processing, bench_policy_evaluation,
             bench_deterministic_execution, bench_evidence_processing, bench_alerting_engine
);

criterion_main!(system_benches);
