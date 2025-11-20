//! Comprehensive performance benchmarks for AOS 2.0 format
//!
//! Measures actual performance of:
//! - Header parsing
//! - Manifest loading
//! - Memory-mapped file access
//! - Safetensors parsing
//! - Memory allocation patterns
//!
//! Run with: cargo bench -p adapteros-aos

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, PlotConfiguration,
    Throughput,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use tempfile::NamedTempFile;

// ============================================================================
// Test Data Structures
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
struct BenchmarkManifest {
    version: String,
    adapter_id: String,
    weights_offset: u64,
    tensor_shapes: HashMap<String, Vec<usize>>,
    rank: u32,
    alpha: f32,
    target_modules: Vec<String>,
}

impl BenchmarkManifest {
    fn new(num_tensors: usize) -> Self {
        let mut tensor_shapes = HashMap::new();
        for i in 0..num_tensors {
            tensor_shapes.insert(
                format!("layer.{}.weight", i),
                vec![768, 768], // Typical transformer layer size
            );
        }

        Self {
            version: "2.0".to_string(),
            adapter_id: "benchmark-adapter".to_string(),
            weights_offset: 8,
            tensor_shapes,
            rank: 8,
            alpha: 16.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
        }
    }
}

// ============================================================================
// Archive Creation Helpers
// ============================================================================

fn create_test_archive(
    manifest: &BenchmarkManifest,
    weights_size_bytes: usize,
) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let temp_file = NamedTempFile::new()?;

    // Create fake weights data
    let weights_data = vec![0u8; weights_size_bytes];

    // Serialize manifest
    let manifest_json = serde_json::to_vec(manifest)?;

    // Calculate offsets
    let header_size = 8;
    let manifest_offset = header_size + weights_data.len();
    let manifest_len = manifest_json.len();

    // Write archive
    let mut file = File::create(temp_file.path())?;

    // Write header
    file.write_all(&(manifest_offset as u32).to_le_bytes())?;
    file.write_all(&(manifest_len as u32).to_le_bytes())?;

    // Write weights
    file.write_all(&weights_data)?;

    // Write manifest
    file.write_all(&manifest_json)?;

    file.flush()?;

    Ok(temp_file)
}

// ============================================================================
// Benchmark 1: Header Parsing
// ============================================================================

fn benchmark_header_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("header_parsing");

    // Create test archive
    let manifest = BenchmarkManifest::new(10);
    let archive = create_test_archive(&manifest, 1024 * 1024).unwrap(); // 1MB weights

    group.bench_function("parse_header", |b| {
        b.iter(|| {
            let mut file = File::open(archive.path()).unwrap();
            let mut header = [0u8; 8];
            file.read_exact(&mut header).unwrap();

            let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

            black_box((manifest_offset, manifest_len))
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 2: Manifest Loading
// ============================================================================

fn benchmark_manifest_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("manifest_loading");

    let sizes = vec![10, 50, 100, 500];

    for num_tensors in sizes {
        let manifest = BenchmarkManifest::new(num_tensors);
        let archive = create_test_archive(&manifest, 1024 * 1024).unwrap();

        group.throughput(Throughput::Elements(num_tensors as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_tensors", num_tensors)),
            &archive.path(),
            |b, path| {
                b.iter(|| {
                    let mut file = File::open(path).unwrap();
                    let mut header = [0u8; 8];
                    file.read_exact(&mut header).unwrap();

                    let manifest_offset =
                        u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                    let manifest_len =
                        u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

                    // Read manifest
                    let mut manifest_data = vec![0u8; manifest_len as usize];
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
                        .unwrap();
                    file.read_exact(&mut manifest_data).unwrap();

                    let manifest: BenchmarkManifest =
                        serde_json::from_slice(&manifest_data).unwrap();
                    black_box(manifest)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 3: Memory-Mapped vs Regular File Reading
// ============================================================================

fn benchmark_mmap_vs_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("mmap_vs_read");
    group.plot_config(PlotConfiguration::default());

    let sizes_mb = vec![1, 10, 50, 100]; // File sizes in MB

    for size_mb in sizes_mb {
        let size_bytes = size_mb * 1024 * 1024;
        let manifest = BenchmarkManifest::new(10);
        let archive = create_test_archive(&manifest, size_bytes).unwrap();

        group.throughput(Throughput::Bytes(size_bytes as u64));

        // Regular file read
        group.bench_with_input(
            BenchmarkId::new("regular_read", format!("{}MB", size_mb)),
            &archive.path(),
            |b, path| {
                b.iter(|| {
                    let mut file = File::open(path).unwrap();
                    let mut data = Vec::new();
                    file.read_to_end(&mut data).unwrap();
                    black_box(data)
                });
            },
        );

        // Memory-mapped read
        group.bench_with_input(
            BenchmarkId::new("mmap_read", format!("{}MB", size_mb)),
            &archive.path(),
            |b, path| {
                b.iter(|| {
                    let file = File::open(path).unwrap();
                    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
                    // Access first byte to ensure mapping is valid
                    let first_byte = mmap[0];
                    black_box(first_byte)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 4: Full Archive Loading
// ============================================================================

fn benchmark_full_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_archive_load");
    group.sample_size(20); // Smaller sample size for heavy operations

    let configs = vec![
        ("small", 10, 1),     // 10 tensors, 1MB
        ("medium", 50, 10),   // 50 tensors, 10MB
        ("large", 100, 50),   // 100 tensors, 50MB
        ("xlarge", 500, 100), // 500 tensors, 100MB
    ];

    for (name, num_tensors, size_mb) in configs {
        let manifest = BenchmarkManifest::new(num_tensors);
        let archive = create_test_archive(&manifest, size_mb * 1024 * 1024).unwrap();

        group.throughput(Throughput::Bytes((size_mb * 1024 * 1024) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &archive.path(),
            |b, path| {
                b.iter(|| {
                    // Full load simulation
                    let file = File::open(path).unwrap();
                    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };

                    // Parse header
                    let manifest_offset =
                        u32::from_le_bytes([mmap[0], mmap[1], mmap[2], mmap[3]]) as usize;
                    let manifest_len =
                        u32::from_le_bytes([mmap[4], mmap[5], mmap[6], mmap[7]]) as usize;

                    // Parse manifest
                    let manifest_bytes = &mmap[manifest_offset..manifest_offset + manifest_len];
                    let manifest: BenchmarkManifest =
                        serde_json::from_slice(manifest_bytes).unwrap();

                    // Access weights region (simulates reading)
                    let weights_offset = manifest.weights_offset as usize;
                    let _weights = &mmap[weights_offset..manifest_offset];

                    black_box(manifest)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 5: JSON Parsing Performance
// ============================================================================

fn benchmark_json_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    let sizes = vec![10, 100, 500, 1000];

    for num_tensors in sizes {
        let manifest = BenchmarkManifest::new(num_tensors);
        let json_data = serde_json::to_vec(&manifest).unwrap();

        group.throughput(Throughput::Bytes(json_data.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_tensors", num_tensors)),
            &json_data,
            |b, data| {
                b.iter(|| {
                    let manifest: BenchmarkManifest = serde_json::from_slice(data).unwrap();
                    black_box(manifest)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 6: Memory Allocation Patterns
// ============================================================================

fn benchmark_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    let sizes_mb = vec![1, 10, 50, 100];

    for size_mb in sizes_mb {
        let size_bytes = size_mb * 1024 * 1024;

        // Pre-allocated vec
        group.bench_with_input(
            BenchmarkId::new("preallocated", format!("{}MB", size_mb)),
            &size_bytes,
            |b, &size| {
                b.iter(|| {
                    let mut data = Vec::with_capacity(size);
                    data.resize(size, 0u8);
                    black_box(data)
                });
            },
        );

        // Growing vec
        group.bench_with_input(
            BenchmarkId::new("growing", format!("{}MB", size_mb)),
            &size_bytes,
            |b, &size| {
                b.iter(|| {
                    let mut data = Vec::new();
                    for _ in 0..size {
                        data.push(0u8);
                    }
                    black_box(data)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    benches,
    benchmark_header_parsing,
    benchmark_manifest_loading,
    benchmark_mmap_vs_read,
    benchmark_full_load,
    benchmark_json_parsing,
    benchmark_memory_allocation,
);

criterion_main!(benches);
