//! Telemetry performance benchmarks
//!
//! Validates PRD-08 requirement: <1ms telemetry overhead

use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::{
    EventSampler, EventType, LogLevel, TelemetryCompressor, TelemetryEventBuilder,
    TelemetryRingBuffer,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::Runtime;

fn create_test_event(id: u64) -> adapteros_telemetry::TelemetryEvent {
    let identity = IdentityEnvelope::new(
        "bench".to_string(),
        "telemetry".to_string(),
        "benchmark".to_string(),
        "1.0".to_string(),
    );

    TelemetryEventBuilder::new(
        EventType::PerformanceMetric,
        LogLevel::Info,
        format!("Benchmark event {}", id),
        identity,
    )
    .build()
    .expect("Failed to build telemetry event")
}

fn bench_ring_buffer_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer_push");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("capacity", size), size, |b, &size| {
            let rt = Runtime::new().unwrap();
            let buffer = TelemetryRingBuffer::new(size);

            b.iter(|| {
                rt.block_on(async {
                    let event = create_test_event(0);
                    buffer.push(event).await
                })
            });
        });
    }

    group.finish();
}

fn bench_event_sampling(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_sampling");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let sampler = EventSampler::new();

    group.bench_function("should_sample", |b| {
        let event = create_test_event(0);

        b.iter(|| rt.block_on(async { sampler.should_sample(black_box(&event)).await }));
    });

    group.finish();
}

fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");

    // Generate test data of different sizes
    let sizes = vec![1024, 10 * 1024, 100 * 1024]; // 1KB, 10KB, 100KB

    for size in sizes {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("zstd_compress", size), &data, |b, data| {
            let compressor = TelemetryCompressor::new();
            b.iter(|| compressor.compress(black_box(data)).unwrap());
        });

        let compressor = TelemetryCompressor::new();
        let compressed = compressor.compress(&data).unwrap();

        group.bench_with_input(
            BenchmarkId::new("zstd_decompress", size),
            &compressed,
            |b, compressed| {
                b.iter(|| compressor.decompress(black_box(compressed)).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_end_to_end_telemetry_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end_overhead");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let buffer = TelemetryRingBuffer::new(10000);
    let sampler = EventSampler::new();

    group.bench_function("full_telemetry_pipeline", |b| {
        b.iter(|| {
            rt.block_on(async {
                let event = create_test_event(0);

                // 1. Sampling decision
                if sampler.should_sample(&event).await {
                    // 2. Push to ring buffer
                    let _ = buffer.push(event).await;
                }
            })
        });
    });

    group.finish();
}

fn bench_ring_buffer_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer_read");

    for count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*count));
        group.bench_with_input(BenchmarkId::new("read_all", count), count, |b, &count| {
            let rt = Runtime::new().unwrap();
            let buffer = TelemetryRingBuffer::new(10000);

            // Populate buffer
            rt.block_on(async {
                for i in 0..count {
                    buffer.push(create_test_event(i)).await.unwrap();
                }
            });

            b.iter(|| rt.block_on(async { buffer.read_all().await }));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_ring_buffer_push,
    bench_ring_buffer_read,
    bench_event_sampling,
    bench_compression,
    bench_end_to_end_telemetry_overhead
);
criterion_main!(benches);
