// Benchmarks for telemetry bundle compression performance
//
// Run with: cargo bench --bench compression_benchmark

use adapteros_telemetry::bundle::{BundleWriter, CompressionConfig};
use adapteros_telemetry::compression::{
    CompressionAlgorithm, CompressionLevel, TelemetryCompressor,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Generate realistic telemetry event data with varying compressibility
fn generate_telemetry_data(size: usize) -> Vec<u8> {
    // Realistic telemetry: repeated JSON event patterns with some variation
    let event_template = r#"{"timestamp":"2025-11-21T10:30:45.123Z","adapter_id":"adapter-001","event_type":"inference_complete","duration_ms":42.5,"token_count":256,"model":"llama-7b","backend":"coreml"}"#;

    let mut data = Vec::with_capacity(size);
    let mut counter = 0;

    while data.len() < size {
        // Vary fields slightly to make data realistic but still compressible
        let variant = event_template
            .replace("adapter-001", &format!("adapter-{:03}", counter % 50))
            .replace("42.5", &format!("{:.1}", 30.0 + (counter as f64 % 50.0)))
            .replace("256", &format!("{}", 100 + (counter % 1000)));

        data.extend_from_slice(variant.as_bytes());
        data.push(b'\n');
        counter += 1;
    }

    data.truncate(size);
    data
}

/// Generate random/incompressible data for worst-case scenarios
fn generate_random_data(size: usize) -> Vec<u8> {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut data = Vec::with_capacity(size);
    let hasher = RandomState::new();

    for i in 0..size {
        let mut h = hasher.build_hasher();
        h.write_usize(i);
        data.push((h.finish() & 0xFF) as u8);
    }

    data
}

/// Benchmark compressor creation
fn benchmark_compressor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compressor_creation");

    group.bench_function("zstd_creation", |b| {
        b.iter(|| {
            TelemetryCompressor::with_config(
                black_box(CompressionAlgorithm::Zstd),
                black_box(CompressionLevel::DEFAULT),
            )
        })
    });

    group.bench_function("gzip_creation", |b| {
        b.iter(|| {
            TelemetryCompressor::with_config(
                black_box(CompressionAlgorithm::Gzip),
                black_box(CompressionLevel::DEFAULT),
            )
        })
    });

    group.bench_function("lz4_creation", |b| {
        b.iter(|| {
            TelemetryCompressor::with_config(
                black_box(CompressionAlgorithm::Lz4),
                black_box(CompressionLevel::DEFAULT),
            )
        })
    });

    group.finish();
}

/// Benchmark compression of telemetry data at various sizes
fn benchmark_compression_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_sizes");

    for size_kb in [1, 10, 100, 1000].iter() {
        let size = size_kb * 1024;
        let data = black_box(generate_telemetry_data(size));

        group.bench_with_input(
            BenchmarkId::new("zstd_compress", format!("{}KB", size_kb)),
            &size,
            |b, _| {
                let compressor = TelemetryCompressor::with_config(
                    CompressionAlgorithm::Zstd,
                    CompressionLevel::DEFAULT,
                );
                b.iter(|| compressor.compress(&data))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("gzip_compress", format!("{}KB", size_kb)),
            &size,
            |b, _| {
                let compressor = TelemetryCompressor::with_config(
                    CompressionAlgorithm::Gzip,
                    CompressionLevel::DEFAULT,
                );
                b.iter(|| compressor.compress(&data))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("lz4_compress", format!("{}KB", size_kb)),
            &size,
            |b, _| {
                let compressor = TelemetryCompressor::with_config(
                    CompressionAlgorithm::Lz4,
                    CompressionLevel::DEFAULT,
                );
                b.iter(|| compressor.compress(&data))
            },
        );
    }

    group.finish();
}

/// Benchmark decompression
fn benchmark_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression");

    let data = generate_telemetry_data(100 * 1024); // 100KB

    // Zstd
    let zstd_compressor =
        TelemetryCompressor::with_config(CompressionAlgorithm::Zstd, CompressionLevel::DEFAULT);
    let zstd_compressed = zstd_compressor.compress(&data).unwrap();

    group.bench_function("zstd_decompress", |b| {
        b.iter(|| zstd_compressor.decompress(black_box(&zstd_compressed)))
    });

    // Gzip
    let gzip_compressor =
        TelemetryCompressor::with_config(CompressionAlgorithm::Gzip, CompressionLevel::DEFAULT);
    let gzip_compressed = gzip_compressor.compress(&data).unwrap();

    group.bench_function("gzip_decompress", |b| {
        b.iter(|| gzip_compressor.decompress(black_box(&gzip_compressed)))
    });

    // LZ4
    let lz4_compressor =
        TelemetryCompressor::with_config(CompressionAlgorithm::Lz4, CompressionLevel::DEFAULT);
    let lz4_compressed = lz4_compressor.compress(&data).unwrap();

    group.bench_function("lz4_decompress", |b| {
        b.iter(|| lz4_compressor.decompress(black_box(&lz4_compressed)))
    });

    group.finish();
}

/// Benchmark compression level trade-offs
fn benchmark_compression_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_levels");
    group.sample_size(10); // Reduce sample size for slower benchmarks

    let data = generate_telemetry_data(1000 * 1024); // 1MB

    for level in [1, 3, 10, 22].iter() {
        let compressor = TelemetryCompressor::with_config(
            CompressionAlgorithm::Zstd,
            CompressionLevel::new(*level),
        );

        group.bench_with_input(BenchmarkId::new("zstd_level", level), level, |b, _| {
            b.iter(|| compressor.compress(black_box(&data)))
        });
    }

    group.finish();
}

/// Benchmark compression ratio for different data types
fn benchmark_compression_ratio(c: &mut Criterion) {
    let group = c.benchmark_group("compression_ratio");

    let _algorithms = [
        CompressionAlgorithm::Zstd,
        CompressionAlgorithm::Gzip,
        CompressionAlgorithm::Lz4,
    ];

    let compressor =
        TelemetryCompressor::with_config(CompressionAlgorithm::Zstd, CompressionLevel::DEFAULT);

    // Realistic telemetry data
    let telemetry_data = black_box(generate_telemetry_data(100 * 1024));
    let telemetry_compressed = compressor.compress(&telemetry_data).unwrap();
    let telemetry_ratio = telemetry_compressed.len() as f64 / telemetry_data.len() as f64;

    println!(
        "Telemetry compression ratio (Zstd): {:.2}%",
        telemetry_ratio * 100.0
    );

    // Random/incompressible data
    let random_data = black_box(generate_random_data(100 * 1024));
    let random_compressed = compressor.compress(&random_data).unwrap();
    let random_ratio = random_compressed.len() as f64 / random_data.len() as f64;

    println!(
        "Random data compression ratio (Zstd): {:.2}%",
        random_ratio * 100.0
    );

    group.finish();
}

/// Benchmark bundle writer with compression
fn benchmark_bundle_writer(c: &mut Criterion) {
    let mut group = c.benchmark_group("bundle_writer");
    group.sample_size(10);


    group.bench_function("bundle_write_with_compression", |b| {
        b.iter(|| {
            let temp_dir = tempfile::Builder::new()
                .prefix("aos-test-")
                .tempdir()
                .expect("tempdir");
            let mut writer = BundleWriter::with_compression(
                temp_dir.path(),
                10000,
                10 * 1024 * 1024, // 10MB
                CompressionConfig::default(),
            )
            .unwrap();

            // Write 1000 events
            for i in 0..1000 {
                let event = serde_json::json!({
                    "id": i,
                    "type": "test_event",
                    "timestamp": "2025-11-21T10:30:45Z",
                    "data": "test event data"
                });
                writer.write_event(&event).unwrap();
            }
            writer.flush().unwrap();
        })
    });

    group.bench_function("bundle_write_without_compression", |b| {
        b.iter(|| {
            let temp_dir = tempfile::Builder::new()
                .prefix("aos-test-")
                .tempdir()
                .expect("tempdir");
            let config = CompressionConfig {
                enabled: false,
                ..Default::default()
            };
            let mut writer =
                BundleWriter::with_compression(temp_dir.path(), 10000, 10 * 1024 * 1024, config)
                    .unwrap();

            // Write 1000 events
            for i in 0..1000 {
                let event = serde_json::json!({
                    "id": i,
                    "type": "test_event",
                    "timestamp": "2025-11-21T10:30:45Z",
                    "data": "test event data"
                });
                writer.write_event(&event).unwrap();
            }
            writer.flush().unwrap();
        })
    });

    group.finish();
}

/// Benchmark memory usage during compression
fn benchmark_compression_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");

    let data = generate_telemetry_data(10 * 1024 * 1024); // 10MB

    group.bench_function("zstd_compress_memory", |b| {
        let compressor =
            TelemetryCompressor::with_config(CompressionAlgorithm::Zstd, CompressionLevel::DEFAULT);
        b.iter(|| compressor.compress(black_box(&data)))
    });

    group.bench_function("lz4_compress_memory", |b| {
        let compressor =
            TelemetryCompressor::with_config(CompressionAlgorithm::Lz4, CompressionLevel::DEFAULT);
        b.iter(|| compressor.compress(black_box(&data)))
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_compressor_creation,
    benchmark_compression_sizes,
    benchmark_decompression,
    benchmark_compression_levels,
    benchmark_compression_ratio,
    benchmark_bundle_writer,
    benchmark_compression_memory_efficiency,
);

criterion_main!(benches);
