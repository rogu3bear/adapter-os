//! Router performance metrics and monitoring

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per-adapter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetrics {
    /// Adapter ID
    pub adapter_id: u16,
    /// Total activations
    pub activation_count: u64,
    /// Total time spent in adapter forward passes (microseconds)
    pub total_latency_us: u64,
    /// Average latency per activation (microseconds)
    pub avg_latency_us: f64,
    /// p95 latency (microseconds)
    pub p95_latency_us: f64,
    /// p99 latency (microseconds)
    pub p99_latency_us: f64,
    /// Recent latency samples for percentile calculation
    latency_samples: Vec<u64>,
}

impl AdapterMetrics {
    pub fn new(adapter_id: u16) -> Self {
        Self {
            adapter_id,
            activation_count: 0,
            total_latency_us: 0,
            avg_latency_us: 0.0,
            p95_latency_us: 0.0,
            p99_latency_us: 0.0,
            latency_samples: Vec::with_capacity(1000),
        }
    }

    /// Record a new activation with latency
    pub fn record_activation(&mut self, latency_us: u64) {
        self.activation_count += 1;
        self.total_latency_us += latency_us;
        self.avg_latency_us = self.total_latency_us as f64 / self.activation_count as f64;

        // Keep last 1000 samples for percentile calculation
        if self.latency_samples.len() >= 1000 {
            self.latency_samples.remove(0);
        }
        self.latency_samples.push(latency_us);

        // Update percentiles
        self.update_percentiles();
    }

    fn update_percentiles(&mut self) {
        if self.latency_samples.is_empty() {
            return;
        }

        let mut sorted = self.latency_samples.clone();
        sorted.sort_unstable();

        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p99_idx = (sorted.len() as f64 * 0.99) as usize;

        self.p95_latency_us = sorted.get(p95_idx).copied().unwrap_or(0) as f64;
        self.p99_latency_us = sorted.get(p99_idx).copied().unwrap_or(0) as f64;
    }
}

/// Router overhead metrics (Ruleset #11 monitoring)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterOverheadMetrics {
    /// Total router execution time (microseconds)
    pub total_router_time_us: u64,
    /// Total inference time including adapters (microseconds)
    pub total_inference_time_us: u64,
    /// Router overhead as percentage
    pub overhead_pct: f64,
    /// Number of routing decisions made
    pub decision_count: u64,
    /// Average router time per decision (microseconds)
    pub avg_router_time_us: f64,
}

impl RouterOverheadMetrics {
    pub fn new() -> Self {
        Self {
            total_router_time_us: 0,
            total_inference_time_us: 0,
            overhead_pct: 0.0,
            decision_count: 0,
            avg_router_time_us: 0.0,
        }
    }

    /// Record a routing decision with timing
    pub fn record_decision(&mut self, router_time: Duration, total_time: Duration) {
        let router_us = router_time.as_micros() as u64;
        let total_us = total_time.as_micros() as u64;

        self.total_router_time_us += router_us;
        self.total_inference_time_us += total_us;
        self.decision_count += 1;

        self.avg_router_time_us = self.total_router_time_us as f64 / self.decision_count as f64;

        if self.total_inference_time_us > 0 {
            self.overhead_pct =
                (self.total_router_time_us as f64 / self.total_inference_time_us as f64) * 100.0;
        }
    }

    /// Check if overhead exceeds Ruleset #11 budget (8%)
    pub fn exceeds_budget(&self) -> bool {
        self.overhead_pct > 8.0
    }
}

impl Default for RouterOverheadMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Token throughput metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Total tokens processed
    pub total_tokens: u64,
    /// Total time spent processing (seconds)
    pub total_time_secs: f64,
    /// Tokens per second
    pub tokens_per_sec: f64,
    /// Recent sample window for instantaneous throughput
    recent_tokens: u64,
    #[serde(skip)]
    recent_start: Option<Instant>,
}

impl ThroughputMetrics {
    pub fn new() -> Self {
        Self {
            total_tokens: 0,
            total_time_secs: 0.0,
            tokens_per_sec: 0.0,
            recent_tokens: 0,
            recent_start: None,
        }
    }

    /// Record tokens processed
    pub fn record_tokens(&mut self, count: u64, duration: Duration) {
        self.total_tokens += count;
        self.total_time_secs += duration.as_secs_f64();

        if self.total_time_secs > 0.0 {
            self.tokens_per_sec = self.total_tokens as f64 / self.total_time_secs;
        }

        // Track recent throughput (last 10 seconds)
        if let Some(start) = self.recent_start {
            if start.elapsed() > Duration::from_secs(10) {
                self.recent_tokens = 0;
                self.recent_start = Some(Instant::now());
            }
        } else {
            self.recent_start = Some(Instant::now());
        }

        self.recent_tokens += count;
    }

    /// Get instantaneous throughput from recent window
    pub fn instantaneous_throughput(&self) -> f64 {
        if let Some(start) = self.recent_start {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return self.recent_tokens as f64 / elapsed;
            }
        }
        0.0
    }

    /// Check if throughput meets Ruleset #11 minimum (40 tokens/s)
    pub fn meets_budget(&self) -> bool {
        self.tokens_per_sec >= 40.0
    }
}

impl Default for ThroughputMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory pressure watermark tracking
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Low,      // < 25% headroom
    Medium,   // < 20% headroom
    High,     // < 15% headroom (policy trigger)
    Critical, // < 10% headroom
}

/// Memory pressure metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Current memory usage (bytes)
    pub used_bytes: u64,
    /// Total available memory (bytes)
    pub total_bytes: u64,
    /// Current headroom percentage
    pub headroom_pct: f64,
    /// Current pressure level
    pub pressure: MemoryPressure,
    /// Number of adapter evictions due to memory pressure
    pub eviction_count: u64,
    /// Number of K reductions due to memory pressure
    pub k_reduction_count: u64,
}

impl MemoryMetrics {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            used_bytes: 0,
            total_bytes,
            headroom_pct: 100.0,
            pressure: MemoryPressure::Normal,
            eviction_count: 0,
            k_reduction_count: 0,
        }
    }

    /// Update memory usage and recalculate pressure
    pub fn update(&mut self, used_bytes: u64) {
        self.used_bytes = used_bytes;
        let used_pct = (used_bytes as f64 / self.total_bytes as f64) * 100.0;
        self.headroom_pct = 100.0 - used_pct;

        self.pressure = match self.headroom_pct {
            h if h >= 25.0 => MemoryPressure::Normal,
            h if h >= 20.0 => MemoryPressure::Low,
            h if h >= 15.0 => MemoryPressure::Medium,
            h if h >= 10.0 => MemoryPressure::High,
            _ => MemoryPressure::Critical,
        };
    }

    /// Record an adapter eviction due to memory pressure
    pub fn record_eviction(&mut self) {
        self.eviction_count += 1;
    }

    /// Record a K reduction due to memory pressure
    pub fn record_k_reduction(&mut self) {
        self.k_reduction_count += 1;
    }

    /// Check if pressure requires action (Ruleset #12)
    pub fn requires_action(&self) -> bool {
        matches!(
            self.pressure,
            MemoryPressure::High | MemoryPressure::Critical
        )
    }
}

/// Consolidated router monitoring metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterMonitoringMetrics {
    /// Per-adapter metrics
    pub adapter_metrics: HashMap<u16, AdapterMetrics>,
    /// Router overhead metrics
    pub overhead: RouterOverheadMetrics,
    /// Token throughput metrics
    pub throughput: ThroughputMetrics,
    /// Memory pressure metrics
    pub memory: MemoryMetrics,
    /// Timestamp of last update
    pub last_updated: u64, // Unix timestamp
}

impl RouterMonitoringMetrics {
    pub fn new(total_memory_bytes: u64) -> Self {
        Self {
            adapter_metrics: HashMap::new(),
            overhead: RouterOverheadMetrics::new(),
            throughput: ThroughputMetrics::new(),
            memory: MemoryMetrics::new(total_memory_bytes),
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Get or create adapter metrics
    pub fn get_or_create_adapter(&mut self, adapter_id: u16) -> &mut AdapterMetrics {
        self.adapter_metrics
            .entry(adapter_id)
            .or_insert_with(|| AdapterMetrics::new(adapter_id))
    }

    /// Update timestamp
    pub fn touch(&mut self) {
        self.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    /// Check if any Ruleset #11 budgets are violated
    pub fn check_ruleset_11_compliance(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Check router overhead ≤ 8%
        if self.overhead.exceeds_budget() {
            violations.push(format!(
                "Router overhead {:.2}% exceeds 8% budget",
                self.overhead.overhead_pct
            ));
        }

        // Check throughput ≥ 40 tokens/s
        if !self.throughput.meets_budget() {
            violations.push(format!(
                "Throughput {:.2} tokens/s below 40 tokens/s minimum",
                self.throughput.tokens_per_sec
            ));
        }

        // Check per-adapter p95 latency < 24ms
        for (adapter_id, metrics) in &self.adapter_metrics {
            if metrics.p95_latency_us > 24_000.0 {
                violations.push(format!(
                    "Adapter {} p95 latency {:.2}ms exceeds 24ms budget",
                    adapter_id,
                    metrics.p95_latency_us / 1000.0
                ));
            }
        }

        violations
    }
}
