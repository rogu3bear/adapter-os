//! Enhanced telemetry events for router and policy decisions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Router decision event with feature importance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDecisionEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Selected adapter IDs
    pub adapter_ids: Vec<u16>,
    /// Quantized gate values (Q15)
    pub gates_q15: Vec<i16>,
    /// Number of active adapters (K)
    pub k_active: usize,
    /// Attention entropy value (if computed)
    pub entropy: Option<f32>,
    /// Token index in sequence
    pub token_index: usize,
    /// Feature importance scores
    pub feature_importance: HashMap<String, f32>,
    /// Router execution time (microseconds)
    pub router_time_us: u64,
    /// Whether entropy floor was applied
    pub entropy_floor_applied: bool,
}

impl RouterDecisionEvent {
    pub fn new(
        adapter_ids: Vec<u16>,
        gates_q15: Vec<i16>,
        k_active: usize,
        token_index: usize,
        router_time_us: u64,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            adapter_ids,
            gates_q15,
            k_active,
            entropy: None,
            token_index,
            feature_importance: HashMap::new(),
            router_time_us,
            entropy_floor_applied: false,
        }
    }

    pub fn with_entropy(mut self, entropy: f32) -> Self {
        self.entropy = Some(entropy);
        self
    }

    pub fn with_feature_importance(mut self, importance: HashMap<String, f32>) -> Self {
        self.feature_importance = importance;
        self
    }

    pub fn with_entropy_floor(mut self, applied: bool) -> Self {
        self.entropy_floor_applied = applied;
        self
    }
}

/// Abstain decision event (Ruleset #5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstainEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Reason for abstaining
    pub reason: AbstainReason,
    /// Confidence score that triggered abstain
    pub confidence: f32,
    /// Entropy value (if applicable)
    pub entropy: Option<f32>,
    /// Missing evidence fields
    pub missing_fields: Vec<String>,
    /// Number of evidence spans found
    pub evidence_span_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AbstainReason {
    /// Low confidence below threshold
    LowConfidence { threshold: f32 },
    /// High entropy indicating uncertainty
    HighEntropy { threshold: f32 },
    /// Insufficient evidence spans
    InsufficientEvidence { min_required: usize },
    /// Missing required fields
    MissingFields,
}

impl AbstainEvent {
    pub fn low_confidence(confidence: f32, threshold: f32) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            reason: AbstainReason::LowConfidence { threshold },
            confidence,
            entropy: None,
            missing_fields: Vec::new(),
            evidence_span_count: 0,
        }
    }

    pub fn high_entropy(entropy: f32, threshold: f32) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            reason: AbstainReason::HighEntropy { threshold },
            confidence: 0.0,
            entropy: Some(entropy),
            missing_fields: Vec::new(),
            evidence_span_count: 0,
        }
    }

    pub fn insufficient_evidence(span_count: usize, min_required: usize) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            reason: AbstainReason::InsufficientEvidence { min_required },
            confidence: 0.0,
            entropy: None,
            missing_fields: Vec::new(),
            evidence_span_count: span_count,
        }
    }

    pub fn missing_fields(fields: Vec<String>) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            reason: AbstainReason::MissingFields,
            confidence: 0.0,
            entropy: None,
            missing_fields: fields,
            evidence_span_count: 0,
        }
    }
}

/// Adapter eviction event (Ruleset #12 - Memory pressure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEvictionEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Evicted adapter ID
    pub adapter_id: u16,
    /// Reason for eviction
    pub reason: EvictionReason,
    /// Memory headroom percentage before eviction
    pub headroom_pct_before: f64,
    /// Memory headroom percentage after eviction
    pub headroom_pct_after: f64,
    /// Memory freed (bytes)
    pub memory_freed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EvictionReason {
    /// Memory pressure (Ruleset #12)
    MemoryPressure { current_headroom_pct: f64 },
    /// LRU eviction (cold adapter)
    LruCold,
    /// Time-to-live expired (ephemeral adapter)
    TtlExpired,
    /// Quality below threshold (Ruleset #19)
    LowQuality { quality_delta: f32 },
}

impl AdapterEvictionEvent {
    pub fn new(
        adapter_id: u16,
        reason: EvictionReason,
        headroom_before: f64,
        headroom_after: f64,
        memory_freed: u64,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            adapter_id,
            reason,
            headroom_pct_before: headroom_before,
            headroom_pct_after: headroom_after,
            memory_freed_bytes: memory_freed,
        }
    }
}

/// K reduction event (Ruleset #12 - before adapter eviction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Previous K value
    pub k_before: usize,
    /// New K value
    pub k_after: usize,
    /// Memory headroom percentage when reduction occurred
    pub headroom_pct: f64,
    /// Whether this prevented hot adapter eviction
    pub prevented_eviction: bool,
}

impl KReductionEvent {
    pub fn new(k_before: usize, k_after: usize, headroom_pct: f64) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            k_before,
            k_after,
            headroom_pct,
            prevented_eviction: false,
        }
    }

    pub fn with_prevention(mut self, prevented: bool) -> Self {
        self.prevented_eviction = prevented;
        self
    }
}

/// Calibration performance metrics event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationMetricsEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Accuracy on validation set
    pub accuracy: f32,
    /// Precision
    pub precision: f32,
    /// Recall
    pub recall: f32,
    /// F1 score
    pub f1_score: f32,
    /// Mean Reciprocal Rank
    pub mrr: f32,
    /// Training dataset size
    pub train_size: usize,
    /// Validation dataset size
    pub val_size: usize,
    /// Optimization method used
    pub optimization_method: String,
    /// Training time (seconds)
    pub training_time_secs: f64,
}

impl CalibrationMetricsEvent {
    pub fn new(
        accuracy: f32,
        precision: f32,
        recall: f32,
        f1_score: f32,
        mrr: f32,
        train_size: usize,
        val_size: usize,
        optimization_method: String,
        training_time_secs: f64,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            accuracy,
            precision,
            recall,
            f1_score,
            mrr,
            train_size,
            val_size,
            optimization_method,
            training_time_secs,
        }
    }
}

/// Ruleset #11 budget violation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBudgetViolationEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Violation type
    pub violation: BudgetViolation,
    /// Current value
    pub current_value: f64,
    /// Budget threshold
    pub budget_threshold: f64,
    /// Severity (1.0 = at threshold, > 1.0 = exceeded)
    pub severity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BudgetViolation {
    /// p95 latency exceeded 24ms
    P95Latency { adapter_id: Option<u16> },
    /// Router overhead exceeded 8%
    RouterOverhead,
    /// Throughput below 40 tokens/s
    Throughput,
}

impl PerformanceBudgetViolationEvent {
    pub fn p95_latency(current_ms: f64, adapter_id: Option<u16>) -> Self {
        let budget_ms = 24.0;
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            violation: BudgetViolation::P95Latency { adapter_id },
            current_value: current_ms,
            budget_threshold: budget_ms,
            severity: current_ms / budget_ms,
        }
    }

    pub fn router_overhead(current_pct: f64) -> Self {
        let budget_pct = 8.0;
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            violation: BudgetViolation::RouterOverhead,
            current_value: current_pct,
            budget_threshold: budget_pct,
            severity: current_pct / budget_pct,
        }
    }

    pub fn throughput(current_tps: f64) -> Self {
        let budget_tps = 40.0;
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            violation: BudgetViolation::Throughput,
            current_value: current_tps,
            budget_threshold: budget_tps,
            severity: budget_tps / current_tps, // Inverted for minimum threshold
        }
    }
}

