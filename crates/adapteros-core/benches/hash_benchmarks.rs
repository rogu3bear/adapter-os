//! Performance benchmarks for BLAKE3 hashing and HKDF seed derivation
//!
//! This module benchmarks cryptographic operations critical to AdapterOS:
//! - BLAKE3 hashing across various file sizes (1KB to 256MB)
//! - HKDF seed derivation performance (derive_seed and derive_seed_full)
//! - Hash verification operations
//! - B3Hash serialization/deserialization
//! - Zero-copy operation efficiency
//! - Comparison with SHA256 baseline
//!
//! Run with: cargo bench -p adapteros-core --bench hash_benchmarks
//! Run specific benchmark: cargo bench -p adapteros-core --bench hash_benchmarks -- blake3_sizes
//! Run with custom sample size: cargo bench -p adapteros-core --bench hash_benchmarks -- --sample-size 100
//!
//! Use --verbose flag to see more detailed output:
//! cargo bench -p adapteros-core --bench hash_benchmarks -- --verbose
//!
//! Note on parallelism: BLAKE3 internally uses SIMD parallelism for large inputs.
//! External rayon parallelism is not currently available in this crate, but BLAKE3's
//! tree hashing mode provides excellent throughput on multi-core systems.

use adapteros_core::{derive_seed, derive_seed_full, derive_seed_indexed, B3Hash};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// ============================================================================
// BLAKE3 HASHING BENCHMARKS
// ============================================================================

/// Benchmark BLAKE3 hashing for requested file sizes
///
/// Tests the specific sizes requested for AdapterOS:
/// - 1KB: Small configuration files
/// - 64KB: Typical adapter metadata
/// - 1MB: Medium-sized models
/// - 64MB: Large adapter weights
/// - 256MB: Full model files
///
/// This measures:
/// - Throughput (GB/s) at different data scales
/// - Constant overhead cost
/// - Linearity of scaling
fn bench_blake3_requested_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_requested_sizes");

    // Configure for diverse file sizes
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // Requested sizes: 1KB, 64KB, 1MB, 64MB, 256MB
    let file_sizes = vec![
        ("1KB", 1_024),
        ("64KB", 64 * 1_024),
        ("1MB", 1_024 * 1_024),
        ("64MB", 64 * 1_024 * 1_024),
        ("256MB", 256 * 1_024 * 1_024),
    ];

    for (label, size) in file_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("file_size", label), &size, |b, &size| {
            // Pre-allocate to isolate hashing performance
            let data = black_box(vec![0xAAu8; size]);
            b.iter(|| {
                black_box(B3Hash::hash(&data));
            });
        });
    }

    group.finish();
}

/// Benchmark BLAKE3 hashing for various file sizes (extended range)
///
/// Tests a broader range for performance characterization:
/// - 1KB to 100MB range
/// - Identifies cache boundary effects
fn bench_blake3_various_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_sizes");

    group.sample_size(20);
    group.measurement_time(Duration::from_secs(15));

    let file_sizes = vec![
        ("1KB", 1_024),
        ("10KB", 10 * 1_024),
        ("100KB", 100 * 1_024),
        ("1MB", 1_024 * 1_024),
        ("10MB", 10 * 1_024 * 1_024),
        ("100MB", 100 * 1_024 * 1_024),
    ];

    for (label, size) in file_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("file_size", label), &size, |b, &size| {
            let data = black_box(vec![0xAAu8; size]);
            b.iter(|| {
                B3Hash::hash(&data);
            });
        });
    }

    group.finish();
}

/// Benchmark BLAKE3 hashing of pre-allocated buffers
///
/// Isolates the hashing cost from allocation overhead.
/// This tests:
/// - Pure hash computation performance
/// - Memory bandwidth utilization
/// - CPU cache efficiency
fn bench_blake3_hash_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_hash_only");

    group.sample_size(30);
    group.measurement_time(Duration::from_secs(15));

    let sizes = vec![
        ("64B", 64),
        ("256B", 256),
        ("1KB", 1_024),
        ("4KB", 4_096),
        ("16KB", 16_384),
        ("64KB", 65_536),
        ("256KB", 262_144),
        ("1MB", 1_024 * 1_024),
    ];

    for (label, size) in sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("buffer_size", label), &size, |b, &size| {
            // Pre-allocate buffer to isolate hashing cost
            let data = vec![0xDEu8; size];
            b.iter(|| {
                black_box(B3Hash::hash(black_box(&data)));
            });
        });
    }

    group.finish();
}

// ============================================================================
// HASH VERIFICATION BENCHMARKS
// ============================================================================

/// Benchmark hash verification performance
///
/// Tests the complete verification workflow:
/// - Hash computation + comparison
/// - Pre-computed hash comparison
/// - Batch verification scenarios
///
/// This is critical for adapter integrity checking.
fn bench_hash_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_verification");

    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    let sizes = vec![("1KB", 1_024), ("64KB", 64 * 1_024), ("1MB", 1_024 * 1_024)];

    for (label, size) in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        // Full verification (compute + compare)
        group.bench_with_input(BenchmarkId::new("full_verify", label), &size, |b, &size| {
            let data = vec![0xAAu8; size];
            let expected_hash = B3Hash::hash(&data);
            b.iter(|| {
                let computed = B3Hash::hash(black_box(&data));
                black_box(computed == expected_hash)
            });
        });

        // Hash comparison only (pre-computed)
        group.bench_with_input(
            BenchmarkId::new("compare_only", label),
            &size,
            |b, &size| {
                let data = vec![0xAAu8; size];
                let hash1 = B3Hash::hash(&data);
                let hash2 = B3Hash::hash(&data);
                b.iter(|| black_box(hash1 == hash2));
            },
        );
    }

    // Batch verification (multiple hashes)
    group.bench_function("batch_verify_10", |b| {
        let hashes: Vec<_> = (0..10).map(|i| B3Hash::hash(&[i as u8; 1024])).collect();
        let expected: Vec<_> = hashes.clone();
        b.iter(|| {
            let mut all_match = true;
            for (h, e) in hashes.iter().zip(expected.iter()) {
                all_match &= h == e;
            }
            black_box(all_match)
        });
    });

    // Hash ordering comparison (for sorted collections)
    group.bench_function("ordering_compare", |b| {
        let hash1 = B3Hash::hash(b"hash_a");
        let hash2 = B3Hash::hash(b"hash_b");
        b.iter(|| black_box(hash1.cmp(&hash2)));
    });

    group.finish();
}

// ============================================================================
// HKDF SEED DERIVATION BENCHMARKS
// ============================================================================

/// Benchmark HKDF seed derivation performance
///
/// HKDF is used to derive deterministic seeds from manifest hashes.
/// This measures:
/// - Basic seed derivation (derive_seed with single label)
/// - Full entropy seed derivation (derive_seed_full with manifest + adapter_dir + worker_id)
/// - Indexed seed derivation (derive_seed_indexed for array-like derivations)
///
/// Expected overhead: <100 microseconds per derivation
fn bench_hkdf_seed_derivation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hkdf_derivation");

    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    let global_hash = B3Hash::hash(b"global_manifest_test");
    let manifest_hash = B3Hash::hash(b"manifest_v1");
    let adapter_dir_hash = B3Hash::hash(b"/adapters/test");

    // Basic seed derivation (derive_seed)
    group.bench_function("derive_seed_basic", |b| {
        b.iter(|| {
            black_box(derive_seed(&global_hash, "router"));
        });
    });

    // Full entropy seed derivation (derive_seed_full)
    group.bench_function("derive_seed_full", |b| {
        b.iter(|| {
            black_box(derive_seed_full(
                &global_hash,
                &manifest_hash,
                &adapter_dir_hash,
                black_box(1), // worker_id
                "dropout",
                black_box(0), // nonce
            ));
        });
    });

    // Indexed seed derivation (derive_seed_indexed)
    group.bench_function("derive_seed_indexed", |b| {
        b.iter(|| {
            black_box(derive_seed_indexed(&global_hash, "sampler", black_box(0)));
        });
    });

    // Compare derive_seed vs derive_seed_full overhead
    group.bench_function("derive_seed_full_overhead", |b| {
        b.iter(|| {
            // Measure the additional cost of full context
            let basic = derive_seed(&global_hash, "router");
            let full = derive_seed_full(
                &global_hash,
                &manifest_hash,
                &adapter_dir_hash,
                1,
                "router",
                0,
            );
            black_box((basic, full))
        });
    });

    group.finish();
}

/// Benchmark batch seed derivation
///
/// Measures the cost of deriving multiple seeds at once.
/// Used for initializing RNG for multiple components.
fn bench_hkdf_batch_derivation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hkdf_batch");

    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    let batch_sizes = vec![1, 4, 8, 16, 32, 64];
    let global_hash = B3Hash::hash(b"test_manifest");

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("seeds", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    // Derive multiple seeds sequentially
                    for i in 0..batch_size {
                        let label = format!("seed_{}", i);
                        black_box(derive_seed(&global_hash, &label));
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark HKDF entropy isolation
///
/// Verifies that different label contexts produce different seeds efficiently.
/// Tests:
/// - Manifest hash variation
/// - Worker ID variation
/// - Nonce variation
fn bench_hkdf_entropy_isolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hkdf_isolation");

    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    let global_hash = B3Hash::hash(b"global");
    let manifest1 = B3Hash::hash(b"manifest1");
    let manifest2 = B3Hash::hash(b"manifest2");
    let adapter_dir = B3Hash::hash(b"/adapters/test");

    // Different manifests should produce different seeds
    group.bench_function("manifest_variation", |b| {
        b.iter(|| {
            let seed1 = derive_seed_full(&global_hash, &manifest1, &adapter_dir, 1, "router", 0);
            let seed2 = derive_seed_full(&global_hash, &manifest2, &adapter_dir, 1, "router", 0);
            black_box((seed1, seed2))
        });
    });

    // Different worker IDs should produce different seeds
    group.bench_function("worker_variation", |b| {
        b.iter(|| {
            let seed1 = derive_seed_full(&global_hash, &manifest1, &adapter_dir, 1, "dropout", 0);
            let seed2 = derive_seed_full(&global_hash, &manifest1, &adapter_dir, 2, "dropout", 0);
            black_box((seed1, seed2))
        });
    });

    // Different nonces should produce different seeds
    group.bench_function("nonce_variation", |b| {
        b.iter(|| {
            let seed1 = derive_seed_full(&global_hash, &manifest1, &adapter_dir, 1, "sampling", 0);
            let seed2 = derive_seed_full(&global_hash, &manifest1, &adapter_dir, 1, "sampling", 1);
            black_box((seed1, seed2))
        });
    });

    group.finish();
}

// ============================================================================
// SERIALIZATION BENCHMARKS
// ============================================================================

/// Benchmark B3Hash serialization/deserialization
///
/// Tests all serialization paths for B3Hash:
/// - JSON serialization (serde_json)
/// - Hex encoding/decoding
/// - Binary roundtrip
///
/// Important for database and API operations.
fn bench_hash_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_serialization");

    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let hash = B3Hash::hash(b"test data for serialization benchmarks");

    // JSON serialization
    group.bench_function("serde_json_serialize", |b| {
        b.iter(|| black_box(serde_json::to_string(&hash).expect("serialize")));
    });

    // JSON deserialization
    group.bench_function("serde_json_deserialize", |b| {
        let json = serde_json::to_string(&hash).expect("serialize");
        b.iter(|| black_box(serde_json::from_str::<B3Hash>(&json).expect("deserialize")));
    });

    // JSON roundtrip
    group.bench_function("serde_json_roundtrip", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&hash).expect("serialize");
            black_box(serde_json::from_str::<B3Hash>(&json).expect("deserialize"))
        });
    });

    // Binary serialization (raw bytes)
    group.bench_function("to_bytes", |b| {
        b.iter(|| black_box(hash.to_bytes()));
    });

    // Binary deserialization
    group.bench_function("from_bytes", |b| {
        let bytes = hash.to_bytes();
        b.iter(|| black_box(B3Hash::from_bytes(bytes)));
    });

    // Binary roundtrip
    group.bench_function("bytes_roundtrip", |b| {
        b.iter(|| {
            let bytes = hash.to_bytes();
            black_box(B3Hash::from_bytes(bytes))
        });
    });

    group.finish();
}

/// Benchmark hex encoding/decoding roundtrip
///
/// Tests the performance of converting between bytes and hex strings.
/// Used for hash display and string-based storage.
fn bench_hex_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex_roundtrip");

    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let hash = B3Hash::hash(b"test data for hex encoding");

    group.bench_function("to_hex", |b| {
        b.iter(|| black_box(hash.to_hex()));
    });

    group.bench_function("to_short_hex", |b| {
        b.iter(|| black_box(hash.to_short_hex()));
    });

    group.bench_function("from_hex", |b| {
        let hex = hash.to_hex();
        b.iter(|| black_box(B3Hash::from_hex(&hex).expect("parse hex")));
    });

    group.bench_function("hex_roundtrip", |b| {
        b.iter(|| {
            let hex = hash.to_hex();
            black_box(B3Hash::from_hex(&hex).expect("parse hex"))
        });
    });

    // Display formatting
    group.bench_function("display_format", |b| {
        b.iter(|| black_box(format!("{}", hash)));
    });

    group.bench_function("debug_format", |b| {
        b.iter(|| black_box(format!("{:?}", hash)));
    });

    group.finish();
}

// ============================================================================
// MULTI-HASH AND ZERO-COPY BENCHMARKS
// ============================================================================

/// Benchmark multi-hash operations (hash_multi)
///
/// Tests hashing multiple byte slices in one operation.
/// This is more efficient than concatenating then hashing.
fn bench_blake3_multi_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_multi");

    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    let slice_configs = vec![
        ("2x512B", 2, 512),
        ("4x256B", 4, 256),
        ("8x128B", 8, 128),
        ("16x64B", 16, 64),
        ("32x32B", 32, 32),
    ];

    for (label, num_slices, slice_size) in slice_configs {
        let total_bytes = num_slices * slice_size;
        group.throughput(Throughput::Bytes(total_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("config", label),
            &(num_slices, slice_size),
            |b, &(num_slices, slice_size)| {
                let slices: Vec<_> = (0..num_slices).map(|_| vec![0xBBu8; slice_size]).collect();
                let slice_refs: Vec<_> = slices.iter().map(|s| s.as_slice()).collect();

                b.iter(|| {
                    black_box(B3Hash::hash_multi(&slice_refs));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark zero-copy pattern efficiency
///
/// Measures the performance impact of avoiding data copies.
/// Tests:
/// - Direct hashing (no copy)
/// - Hash_multi (multiple slices without concatenation)
/// - Comparison with concatenate-then-hash
fn bench_zero_copy_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_copy");

    group.sample_size(30);
    group.measurement_time(Duration::from_secs(15));

    let total_bytes = 1_024 * 1_024; // 1MB total
    let num_slices = 4;
    let slice_size = total_bytes / num_slices;

    group.throughput(Throughput::Bytes(total_bytes as u64));

    // Concatenate then hash (baseline)
    group.bench_function("concat_then_hash", |b| {
        let data = vec![0xCCu8; total_bytes];
        b.iter(|| black_box(B3Hash::hash(&data)));
    });

    // Hash multi (zero-copy)
    group.bench_function("hash_multi_zero_copy", |b| {
        let slices: Vec<_> = (0..num_slices).map(|_| vec![0xCCu8; slice_size]).collect();
        let slice_refs: Vec<_> = slices.iter().map(|s| s.as_slice()).collect();
        b.iter(|| black_box(B3Hash::hash_multi(&slice_refs)));
    });

    group.finish();
}

// ============================================================================
// COMPARISON BENCHMARKS
// ============================================================================

/// Benchmark BLAKE3 vs SHA256 comparison
///
/// Provides performance baseline comparison.
/// BLAKE3 should be significantly faster due to:
/// - Parallel tree hashing
/// - Wider internal state
/// - Modern instruction selection (SIMD)
fn bench_blake3_vs_sha256(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_vs_sha256");

    group.sample_size(30);
    group.measurement_time(Duration::from_secs(15));

    let sizes = vec![("1KB", 1_024), ("64KB", 64 * 1_024), ("1MB", 1_024 * 1_024)];

    for (label, size) in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        // BLAKE3 benchmark
        group.bench_with_input(BenchmarkId::new("blake3", label), &size, |b, &size| {
            let data = vec![0xAAu8; size];
            b.iter(|| black_box(B3Hash::hash(&data)));
        });

        // SHA256 benchmark (using sha2 crate for comparison)
        group.bench_with_input(BenchmarkId::new("sha256", label), &size, |b, &size| {
            use sha2::{Digest, Sha256};
            let data = vec![0xAAu8; size];
            b.iter(|| {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                black_box(hasher.finalize())
            });
        });
    }

    group.finish();
}

// ============================================================================
// FILE I/O BENCHMARKS
// ============================================================================

/// Benchmark hash_file operation
///
/// Measures the cost of reading and hashing a file from disk.
/// Important for adapter registration and manifest verification.
fn bench_hash_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_file");

    group.sample_size(20);
    group.measurement_time(Duration::from_secs(15));

    // Create temporary files for benchmarking
    let temp_sizes = vec![("1KB", 1_024), ("64KB", 64 * 1_024), ("1MB", 1_024 * 1_024)];

    for (label, size) in temp_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("file_size", label), &size, |b, &size| {
            use std::io::Write;

            let root = std::path::PathBuf::from("var").join("tmp");
            std::fs::create_dir_all(&root).expect("create var/tmp");
            let file = tempfile::NamedTempFile::new_in(&root).expect("create temp file");
            let path = file.path().to_path_buf();

            // Write test data
            {
                let mut f = std::fs::File::create(&path).expect("create test file");
                let data = vec![0xAAu8; size];
                f.write_all(&data).expect("write test file");
            }

            b.iter(|| black_box(B3Hash::hash_file(&path).expect("hash file")));

            // Cleanup handled by NamedTempFile drop
        });
    }

    group.finish();
}

// ============================================================================
// PARALLEL HASHING BENCHMARKS (BLAKE3 Internal)
// ============================================================================

/// Benchmark BLAKE3 internal parallelism
///
/// BLAKE3 uses internal SIMD parallelism. This benchmark demonstrates
/// the throughput scaling with larger inputs where BLAKE3's tree hashing
/// becomes more effective.
///
/// Note: External rayon parallelism is not available in this crate.
/// For parallel file hashing across multiple files, consider using
/// tokio::spawn or rayon at the application layer.
fn bench_blake3_parallel_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_parallel_scaling");

    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // Test sizes where BLAKE3's internal parallelism kicks in
    // BLAKE3 chunk size is 1KB, tree hashing becomes effective at larger sizes
    let sizes = vec![
        ("below_chunk_512B", 512),
        ("single_chunk_1KB", 1_024),
        ("multi_chunk_4KB", 4 * 1_024),
        ("tree_level_64KB", 64 * 1_024),
        ("tree_level_1MB", 1_024 * 1_024),
        ("tree_level_16MB", 16 * 1_024 * 1_024),
        ("tree_level_64MB", 64 * 1_024 * 1_024),
    ];

    for (label, size) in sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("internal_parallel", label),
            &size,
            |b, &size| {
                let data = vec![0xFFu8; size];
                b.iter(|| black_box(B3Hash::hash(&data)));
            },
        );
    }

    group.finish();
}

// ============================================================================
// CRITERION CONFIGURATION
// ============================================================================

// Register all benchmark groups
criterion_group!(
    benches,
    // Primary requested benchmarks
    bench_blake3_requested_sizes,
    bench_hkdf_seed_derivation,
    bench_hash_verification,
    bench_hash_serialization,
    // Extended benchmarks
    bench_blake3_various_sizes,
    bench_blake3_hash_only,
    bench_hkdf_batch_derivation,
    bench_blake3_multi_hash,
    bench_blake3_vs_sha256,
    bench_zero_copy_efficiency,
    bench_hkdf_entropy_isolation,
    bench_hash_file,
    bench_hex_roundtrip,
    bench_blake3_parallel_scaling,
);

criterion_main!(benches);
