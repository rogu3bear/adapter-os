// System metrics benchmarks
use adapteros_benchmarks::*;
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_model_hub::manifest::Policies;
use adapteros_policy::PolicyEngine;
use adapteros_system_metrics::SystemMetricsCollector;
use adapteros_telemetry::{
    AlertingEngine, EventType, LogLevel, TelemetryEventBuilder,
    TelemetryRingBuffer as MetricsBuffer,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Benchmark system metrics collection
fn bench_system_metrics_collection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let mut collector = SystemMetricsCollector::new();
        let buffer = Arc::new(MetricsBuffer::new(1000));

        // Benchmark CPU metrics collection
        c.bench_function("cpu_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_metrics();
                black_box(metrics);
            })
        });

        // Benchmark memory metrics collection
        c.bench_function("memory_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_metrics();
                black_box(metrics);
            })
        });

        // Benchmark disk I/O metrics collection
        c.bench_function("disk_io_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_metrics();
                black_box(metrics);
            })
        });

        // Benchmark network metrics collection
        c.bench_function("network_metrics_collection", |b| {
            b.iter(|| {
                let metrics = collector.collect_metrics();
                black_box(metrics);
            })
        });

        // Benchmark comprehensive system metrics collection
        c.bench_function("comprehensive_system_metrics", |b| {
            b.iter(|| {
                let metrics = collector.collect_metrics();
                black_box(metrics);
            })
        });

        // Benchmark metrics buffering
        c.bench_function("metrics_buffering_100_samples", |b| {
            b.iter(|| {
                for _ in 0..100 {
                    let metrics = collector.collect_metrics();
                    let identity = adapteros_core::identity::IdentityEnvelope::new(
                        "test".to_string(),
                        "telemetry".to_string(),
                        "benchmark".to_string(),
                        "1.0".to_string(),
                    );
                    let event = TelemetryEventBuilder::new(
                        EventType::SystemStart,
                        LogLevel::Info,
                        "benchmark".to_string(),
                        identity,
                    )
                    .metadata(serde_json::to_value(metrics).unwrap())
                    .build()
                    .unwrap();

                    let _ = rt.block_on(buffer.push(event));
                }
            })
        });
    });
}

/// Benchmark telemetry collection and processing
fn bench_telemetry_processing(c: &mut Criterion) {
    // Benchmark telemetry event creation
    c.bench_function("telemetry_event_creation", |b| {
        b.iter(|| {
            let identity = adapteros_core::identity::IdentityEnvelope::new(
                "test".to_string(),
                "telemetry".to_string(),
                "benchmark".to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::Custom("benchmark".to_string()),
                LogLevel::Info,
                "performance_measurement".to_string(),
                identity,
            )
            .metadata(serde_json::json!({"duration_ns": 1000000u64, "category": "kernel"}))
            .build()
            .unwrap();
            black_box(event);
        })
    });

    // Benchmark telemetry batch processing
    c.bench_function("telemetry_batch_processing_100_events", |b| {
        b.iter(|| {
            let mut events = Vec::new();
            for i in 0..100 {
                let identity = adapteros_core::identity::IdentityEnvelope::new(
                    "test".to_string(),
                    "telemetry".to_string(),
                    "benchmark".to_string(),
                    "1.0".to_string(),
                );
                let event = TelemetryEventBuilder::new(
                    EventType::Custom("batch_test".to_string()),
                    LogLevel::Info,
                    "performance_measurement".to_string(),
                    identity,
                )
                .metadata(serde_json::json!({"iteration": i as u64}))
                .build()
                .unwrap();
                events.push(event);
            }
            black_box(events);
        })
    });
}

/// Benchmark policy evaluation performance
fn bench_policy_evaluation(c: &mut Criterion) {
    let _policy_engine = Arc::new(PolicyEngine::new(Policies::default()));

    // Benchmark policy evaluation
    c.bench_function("policy_evaluation_single", |b| {
        b.iter(|| {
            _policy_engine.check_system_thresholds(50.0, 60.0).unwrap();
            black_box(());
        })
    });

    // Benchmark memory headroom policy
    c.bench_function("policy_memory_headroom", |b| {
        b.iter(|| {
            _policy_engine.check_memory_headroom(20.0).unwrap();
            black_box(());
        })
    });
}

/// Benchmark deterministic execution overhead
fn bench_deterministic_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let executor = Arc::new(DeterministicExecutor::new(
            adapteros_deterministic_exec::ExecutorConfig::default(),
        ));

        // Benchmark deterministic task spawning
        c.bench_function("deterministic_task_spawn", |b| {
            b.iter(|| {
                let identity = adapteros_core::identity::IdentityEnvelope::new(
                    "test".to_string(),
                    "telemetry".to_string(),
                    "benchmark".to_string(),
                    "1.0".to_string(),
                );
                let _task_id = executor
                    .spawn_deterministic("benchmark_task".to_string(), async move {
                        black_box(identity);
                    })
                    .unwrap();
            })
        });
    });
}

/// Benchmark evidence processing and validation
fn bench_evidence_processing(c: &mut Criterion) {
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
}

/// Benchmark alerting engine performance
fn bench_alerting_engine(c: &mut Criterion) {
    let mut alerting = AlertingEngine::new(100);

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
    c.bench_function("alert_rule_processing_5_metrics", |b| {
        b.iter(|| {
            let metrics = vec![
                ("cpu_usage", 85.0f64),
                ("memory_usage", 90.0f64),
                ("disk_usage", 75.0f64),
                ("network_errors", 5.0f64),
                ("response_time", 150.0f64),
            ];

            for (metric_name, value) in &metrics {
                let _alerts = alerting.evaluate_metric(metric_name, *value);
            }
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
}

criterion_group!(
    name = system_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(15));
    targets = bench_system_metrics_collection, bench_telemetry_processing, bench_policy_evaluation,
             bench_deterministic_execution, bench_evidence_processing, bench_alerting_engine
);

criterion_main!(system_benches);
