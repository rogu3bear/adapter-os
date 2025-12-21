//! Performance benchmarks for LoRA adapter lifecycle management
//!
//! Benchmarks for state transitions, promotion/demotion latency, and eviction.
//!
//! ## Running Benchmarks
//!
//! Full suite:
//! ```bash
//! cargo bench -p adapteros-lora-lifecycle --bench lifecycle_benchmarks
//! ```
//!
//! Specific benchmark:
//! ```bash
//! cargo bench -p adapteros-lora-lifecycle --bench lifecycle_benchmarks -- state_transitions
//! cargo bench -p adapteros-lora-lifecycle --bench lifecycle_benchmarks -- promotion
//! cargo bench -p adapteros-lora-lifecycle --bench lifecycle_benchmarks -- eviction
//! ```
//!
//! ## Lifecycle States
//!
//! ```
//! Unloaded → Cold → Warm → Hot → Resident
//!     ↑                          ↓
//!     └──── (eviction) ──────────┘
//! ```
//!
//! [source: crates/adapteros-lora-lifecycle/benches/lifecycle_benchmarks.rs]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// Mock lifecycle state for benchmarking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleState {
    Unloaded,
    Cold,
    Warm,
    Hot,
    Resident,
}

impl LifecycleState {
    fn next(&self) -> Option<Self> {
        match self {
            Self::Unloaded => Some(Self::Cold),
            Self::Cold => Some(Self::Warm),
            Self::Warm => Some(Self::Hot),
            Self::Hot => Some(Self::Resident),
            Self::Resident => None,
        }
    }

    fn prev(&self) -> Option<Self> {
        match self {
            Self::Unloaded => None,
            Self::Cold => Some(Self::Unloaded),
            Self::Warm => Some(Self::Cold),
            Self::Hot => Some(Self::Warm),
            Self::Resident => Some(Self::Hot),
        }
    }
}

// Mock adapter with lifecycle state
struct Adapter {
    id: String,
    state: LifecycleState,
    activation_pct: f64,
    last_used: u64,
    size_bytes: usize,
}

impl Adapter {
    fn new(id: impl Into<String>, size_bytes: usize) -> Self {
        Self {
            id: id.into(),
            state: LifecycleState::Unloaded,
            activation_pct: 0.0,
            last_used: 0,
            size_bytes,
        }
    }

    fn promote(&mut self) -> bool {
        if let Some(next_state) = self.state.next() {
            self.state = next_state;
            true
        } else {
            false
        }
    }

    fn demote(&mut self) -> bool {
        if let Some(prev_state) = self.state.prev() {
            self.state = prev_state;
            true
        } else {
            false
        }
    }

    fn record_activation(&mut self, current_tick: u64) {
        self.last_used = current_tick;
        self.activation_pct = (self.activation_pct * 0.95) + 5.0; // EWMA
    }
}

// =============================================================================
// State Transition Benchmarks
// =============================================================================

/// Benchmark state transitions across lifecycle
///
/// Measures time to transition: Unloaded → Cold → Warm → Hot → Resident
fn bench_state_transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_transitions");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let transitions = [
        ("unloaded_to_cold", LifecycleState::Unloaded),
        ("cold_to_warm", LifecycleState::Cold),
        ("warm_to_hot", LifecycleState::Warm),
        ("hot_to_resident", LifecycleState::Hot),
    ];

    for (label, initial_state) in transitions {
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::new("transition", label),
            &initial_state,
            |b, &initial_state| {
                b.iter(|| {
                    let mut adapter = Adapter::new("test-adapter", 64 * 1024 * 1024);
                    adapter.state = initial_state;
                    let promoted = adapter.promote();
                    black_box(promoted);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark full lifecycle progression
///
/// Measures time to go from Unloaded to Resident (all 4 promotions)
fn bench_full_lifecycle_progression(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_lifecycle_progression");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(4)); // 4 transitions

    group.bench_function("unloaded_to_resident", |b| {
        b.iter(|| {
            let mut adapter = Adapter::new("test-adapter", 64 * 1024 * 1024);
            while adapter.promote() {
                black_box(&adapter.state);
            }
            black_box(adapter);
        });
    });

    group.finish();
}

// =============================================================================
// Promotion Latency Benchmarks
// =============================================================================

/// Benchmark promotion based on activation percentage
///
/// Simulates promotion decision logic based on usage patterns
fn bench_promotion_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("promotion_latency");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let activation_thresholds = [
        ("low_activation", 5.0),       // 5% activation
        ("medium_activation", 25.0),   // 25% activation
        ("high_activation", 75.0),     // 75% activation
        ("critical_activation", 95.0), // 95% activation
    ];

    for (label, activation_pct) in activation_thresholds {
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::new("activation_pct", label),
            &activation_pct,
            |b, &activation_pct| {
                b.iter(|| {
                    let mut adapter = Adapter::new("test-adapter", 64 * 1024 * 1024);
                    adapter.state = LifecycleState::Warm;
                    adapter.activation_pct = activation_pct;

                    // Promotion logic: >= 20% activation promotes to Hot
                    let should_promote = activation_pct >= 20.0;
                    if should_promote {
                        adapter.promote();
                    }
                    black_box(adapter);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark batch promotion of multiple adapters
///
/// Simulates promoting multiple adapters in a single decision cycle
fn bench_batch_promotion(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_promotion");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let batch_sizes = [1, 4, 8, 16, 32];

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("adapters", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let mut adapters: Vec<Adapter> = (0..batch_size)
                        .map(|i| {
                            let mut adapter =
                                Adapter::new(format!("adapter-{}", i), 64 * 1024 * 1024);
                            adapter.state = LifecycleState::Cold;
                            adapter.activation_pct = 30.0; // Above promotion threshold
                            adapter
                        })
                        .collect();

                    for adapter in &mut adapters {
                        adapter.promote();
                    }
                    black_box(adapters);
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Demotion Latency Benchmarks
// =============================================================================

/// Benchmark demotion after timeout
///
/// Measures time to demote adapters that haven't been used recently
fn bench_demotion_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("demotion_latency");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let timeout_durations = [
        ("1min_timeout", 60),
        ("5min_timeout", 300),
        ("15min_timeout", 900),
        ("1hr_timeout", 3600),
    ];

    for (label, timeout_secs) in timeout_durations {
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::new("timeout", label),
            &timeout_secs,
            |b, &timeout_secs| {
                b.iter(|| {
                    let mut adapter = Adapter::new("test-adapter", 64 * 1024 * 1024);
                    adapter.state = LifecycleState::Hot;
                    adapter.last_used = 1000;

                    let current_tick = 1000 + timeout_secs + 1; // Expired
                    let should_demote = (current_tick - adapter.last_used) > timeout_secs;

                    if should_demote {
                        adapter.demote();
                    }
                    black_box(adapter);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark decay of activation percentage
///
/// Measures EWMA update performance for activation tracking
fn bench_activation_decay(c: &mut Criterion) {
    let mut group = c.benchmark_group("activation_decay");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let decay_iterations = [10, 100, 1000];

    for iterations in decay_iterations {
        group.throughput(Throughput::Elements(iterations as u64));

        group.bench_with_input(
            BenchmarkId::new("iterations", iterations),
            &iterations,
            |b, &iterations| {
                b.iter(|| {
                    let mut adapter = Adapter::new("test-adapter", 64 * 1024 * 1024);
                    adapter.activation_pct = 80.0;

                    for tick in 0..iterations {
                        adapter.record_activation(tick);
                    }
                    black_box(adapter);
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Eviction Under Memory Pressure
// =============================================================================

/// Benchmark emergency eviction time under memory pressure
///
/// Measures time to evict adapters when system memory is low
fn bench_eviction_under_pressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("eviction_under_pressure");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [4, 8, 16, 32, 64];

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let mut adapters: Vec<Adapter> = (0..count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), 64 * 1024 * 1024);
                        adapter.state = LifecycleState::Warm;
                        adapter.activation_pct = (i as f64 / count as f64) * 100.0;
                        adapter.last_used = 1000 - i as u64; // Older adapters have lower last_used
                        adapter
                    })
                    .collect();

                // Sort by activation % (ascending) to evict lowest-priority first
                adapters.sort_by(|a, b| {
                    a.activation_pct
                        .partial_cmp(&b.activation_pct)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Evict bottom 25%
                let evict_count = (count / 4).max(1);
                for adapter in adapters.iter_mut().take(evict_count) {
                    adapter.state = LifecycleState::Unloaded;
                }

                black_box(adapters);
            });
        });
    }

    group.finish();
}

/// Benchmark LRU eviction policy
///
/// Measures time to find and evict least-recently-used adapters
fn bench_lru_eviction_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("lru_eviction_policy");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [8, 16, 32, 64];

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let mut adapters: Vec<Adapter> = (0..count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), 64 * 1024 * 1024);
                        adapter.state = LifecycleState::Hot;
                        adapter.last_used = 1000 + (i % 10) as u64; // Mixed access patterns
                        adapter
                    })
                    .collect();

                // Sort by last_used (ascending) for LRU
                adapters.sort_by_key(|a| a.last_used);

                // Evict oldest 2 adapters
                for adapter in adapters.iter_mut().take(2) {
                    while adapter.demote() {}
                }

                black_box(adapters);
            });
        });
    }

    group.finish();
}

/// Benchmark memory headroom calculation
///
/// Measures overhead of calculating available memory and eviction targets
fn bench_memory_headroom_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_headroom_calculation");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [16, 32, 64, 128];

    for count in adapter_counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                let adapters: Vec<Adapter> = (0..count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), 64 * 1024 * 1024);
                        adapter.state = LifecycleState::Warm;
                        adapter
                    })
                    .collect();

                let total_mem = 16 * 1024 * 1024 * 1024u64; // 16 GB
                let used_mem: u64 = adapters.iter().map(|a| a.size_bytes as u64).sum();
                let free_mem = total_mem.saturating_sub(used_mem);
                let headroom_pct = (free_mem as f64 / total_mem as f64) * 100.0;

                // Eviction triggered when headroom < 15%
                let should_evict = headroom_pct < 15.0;

                black_box((used_mem, headroom_pct, should_evict));
            });
        });
    }

    group.finish();
}

// =============================================================================
// Register Benchmark Groups
// =============================================================================

criterion_group!(
    name = state_benchmarks;
    config = Criterion::default();
    targets = bench_state_transitions, bench_full_lifecycle_progression
);

criterion_group!(
    name = promotion_benchmarks;
    config = Criterion::default();
    targets = bench_promotion_latency, bench_batch_promotion
);

criterion_group!(
    name = demotion_benchmarks;
    config = Criterion::default();
    targets = bench_demotion_latency, bench_activation_decay
);

criterion_group!(
    name = eviction_benchmarks;
    config = Criterion::default();
    targets = bench_eviction_under_pressure, bench_lru_eviction_policy, bench_memory_headroom_calculation
);

criterion_main!(
    state_benchmarks,
    promotion_benchmarks,
    demotion_benchmarks,
    eviction_benchmarks
);
