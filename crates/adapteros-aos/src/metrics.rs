//! Performance metrics for .aos file operations

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct LoadMetrics {
    total_loads: AtomicU64,
    total_load_time_us: AtomicU64,
    total_verify_time_us: AtomicU64,
    failed_loads: AtomicU64,
}

impl LoadMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_load(&self, load_time: Duration, verify_time: Duration) {
        self.total_loads.fetch_add(1, Ordering::Relaxed);
        self.total_load_time_us
            .fetch_add(load_time.as_micros() as u64, Ordering::Relaxed);
        self.total_verify_time_us
            .fetch_add(verify_time.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failed_loads.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total_loads(&self) -> u64 {
        self.total_loads.load(Ordering::Relaxed)
    }

    pub fn avg_load_time(&self) -> Duration {
        let total = self.total_loads.load(Ordering::Relaxed);
        if total == 0 {
            return Duration::ZERO;
        }
        let total_us = self.total_load_time_us.load(Ordering::Relaxed);
        Duration::from_micros(total_us / total)
    }

    pub fn failed_loads(&self) -> u64 {
        self.failed_loads.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub struct CacheMetrics {
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    size_bytes: AtomicU64,
    evicted_bytes: AtomicU64,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_eviction(&self, size_bytes: u64) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
        self.evicted_bytes.fetch_add(size_bytes, Ordering::Relaxed);
        self.size_bytes.fetch_sub(size_bytes, Ordering::Relaxed);
    }

    pub fn update_size(&self, delta: i64) {
        if delta >= 0 {
            self.size_bytes.fetch_add(delta as u64, Ordering::Relaxed);
        } else {
            self.size_bytes
                .fetch_sub((-delta) as u64, Ordering::Relaxed);
        }
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes.load(Ordering::Relaxed)
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits();
        let misses = self.misses();
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64
    }
}

#[derive(Debug, Default)]
pub struct SwapMetrics {
    total_swaps: AtomicU64,
    total_swap_time_us: AtomicU64,
    failed_swaps: AtomicU64,
    rollbacks: AtomicU64,
}

impl SwapMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_swap(&self, swap_time: Duration) {
        self.total_swaps.fetch_add(1, Ordering::Relaxed);
        self.total_swap_time_us
            .fetch_add(swap_time.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failed_swaps.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_rollback(&self) {
        self.rollbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total_swaps(&self) -> u64 {
        self.total_swaps.load(Ordering::Relaxed)
    }

    pub fn rollbacks(&self) -> u64 {
        self.rollbacks.load(Ordering::Relaxed)
    }
}
