//! Performance benchmarks for adapter stack activation
//!
//! Benchmarks for activating/deactivating adapter stacks and VRAM allocation.
//!
//! ## Running Benchmarks
//!
//! Full suite:
//! ```bash
//! cargo bench -p adapteros-lora-worker --bench stack_activation
//! ```
//!
//! Specific benchmark:
//! ```bash
//! cargo bench -p adapteros-lora-worker --bench stack_activation -- stack_activation
//! cargo bench -p adapteros-lora-worker --bench stack_activation -- vram_allocation
//! ```
//!
//! [source: crates/adapteros-lora-worker/benches/stack_activation.rs]

#![allow(dead_code)]
#![allow(unused_must_use)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// Mock adapter for benchmarking
#[derive(Clone)]
struct Adapter {
    id: String,
    size_bytes: usize,
    rank: u16,
    loaded: bool,
    vram_ptr: Option<u64>,
}

impl Adapter {
    fn new(id: impl Into<String>, rank: u16) -> Self {
        let size_bytes = Self::calculate_size(rank);
        Self {
            id: id.into(),
            size_bytes,
            rank,
            loaded: false,
            vram_ptr: None,
        }
    }

    fn calculate_size(rank: u16) -> usize {
        // Approximate LoRA adapter size: rank × hidden_dim × num_layers × 4 (fp32)
        let hidden_dim = 4096;
        let num_layers = 32;
        (rank as usize) * hidden_dim * num_layers * 4
    }

    fn load_to_vram(&mut self, base_ptr: u64) -> Result<(), &'static str> {
        if self.loaded {
            return Err("Already loaded");
        }
        self.vram_ptr = Some(base_ptr);
        self.loaded = true;
        Ok(())
    }

    fn unload_from_vram(&mut self) {
        self.vram_ptr = None;
        self.loaded = false;
    }
}

// Mock adapter stack
struct AdapterStack {
    id: String,
    adapters: Vec<Adapter>,
    active: bool,
}

impl AdapterStack {
    fn new(id: impl Into<String>, adapters: Vec<Adapter>) -> Self {
        Self {
            id: id.into(),
            adapters,
            active: false,
        }
    }

    fn activate(&mut self) -> Result<(), &'static str> {
        if self.active {
            return Err("Already active");
        }

        let mut vram_offset = 0u64;
        for adapter in &mut self.adapters {
            adapter.load_to_vram(vram_offset)?;
            vram_offset += adapter.size_bytes as u64;
        }

        self.active = true;
        Ok(())
    }

    fn deactivate(&mut self) {
        for adapter in &mut self.adapters {
            adapter.unload_from_vram();
        }
        self.active = false;
    }

    fn total_size(&self) -> usize {
        self.adapters.iter().map(|a| a.size_bytes).sum()
    }
}

// =============================================================================
// Stack Activation Benchmarks
// =============================================================================

/// Benchmark stack activation with varying adapter counts
///
/// Measures time to activate stacks with 1, 2, 4, 8 adapters
fn bench_stack_activation(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_activation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [1, 2, 4, 8];
    let rank = 16; // Standard LoRA rank

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let adapters: Vec<Adapter> = (0..count)
                    .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                    .collect();

                let mut stack = AdapterStack::new("test-stack", adapters);
                let result = stack.activate();
                black_box(result);
                black_box(stack);
            });
        });
    }

    group.finish();
}

/// Benchmark stack activation with varying adapter ranks
///
/// Different ranks affect memory allocation size
fn bench_stack_activation_by_rank(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_activation_by_rank");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let ranks = [4, 8, 16, 32, 64];
    let adapter_count = 4;

    for rank in ranks {
        let total_size = Adapter::calculate_size(rank) * adapter_count;
        group.throughput(Throughput::Bytes(total_size as u64));

        group.bench_with_input(BenchmarkId::new("rank", rank), &rank, |b, &rank| {
            b.iter(|| {
                let adapters: Vec<Adapter> = (0..adapter_count)
                    .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                    .collect();

                let mut stack = AdapterStack::new("test-stack", adapters);
                let result = stack.activate();
                black_box(result);
                black_box(stack);
            });
        });
    }

    group.finish();
}

/// Benchmark activation of multiple stacks sequentially
///
/// Simulates activating multiple stacks in a single decision cycle
fn bench_multi_stack_activation(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_stack_activation");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let stack_counts = [1, 2, 4, 8];
    let adapters_per_stack = 4;
    let rank = 16;

    for stack_count in stack_counts {
        group.throughput(Throughput::Elements(
            (stack_count * adapters_per_stack) as u64,
        ));

        group.bench_with_input(
            BenchmarkId::new("stacks", stack_count),
            &stack_count,
            |b, &stack_count| {
                b.iter(|| {
                    let mut stacks: Vec<AdapterStack> = (0..stack_count)
                        .map(|i| {
                            let adapters: Vec<Adapter> = (0..adapters_per_stack)
                                .map(|j| Adapter::new(format!("stack{}-adapter{}", i, j), rank))
                                .collect();
                            AdapterStack::new(format!("stack-{}", i), adapters)
                        })
                        .collect();

                    for stack in &mut stacks {
                        let _ = stack.activate();
                    }

                    black_box(stacks);
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Stack Deactivation Benchmarks
// =============================================================================

/// Benchmark stack deactivation and cleanup time
///
/// Measures time to unload adapters and free VRAM
fn bench_stack_deactivation(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_deactivation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [1, 2, 4, 8, 16];
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter_batched(
                || {
                    // Setup: create and activate stack
                    let adapters: Vec<Adapter> = (0..count)
                        .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                        .collect();
                    let mut stack = AdapterStack::new("test-stack", adapters);
                    stack.activate().unwrap();
                    stack
                },
                |mut stack| {
                    // Benchmark deactivation
                    stack.deactivate();
                    black_box(stack);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark activation/deactivation cycle
///
/// Measures round-trip time for activate → deactivate
fn bench_activation_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("activation_cycle");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [2, 4, 8];
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let adapters: Vec<Adapter> = (0..count)
                    .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                    .collect();

                let mut stack = AdapterStack::new("test-stack", adapters);

                // Activate
                stack.activate().unwrap();

                // Deactivate
                stack.deactivate();

                black_box(stack);
            });
        });
    }

    group.finish();
}

// =============================================================================
// VRAM Allocation Benchmarks
// =============================================================================

/// Benchmark VRAM allocation overhead per adapter size
///
/// Measures allocation latency for different adapter ranks
fn bench_vram_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vram_allocation");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let ranks = [4, 8, 16, 32, 64];

    for rank in ranks {
        let size = Adapter::calculate_size(rank);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("rank", rank), &rank, |b, &rank| {
            b.iter(|| {
                let mut adapter = Adapter::new("test-adapter", rank);
                let result = adapter.load_to_vram(0x1000_0000); // Mock VRAM base
                black_box(result);
                black_box(adapter);
            });
        });
    }

    group.finish();
}

/// Benchmark batch VRAM allocation
///
/// Allocates multiple adapters in a single operation
fn bench_batch_vram_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_vram_allocation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let batch_sizes = [4, 8, 16, 32];
    let rank = 16;

    for batch_size in batch_sizes {
        let total_size = Adapter::calculate_size(rank) * batch_size;
        group.throughput(Throughput::Bytes(total_size as u64));

        group.bench_with_input(
            BenchmarkId::new("adapters", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let mut adapters: Vec<Adapter> = (0..batch_size)
                        .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                        .collect();

                    let mut vram_offset = 0x1000_0000u64;
                    for adapter in &mut adapters {
                        adapter.load_to_vram(vram_offset).unwrap();
                        vram_offset += adapter.size_bytes as u64;
                    }

                    black_box(adapters);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark VRAM fragmentation impact
///
/// Simulates allocation with gaps (fragmented VRAM)
fn bench_vram_fragmentation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vram_fragmentation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [8, 16, 32];
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let mut adapters: Vec<Adapter> = (0..count)
                    .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                    .collect();

                // Allocate with gaps (simulate fragmentation)
                let gap_size = 1024 * 1024; // 1MB gap between adapters
                let mut vram_offset = 0x1000_0000u64;
                for adapter in &mut adapters {
                    adapter.load_to_vram(vram_offset).unwrap();
                    vram_offset += adapter.size_bytes as u64 + gap_size;
                }

                black_box(adapters);
            });
        });
    }

    group.finish();
}

/// Benchmark VRAM deallocation
///
/// Measures time to free VRAM for multiple adapters
fn bench_vram_deallocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vram_deallocation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [4, 8, 16, 32];
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter_batched(
                || {
                    // Setup: allocate adapters
                    let mut adapters: Vec<Adapter> = (0..count)
                        .map(|i| Adapter::new(format!("adapter-{}", i), rank))
                        .collect();

                    let mut vram_offset = 0x1000_0000u64;
                    for adapter in &mut adapters {
                        adapter.load_to_vram(vram_offset).unwrap();
                        vram_offset += adapter.size_bytes as u64;
                    }
                    adapters
                },
                |mut adapters| {
                    // Benchmark deallocation
                    for adapter in &mut adapters {
                        adapter.unload_from_vram();
                    }
                    black_box(adapters);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark total VRAM usage calculation
///
/// Measures overhead of summing memory usage across all adapters
fn bench_total_vram_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("total_vram_calculation");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let stack_counts = [4, 8, 16, 32, 64];
    let adapters_per_stack = 4;
    let rank = 16;

    for stack_count in stack_counts {
        group.throughput(Throughput::Elements(
            (stack_count * adapters_per_stack) as u64,
        ));

        group.bench_with_input(
            BenchmarkId::new("stacks", stack_count),
            &stack_count,
            |b, &stack_count| {
                let stacks: Vec<AdapterStack> = (0..stack_count)
                    .map(|i| {
                        let adapters: Vec<Adapter> = (0..adapters_per_stack)
                            .map(|j| Adapter::new(format!("stack{}-adapter{}", i, j), rank))
                            .collect();
                        AdapterStack::new(format!("stack-{}", i), adapters)
                    })
                    .collect();

                b.iter(|| {
                    let total_vram: usize = stacks.iter().map(|s| s.total_size()).sum();
                    black_box(total_vram);
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Register Benchmark Groups
// =============================================================================

criterion_group!(
    name = activation_benchmarks;
    config = Criterion::default();
    targets = bench_stack_activation, bench_stack_activation_by_rank, bench_multi_stack_activation
);

criterion_group!(
    name = deactivation_benchmarks;
    config = Criterion::default();
    targets = bench_stack_deactivation, bench_activation_cycle
);

criterion_group!(
    name = vram_benchmarks;
    config = Criterion::default();
    targets = bench_vram_allocation, bench_batch_vram_allocation, bench_vram_fragmentation, bench_vram_deallocation, bench_total_vram_calculation
);

criterion_main!(
    activation_benchmarks,
    deactivation_benchmarks,
    vram_benchmarks
);
