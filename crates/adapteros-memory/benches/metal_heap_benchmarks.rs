//! Benchmarks for Metal heap observer overhead measurement
//!
//! Run with:
//!   cargo bench --bench metal_heap_benchmarks
//!
//! These benchmarks measure:
//! - Heap observer initialization overhead
//! - FFI call overhead
//! - Statistics collection performance
//! - Fragmentation detection performance
//! - Memory tracking throughput

use adapteros_core::B3Hash;
use adapteros_memory::{
    FFIFragmentationMetrics, FragmentationMetrics, FragmentationType, HeapAllocation, HeapState,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// BENCHMARK FIXTURES
// ============================================================================

/// Mock heap observer for benchmarking on non-Metal systems
struct BenchHeapObserver {
    allocations: HashMap<u64, HeapAllocation>,
    heap_states: HashMap<u64, HeapState>,
    next_buffer_id: u64,
}

impl BenchHeapObserver {
    fn new() -> Self {
        Self {
            allocations: HashMap::new(),
            heap_states: HashMap::new(),
            next_buffer_id: 1,
        }
    }

    fn record_allocation(&mut self, heap_id: u64, size: u64, offset: u64) -> u64 {
        let buffer_id = self.next_buffer_id;
        self.next_buffer_id += 1;

        let allocation = HeapAllocation {
            allocation_id: Uuid::new_v4(),
            heap_id,
            buffer_id,
            size_bytes: size,
            offset_bytes: offset,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros(),
            memory_addr: Some(0x1000 + offset),
            storage_mode: "shared".to_string(),
        };

        self.allocations.insert(buffer_id, allocation);
        buffer_id
    }

    fn get_memory_stats(&self) -> (u64, usize) {
        let total = self.allocations.values().map(|a| a.size_bytes).sum();
        let count = self.allocations.len();
        (total, count)
    }

    fn detect_fragmentation(&self) -> FragmentationMetrics {
        let allocations: Vec<_> = self.allocations.values().collect();

        if allocations.is_empty() {
            return FragmentationMetrics {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 1.0,
                fragmentation_type: FragmentationType::None,
            };
        }

        let mut sorted = allocations;
        sorted.sort_by_key(|a| a.offset_bytes);

        let total_allocated: u64 = sorted.iter().map(|a| a.size_bytes).sum();
        let heap_states_total: u64 = self.heap_states.values().map(|h| h.total_size).sum();

        let mut free_blocks = Vec::new();
        let mut current_offset = 0u64;

        for alloc in &sorted {
            if alloc.offset_bytes > current_offset {
                free_blocks.push(alloc.offset_bytes - current_offset);
            }
            current_offset = alloc.offset_bytes + alloc.size_bytes;
        }

        if current_offset < heap_states_total {
            free_blocks.push(heap_states_total - current_offset);
        }

        let total_free: u64 = free_blocks.iter().sum();
        let num_free = free_blocks.len();
        let avg_free = if num_free > 0 {
            total_free / num_free as u64
        } else {
            0
        };
        let largest_free = free_blocks.iter().max().copied().unwrap_or(0);

        let external_frag = if heap_states_total > 0 {
            total_free as f32 / heap_states_total as f32
        } else {
            0.0
        };

        let internal_frag = if total_allocated > 0 {
            (total_allocated as f32 * 0.05 / total_allocated as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let frag_ratio = (external_frag + internal_frag) / 2.0;

        let max_recoverable = if num_free > 1 {
            total_free - largest_free
        } else {
            0
        };

        let compaction_eff = if max_recoverable > 0 && total_free > 0 {
            1.0 - (max_recoverable as f32 / total_free as f32)
        } else {
            1.0
        };

        let frag_type = match frag_ratio {
            r if r < 0.2 => FragmentationType::Low,
            r if r < 0.5 => FragmentationType::Medium,
            r if r < 0.8 => FragmentationType::High,
            _ => FragmentationType::Critical,
        };

        FragmentationMetrics {
            fragmentation_ratio: frag_ratio,
            external_fragmentation: external_frag,
            internal_fragmentation: internal_frag,
            free_blocks: num_free,
            total_free_bytes: total_free,
            avg_free_block_size: avg_free,
            largest_free_block: largest_free,
            compaction_efficiency: compaction_eff,
            fragmentation_type: frag_type,
        }
    }
}

// ============================================================================
// INITIALIZATION BENCHMARKS
// ============================================================================

fn bench_observer_creation(c: &mut Criterion) {
    c.bench_function("bench_mock_observer_creation", |b| {
        b.iter(|| {
            let _observer = BenchHeapObserver::new();
            black_box(_observer)
        });
    });
}

// ============================================================================
// ALLOCATION TRACKING BENCHMARKS
// ============================================================================

fn bench_allocation_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_tracking");

    for alloc_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(alloc_count),
            alloc_count,
            |b, &alloc_count| {
                let mut observer = BenchHeapObserver::new();
                let heap_id = black_box(1u64);

                b.iter(|| {
                    for i in 0..alloc_count {
                        let size = black_box(1024u64);
                        let offset = black_box((i as u64) * 1024);
                        observer.record_allocation(heap_id, size, offset);
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// STATISTICS COLLECTION BENCHMARKS
// ============================================================================

fn bench_memory_stats_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_stats");

    for alloc_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("get_memory_stats", alloc_count),
            alloc_count,
            |b, &alloc_count| {
                let mut observer = BenchHeapObserver::new();

                // Setup allocations
                for i in 0..alloc_count {
                    observer.record_allocation(1, 1024, (i as u64) * 1024);
                }

                b.iter(|| {
                    let (total, count) = observer.get_memory_stats();
                    black_box((total, count))
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// FRAGMENTATION DETECTION BENCHMARKS
// ============================================================================

fn bench_fragmentation_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragmentation_detection");

    // Benchmark fragmentation detection with different allocation patterns
    for pattern in ["contiguous", "fragmented", "sparse"].iter() {
        group.bench_with_input(
            BenchmarkId::new("detect_fragmentation", pattern),
            pattern,
            |b, &pattern| {
                let mut observer = BenchHeapObserver::new();

                // Create different allocation patterns
                match pattern {
                    "contiguous" => {
                        // Contiguous allocations (no gaps)
                        for i in 0..100 {
                            observer.record_allocation(1, 1024, (i as u64) * 1024);
                        }
                    }
                    "fragmented" => {
                        // Fragmented allocations (many gaps)
                        for i in 0..100 {
                            observer.record_allocation(1, 256, (i as u64) * 2048);
                        }
                    }
                    "sparse" => {
                        // Very sparse allocations
                        for i in 0..100 {
                            observer.record_allocation(1, 128, (i as u64) * 16384);
                        }
                    }
                    _ => {}
                }

                // Setup heap state
                observer.heap_states.insert(
                    1,
                    HeapState {
                        heap_id: 1,
                        total_size: 10 * 1024 * 1024,
                        used_size: 100 * 1024,
                        allocation_count: 100,
                        heap_hash: B3Hash::hash(b"bench"),
                        allocation_order_hash: B3Hash::hash(b"order"),
                    },
                );

                b.iter(|| {
                    let metrics = observer.detect_fragmentation();
                    black_box(metrics)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// FFI INTERFACE BENCHMARKS
// ============================================================================

fn bench_ffi_structure_creation(c: &mut Criterion) {
    c.bench_function("bench_ffi_fragmentation_metrics_create", |b| {
        b.iter(|| {
            let metrics = FFIFragmentationMetrics {
                fragmentation_ratio: black_box(0.5f32),
                external_fragmentation: black_box(0.3f32),
                internal_fragmentation: black_box(0.2f32),
                free_blocks: black_box(10u32),
                total_free_bytes: black_box(1024u64),
                avg_free_block_size: black_box(102u64),
                largest_free_block: black_box(512u64),
                compaction_efficiency: black_box(0.8f32),
            };
            black_box(metrics)
        });
    });
}

// ============================================================================
// THROUGHPUT BENCHMARKS
// ============================================================================

fn bench_allocation_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.sample_size(100);

    group.bench_function("allocations_per_second", |b| {
        b.iter(|| {
            let mut observer = BenchHeapObserver::new();
            for i in 0..10000 {
                observer.record_allocation(1, 1024, (i as u64) * 1024);
            }
            black_box(observer)
        });
    });

    group.finish();
}

// ============================================================================
// SCALABILITY BENCHMARKS
// ============================================================================

fn bench_multi_heap_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_heap_scalability");

    for heap_count in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("get_memory_stats", heap_count),
            heap_count,
            |b, &heap_count| {
                let mut observer = BenchHeapObserver::new();

                // Setup multiple heaps
                for heap_id in 0..heap_count {
                    for i in 0..10 {
                        observer.record_allocation(heap_id as u64, 1024, (i as u64) * 1024);
                    }

                    observer.heap_states.insert(
                        heap_id as u64,
                        HeapState {
                            heap_id: heap_id as u64,
                            total_size: 1024 * 1024,
                            used_size: 10 * 1024,
                            allocation_count: 10,
                            heap_hash: B3Hash::hash(format!("heap_{}", heap_id).as_bytes()),
                            allocation_order_hash: B3Hash::hash(b"order"),
                        },
                    );
                }

                b.iter(|| {
                    let (total, count) = observer.get_memory_stats();
                    black_box((total, count))
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// CRITERION SETUP
// ============================================================================

criterion_group!(
    benches,
    bench_observer_creation,
    bench_allocation_tracking,
    bench_memory_stats_collection,
    bench_fragmentation_detection,
    bench_ffi_structure_creation,
    bench_allocation_throughput,
    bench_multi_heap_scalability,
);

criterion_main!(benches);

// ============================================================================
// BENCHMARK DOCUMENTATION
// ============================================================================
//
// # Running Benchmarks
//
// ## All benchmarks
// ```bash
// cargo bench --bench metal_heap_benchmarks
// ```
//
// ## Specific benchmark group
// ```bash
// cargo bench --bench metal_heap_benchmarks -- allocation_tracking
// cargo bench --bench metal_heap_benchmarks -- fragmentation_detection
// cargo bench --bench metal_heap_benchmarks -- throughput
// ```
//
// ## With baseline comparison
// ```bash
// cargo bench --bench metal_heap_benchmarks -- --baseline main
// ```
//
// ## Expected Performance Targets
//
// - Observer creation: < 1 μs
// - Single allocation tracking: < 10 μs
// - Memory stats retrieval (100 allocs): < 100 μs
// - Fragmentation detection (100 allocs): < 500 μs
// - FFI structure creation: < 1 μs
// - Allocation throughput: > 100k allocations/sec
//
// ## Interpreting Results
//
// When benchmarks regress:
// 1. Check allocation tracking overhead
// 2. Review fragmentation detection algorithm
// 3. Verify statistics collection doesn't iterate excessively
// 4. Profile with `perf` or `Instruments` if needed
