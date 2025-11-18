//! Enhanced telemetry events for router and policy decisions

use serde::{Deserialize, Serialize};

/// RNG state snapshot for deterministic replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RngSnapshot {
    /// Global nonce value when snapshot was taken
    pub global_nonce: u64,
    /// Label of the RNG instance
    pub label: String,
    /// Step count
    pub step_count: u64,
}

/// Inference event with RNG tracking (Ruleset #2 - Determinism)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Session ID
    pub session_id: String,
    /// Request ID
    pub request_id: String,
    /// Input token count
    pub input_tokens: usize,
    /// Output token count
    pub output_tokens: usize,
    /// Inference duration (microseconds)
    pub duration_us: u64,
    /// Success flag
    pub success: bool,
    /// Error message (if any)
    pub error: Option<String>,
    /// Current RNG nonce value
    pub rng_nonce: u64,
    /// RNG state snapshot for replay
    pub rng_snapshot: Option<RngSnapshot>,
    /// Router decisions made during inference
    pub router_decisions: Vec<String>,
    /// Memory usage (bytes)
    pub memory_used_bytes: u64,
    /// Model ID
    pub model_id: String,
    /// Adapter IDs used
    pub adapter_ids: Vec<u16>,
    /// Global nonce at inference start
    #[serde(default)]
    pub global_nonce: u64,
    /// Seed label used for RNG derivation
    #[serde(default)]
    pub seed_label: String,
    /// BLAKE3 checksum of seed for validation
    #[serde(default)]
    pub seed_checksum: String,
    /// RNG draw counter at inference end
    #[serde(default)]
    pub rng_counter: u64,
    /// Worker ID for cross-worker verification
    #[serde(default)]
    pub worker_id: u32,
    /// Stack ID for telemetry correlation (PRD-03)
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation (PRD-03)
    #[serde(default)]
    pub stack_version: Option<i64>,
}

impl InferenceEvent {
    pub fn new(
        session_id: String,
        request_id: String,
        input_tokens: usize,
        model_id: String,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            session_id,
            request_id,
            input_tokens,
            output_tokens: 0,
            duration_us: 0,
            success: true,
            error: None,
            rng_nonce: 0,
            rng_snapshot: None,
            router_decisions: Vec::new(),
            memory_used_bytes: 0,
            model_id,
            adapter_ids: Vec::new(),
            global_nonce: 0,
            seed_label: String::new(),
            seed_checksum: String::new(),
            rng_counter: 0,
            worker_id: 0,
            stack_id: None,
            stack_version: None,
        }
    }

    pub fn with_rng_metadata(
        mut self,
        global_nonce: u64,
        seed_label: String,
        seed_checksum: String,
        rng_counter: u64,
        worker_id: u32,
    ) -> Self {
        self.global_nonce = global_nonce;
        self.seed_label = seed_label;
        self.seed_checksum = seed_checksum;
        self.rng_counter = rng_counter;
        self.worker_id = worker_id;
        self
    }

    pub fn with_rng_snapshot(mut self, snapshot: RngSnapshot) -> Self {
        self.rng_nonce = snapshot.global_nonce;
        self.rng_snapshot = Some(snapshot);
        self
    }

    pub fn with_output(mut self, output_tokens: usize, duration_us: u64) -> Self {
        self.output_tokens = output_tokens;
        self.duration_us = duration_us;
        self
    }

    pub fn with_adapters(mut self, adapter_ids: Vec<u16>) -> Self {
        self.adapter_ids = adapter_ids;
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.success = false;
        self.error = Some(error);
        self
    }

    /// Attach stack metadata for telemetry correlation (PRD-03)
    pub fn with_stack_metadata(mut self, stack_id: Option<String>, stack_version: Option<i64>) -> Self {
        self.stack_id = stack_id;
        self.stack_version = stack_version;
        self
    }
}

/// Canonical `router.decision` payload that must remain frozen (tests assert the exact shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDecisionEvent {
    /// Zero-based step/token index for the decision
    pub step: usize,
    /// Token ID that guided the decision (context token or candidate)
    pub input_token_id: Option<u32>,
    /// Candidate adapters with raw scores and quantized gates
    pub candidate_adapters: Vec<RouterCandidate>,
    /// Shannon entropy computed from the gate distribution
    pub entropy: f32,
    /// Temperature (tau) used for the softmax
    pub tau: f32,
    /// Entropy floor (epsilon) enforced during normalization
    pub entropy_floor: f32,
    /// Optional hash of the active adapter stack
    pub stack_hash: Option<String>,
    /// Stack ID for telemetry correlation (PRD-03)
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation (PRD-03)
    #[serde(default)]
    pub stack_version: Option<i64>,
}

/// Candidate adapter entry inside the canonical router decision stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterCandidate {
    /// Adapter index used in kernel routing (zero-based)
    pub adapter_idx: u16,
    /// Raw score before softmax/quantization
    pub raw_score: f32,
    /// Quantized gate value (Q15)
    pub gate_q15: i16,
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

/// Policy hash validation event
///
/// Logged at 100% sampling (policy violations per Telemetry Ruleset #9).
/// Tracks runtime policy pack hash validation to detect mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyHashValidationEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Policy pack identifier
    pub policy_pack_id: String,
    /// Previously known hash (baseline)
    pub prev_hash: String,
    /// Currently computed hash
    pub current_hash: String,
    /// Validation status
    pub status: ValidationStatus,
    /// Control Plane ID (optional)
    pub cpid: Option<String>,
}

/// Policy hash validation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    /// Hash matches baseline
    Valid,
    /// Hash mismatch detected
    Mismatch,
    /// No baseline hash found
    Missing,
}

impl PolicyHashValidationEvent {
    pub fn valid(policy_pack_id: String, hash: String, cpid: Option<String>) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            policy_pack_id,
            prev_hash: hash.clone(),
            current_hash: hash,
            status: ValidationStatus::Valid,
            cpid,
        }
    }

    pub fn mismatch(
        policy_pack_id: String,
        prev_hash: String,
        current_hash: String,
        cpid: Option<String>,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            policy_pack_id,
            prev_hash,
            current_hash,
            status: ValidationStatus::Mismatch,
            cpid,
        }
    }

    pub fn missing(policy_pack_id: String, current_hash: String, cpid: Option<String>) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            policy_pack_id,
            prev_hash: String::new(),
            current_hash,
            status: ValidationStatus::Missing,
            cpid,
        }
    }
}
