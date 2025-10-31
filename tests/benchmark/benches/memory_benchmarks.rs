#![cfg(all(test, feature = "extended-tests"))]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use adapteros_benchmarks::*;
use adapteros_memory::{MemoryPool, MemoryTracker, AllocationStrategy};
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::thread;

/// Benchmark memory allocation patterns
fn bench_memory_allocation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark different allocation sizes
        let sizes = [64, 1024, 64 * 1024, 1024 * 1024, 16 * 1024 * 1024]; // 64B to 16MB

        for &size in &sizes {
            c.bench_function(&format!("memory_allocation_{}b", size), |b| {
                b.iter(|| {
                    let data = vec![0u8; size];
                    black_box(data);
                })
            });
        }

        // Benchmark memory pool allocation
        let memory_pool = Arc::new(MemoryPool::new(1024 * 1024 * 100).unwrap()); // 100MB pool

        for &size in &sizes {
            c.bench_function(&format!("memory_pool_allocation_{}b", size), |b| {
                b.iter(|| {
                    let buffer = memory_pool.allocate(size).unwrap();
                    black_box(buffer);
                })
            });
        }
    });
}

/// Benchmark memory access patterns
fn bench_memory_access_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Create test data
        let mut data = vec![0u8; 64 * 1024 * 1024]; // 64MB
        for (i, val) in data.iter_mut().enumerate() {
            *val = (i % 256) as u8;
        }

        // Benchmark sequential access
        c.bench_function("memory_sequential_access_64mb", |b| {
            b.iter(|| {
                let mut sum = 0u64;
                for &val in &data {
                    sum = sum.wrapping_add(val as u64);
                }
                black_box(sum);
            })
        });

        // Benchmark random access
        let indices: Vec<usize> = (0..data.len()).collect();
        let mut rng = utils::DeterministicRng::new(42);

        c.bench_function("memory_random_access_64mb", |b| {
            b.iter(|| {
                let mut sum = 0u64;
                for _ in 0..100000 {
                    let idx = rng.next_u32() as usize % data.len();
                    sum = sum.wrapping_add(data[idx] as u64);
                }
                black_box(sum);
            })
        });

        // Benchmark strided access (cache-unfriendly)
        c.bench_function("memory_strided_access_64mb_stride_64", |b| {
            b.iter(|| {
                let mut sum = 0u64;
                let stride = 64;
                for i in (0..data.len()).step_by(stride) {
                    sum = sum.wrapping_add(data[i] as u64);
                }
                black_box(sum);
            })
        });
    });
}

/// Benchmark memory pressure scenarios
fn bench_memory_pressure(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark memory fragmentation
        c.bench_function("memory_fragmentation_simulation", |b| {
            b.iter(|| {
                let mut allocations = Vec::new();

                // Allocate many small blocks
                for _ in 0..1000 {
                    allocations.push(vec![0u8; 1024]); // 1KB each
                }

                // Free every other block to create fragmentation
                let mut i = 0;
                allocations.retain(|_| {
                    i += 1;
                    i % 2 == 0
                });

                // Allocate larger blocks in fragmented memory
                for _ in 0..500 {
                    allocations.push(vec![0u8; 2048]); // 2KB each
                }

                black_box(allocations.len());
            })
        });

        // Benchmark garbage collection simulation
        c.bench_function("memory_gc_simulation", |b| {
            b.iter(|| {
                let mut live_objects = Vec::new();
                let mut dead_objects = Vec::new();

                // Simulate object allocation and death
                for i in 0..10000 {
                    if i % 3 == 0 {
                        // Keep some objects alive
                        live_objects.push(vec![i as u8; 256]);
                    } else {
                        // Let others die
                        dead_objects.push(vec![i as u8; 256]);
                    }
                }

                // Simulate GC sweep
                dead_objects.clear();

                // Compact live objects
                live_objects.shrink_to_fit();

                black_box((live_objects.len(), dead_objects.len()));
            })
        });
    });
}

/// Benchmark concurrent memory operations
fn bench_concurrent_memory(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark concurrent allocations
        c.bench_function("concurrent_memory_allocation_4_threads", |b| {
            b.iter(|| {
                let handles: Vec<_> = (0..4).map(|_| {
                    thread::spawn(|| {
                        let mut allocations = Vec::new();
                        for _ in 0..100 {
                            allocations.push(vec![0u8; 64 * 1024]); // 64KB each
                        }
                        allocations
                    })
                }).collect();

                let mut total_size = 0;
                for handle in handles {
                    let allocations = handle.join().unwrap();
                    total_size += allocations.len();
                }

                black_box(total_size);
            })
        });

        // Benchmark shared memory pool contention
        let memory_pool = Arc::new(MemoryPool::new(1024 * 1024 * 50).unwrap()); // 50MB pool

        c.bench_function("memory_pool_contention_8_threads", |b| {
            b.iter(|| {
                let pool = Arc::clone(&memory_pool);
                let handles: Vec<_> = (0..8).map(|_| {
                    let pool_clone = Arc::clone(&pool);
                    thread::spawn(move || {
                        let mut allocations = Vec::new();
                        for _ in 0..50 {
                            let buffer = pool_clone.allocate(64 * 1024).unwrap(); // 64KB
                            allocations.push(buffer);
                        }
                        allocations
                    })
                }).collect();

                let mut total_allocations = 0;
                for handle in handles {
                    let allocations = handle.join().unwrap();
                    total_allocations += allocations.len();
                }

                black_box(total_allocations);
            })
        });
    });
}

/// Benchmark memory tracking overhead
fn bench_memory_tracking(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark memory tracker
        let tracker = MemoryTracker::new();

        c.bench_function("memory_tracking_overhead", |b| {
            b.iter(|| {
                let allocation_id = tracker.allocate(1024 * 1024).unwrap(); // 1MB
                tracker.record_access(allocation_id, 0, 1024).unwrap();
                tracker.deallocate(allocation_id).unwrap();
            })
        });

        // Benchmark allocation strategy selection
        let strategies = vec![
            AllocationStrategy::FirstFit,
            AllocationStrategy::BestFit,
            AllocationStrategy::WorstFit,
        ];

        for strategy in strategies {
            c.bench_function(&format!("allocation_strategy_{:?}", strategy), |b| {
                let mut pool = MemoryPool::new(1024 * 1024 * 10).unwrap(); // 10MB
                pool.set_strategy(strategy);

                b.iter(|| {
                    let mut allocations = Vec::new();
                    // Allocate various sizes to test strategy
                    for size in [1024, 2048, 4096, 8192, 16384] {
                        if let Ok(buffer) = pool.allocate(size) {
                            allocations.push(buffer);
                        }
                    }
                    black_box(allocations.len());
                })
            });
        }
    });
}

/// Benchmark memory-mapped file operations
fn bench_memory_mapped_files(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        use std::fs;
        use tempfile::NamedTempFile;

        // Create temporary file for benchmarking
        let temp_file = NamedTempFile::new().unwrap();
        let file_size = 64 * 1024 * 1024; // 64MB

        // Write test data to file
        let test_data = vec![42u8; file_size];
        fs::write(temp_file.path(), &test_data).unwrap();

        // Benchmark memory-mapped file access
        c.bench_function("memory_mapped_file_64mb", |b| {
            b.iter(|| {
                let mmap = unsafe { memmap2::Mmap::map(&fs::File::open(temp_file.path()).unwrap()).unwrap() };
                let mut sum = 0u64;

                // Access every 1024th byte to simulate sparse access
                for i in (0..mmap.len()).step_by(1024) {
                    sum = sum.wrapping_add(mmap[i] as u64);
                }

                black_box(sum);
            })
        });

        // Benchmark regular file I/O for comparison
        c.bench_function("regular_file_io_64mb", |b| {
            b.iter(|| {
                let data = fs::read(temp_file.path()).unwrap();
                let mut sum = 0u64;

                for &byte in &data {
                    sum = sum.wrapping_add(byte as u64);
                }

                black_box(sum);
            })
        });
    });
}

criterion_group!(
    name = memory_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(15))
        .noise_threshold(0.05);
    targets = bench_memory_allocation, bench_memory_access_patterns, bench_memory_pressure,
             bench_concurrent_memory, bench_memory_tracking, bench_memory_mapped_files
);

criterion_main!(memory_benches);