//! Enhanced telemetry events for router and policy decisions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Stack ID for telemetry correlation
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default)]
    pub stack_version: Option<i64>,

    /// Determinism mode active for the request (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,

    /// Optional per-adapter metadata (backend, scope_path, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_metadata: Option<HashMap<String, AdapterInferenceMetadata>>,
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
            determinism_mode: None,
            adapter_metadata: None,
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

    /// Attach stack metadata for telemetry correlation
    pub fn with_stack_metadata(
        mut self,
        stack_id: Option<String>,
        stack_version: Option<i64>,
    ) -> Self {
        self.stack_id = stack_id;
        self.stack_version = stack_version;
        self
    }

    /// Attach determinism mode for this inference (e.g., "strict", "debug")
    pub fn with_determinism_mode(mut self, determinism_mode: Option<String>) -> Self {
        self.determinism_mode = determinism_mode;
        self
    }

    /// Attach per-adapter metadata such as backend and scope_path
    pub fn with_adapter_metadata(
        mut self,
        adapter_metadata: HashMap<String, AdapterInferenceMetadata>,
    ) -> Self {
        self.adapter_metadata = Some(adapter_metadata);
        self
    }
}

/// Metadata captured for each adapter involved in an inference
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterInferenceMetadata {
    /// Backend tag used to serve the adapter (canonical/metal/mlx/coreml)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Scope path derived from domain/group/scope/operation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    /// Selected segment identifier (if segmented adapter)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<u32>,
    /// Effective LoRA strength used for this adapter (0.0-1.0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
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
    /// Stack ID for telemetry correlation
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
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

/// K reduction request initiation event (Ruleset #12 - memory pressure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionRequestEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Unique request ID for correlation
    pub request_id: String,
    /// Current K value before reduction
    pub k_current: usize,
    /// Proposed target K value
    pub k_target: usize,
    /// Memory pressure level (0-1, 1=critical)
    pub pressure_level: f32,
    /// Bytes needed to be freed
    pub bytes_to_free: u64,
    /// Current memory headroom percentage
    pub headroom_pct: f32,
    /// Reason for reduction request
    pub reason: String,
    /// Whether request is valid (target < current && target >= min_k)
    pub is_valid: bool,
}

impl KReductionRequestEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: String,
        k_current: usize,
        k_target: usize,
        pressure_level: f32,
        bytes_to_free: u64,
        headroom_pct: f32,
        reason: String,
        is_valid: bool,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            request_id,
            k_current,
            k_target,
            pressure_level,
            bytes_to_free,
            headroom_pct,
            reason,
            is_valid,
        }
    }
}

/// K reduction evaluation event (Ruleset #12 - lifecycle manager evaluation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionEvaluationEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Correlation ID linking to request event
    pub request_id: String,
    /// Evaluation duration (microseconds)
    pub evaluation_duration_us: u64,
    /// Whether evaluation resulted in approval
    pub approved: bool,
    /// Number of adapters selected for unload
    pub adapters_to_unload_count: usize,
    /// Estimated memory that will be freed
    pub estimated_freed: u64,
    /// Reason for approval/rejection
    pub reason: String,
    /// Lock acquisition time (microseconds) for deadlock detection
    pub lock_acquisition_time_us: u64,
    /// Timeout occurred during evaluation
    pub timeout_occurred: bool,
}

impl KReductionEvaluationEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: String,
        evaluation_duration_us: u64,
        approved: bool,
        adapters_to_unload_count: usize,
        estimated_freed: u64,
        reason: String,
        lock_acquisition_time_us: u64,
        timeout_occurred: bool,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            request_id,
            evaluation_duration_us,
            approved,
            adapters_to_unload_count,
            estimated_freed,
            reason,
            lock_acquisition_time_us,
            timeout_occurred,
        }
    }
}

/// K reduction execution event (Ruleset #12 - adapter unload execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionExecutionEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Correlation ID linking to request/evaluation events
    pub request_id: String,
    /// Execution duration (microseconds)
    pub execution_duration_us: u64,
    /// Whether execution succeeded
    pub success: bool,
    /// Number of adapters actually unloaded
    pub adapters_unloaded_count: usize,
    /// Memory actually freed (bytes)
    pub actual_memory_freed: u64,
    /// Error message if execution failed
    pub error: Option<String>,
    /// New K value after execution
    pub k_final: usize,
    /// Timeout occurred during execution
    pub timeout_occurred: bool,
}

impl KReductionExecutionEvent {
    pub fn new(
        request_id: String,
        execution_duration_us: u64,
        success: bool,
        adapters_unloaded_count: usize,
        actual_memory_freed: u64,
        k_final: usize,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            request_id,
            execution_duration_us,
            success,
            adapters_unloaded_count,
            actual_memory_freed,
            error: None,
            k_final,
            timeout_occurred: false,
        }
    }

    pub fn with_error(mut self, error: String, timeout_occurred: bool) -> Self {
        self.success = false;
        self.error = Some(error);
        self.timeout_occurred = timeout_occurred;
        self
    }
}

/// K reduction completion event (Ruleset #12 - post-execution analysis)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionCompletionEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Correlation ID linking all K reduction events
    pub request_id: String,
    /// Total duration from request to completion (microseconds)
    pub total_duration_us: u64,
    /// Whether the entire operation succeeded
    pub success: bool,
    /// K value before the operation
    pub k_before: usize,
    /// K value after the operation
    pub k_after: usize,
    /// Memory headroom after completion
    pub headroom_after_pct: f32,
    /// Whether eviction of hot adapters was prevented
    pub prevented_hot_eviction: bool,
    /// Deadlock was detected and resolved
    pub deadlock_detected: bool,
    /// Operation was aborted due to timeout
    pub timeout_abort: bool,
}

impl KReductionCompletionEvent {
    pub fn new(
        request_id: String,
        total_duration_us: u64,
        success: bool,
        k_before: usize,
        k_after: usize,
        headroom_after_pct: f32,
        prevented_hot_eviction: bool,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            request_id,
            total_duration_us,
            success,
            k_before,
            k_after,
            headroom_after_pct,
            prevented_hot_eviction,
            deadlock_detected: false,
            timeout_abort: false,
        }
    }

    pub fn with_deadlock_info(mut self, deadlock_detected: bool, timeout_abort: bool) -> Self {
        self.deadlock_detected = deadlock_detected;
        self.timeout_abort = timeout_abort;
        self
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
    #[allow(clippy::too_many_arguments)]
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

/// Residency probe result for audit trail
///
/// Tracks base model residency during adapter hot-swap cycles.
/// Emitted by the hardware residency harness and admin debug endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidencyProbeEvent {
    /// Timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Base model identifier
    pub base_model_id: String,
    /// BLAKE3 hash of the model manifest
    pub manifest_hash: String,
    /// Number of adapters involved in the churn test
    pub adapter_count: u32,
    /// Number of load/unload cycles completed
    pub cycle_count: u32,
    /// Baseline RSS before warmup (bytes)
    pub baseline_rss_bytes: u64,
    /// Peak RSS during test (bytes)
    pub peak_rss_bytes: u64,
    /// Final RSS after test (bytes)
    pub final_rss_bytes: u64,
    /// RSS growth (final - baseline, can be negative)
    pub rss_growth_bytes: i64,
    /// Load latency p95 (microseconds)
    pub load_latency_p95_us: u64,
    /// Probe result status
    pub result: ResidencyProbeResult,
    /// Determinism mode active during probe (if any)
    pub determinism_mode: Option<String>,
    /// Backend used (e.g., "coreml", "metal", "mlx")
    pub backend: String,
    /// Cache hits during test
    pub cache_hits: u64,
    /// Cache misses during test
    pub cache_misses: u64,
    /// Evictions blocked due to pinned entries
    pub eviction_blocked_pinned: u64,
}

/// Residency probe result status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResidencyProbeResult {
    /// Probe passed - base model remained resident
    Ok,
    /// Probe passed with warnings
    Degraded { reason: String },
    /// Probe failed - base model was evicted or unpinned
    Failed { reason: String },
}

impl ResidencyProbeEvent {
    /// Create a successful residency probe event
    #[allow(clippy::too_many_arguments)]
    pub fn ok(
        base_model_id: String,
        manifest_hash: String,
        adapter_count: u32,
        cycle_count: u32,
        baseline_rss_bytes: u64,
        peak_rss_bytes: u64,
        final_rss_bytes: u64,
        load_latency_p95_us: u64,
        backend: String,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            base_model_id,
            manifest_hash,
            adapter_count,
            cycle_count,
            baseline_rss_bytes,
            peak_rss_bytes,
            final_rss_bytes,
            rss_growth_bytes: final_rss_bytes as i64 - baseline_rss_bytes as i64,
            load_latency_p95_us,
            result: ResidencyProbeResult::Ok,
            determinism_mode: None,
            backend,
            cache_hits: 0,
            cache_misses: 0,
            eviction_blocked_pinned: 0,
        }
    }

    /// Create a degraded residency probe event
    #[allow(clippy::too_many_arguments)]
    pub fn degraded(
        base_model_id: String,
        manifest_hash: String,
        adapter_count: u32,
        cycle_count: u32,
        baseline_rss_bytes: u64,
        peak_rss_bytes: u64,
        final_rss_bytes: u64,
        load_latency_p95_us: u64,
        backend: String,
        reason: String,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            base_model_id,
            manifest_hash,
            adapter_count,
            cycle_count,
            baseline_rss_bytes,
            peak_rss_bytes,
            final_rss_bytes,
            rss_growth_bytes: final_rss_bytes as i64 - baseline_rss_bytes as i64,
            load_latency_p95_us,
            result: ResidencyProbeResult::Degraded { reason },
            determinism_mode: None,
            backend,
            cache_hits: 0,
            cache_misses: 0,
            eviction_blocked_pinned: 0,
        }
    }

    /// Create a failed residency probe event
    #[allow(clippy::too_many_arguments)]
    pub fn failed(
        base_model_id: String,
        manifest_hash: String,
        adapter_count: u32,
        cycle_count: u32,
        baseline_rss_bytes: u64,
        peak_rss_bytes: u64,
        final_rss_bytes: u64,
        load_latency_p95_us: u64,
        backend: String,
        reason: String,
    ) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            base_model_id,
            manifest_hash,
            adapter_count,
            cycle_count,
            baseline_rss_bytes,
            peak_rss_bytes,
            final_rss_bytes,
            rss_growth_bytes: final_rss_bytes as i64 - baseline_rss_bytes as i64,
            load_latency_p95_us,
            result: ResidencyProbeResult::Failed { reason },
            determinism_mode: None,
            backend,
            cache_hits: 0,
            cache_misses: 0,
            eviction_blocked_pinned: 0,
        }
    }

    /// Add determinism mode to the event
    pub fn with_determinism_mode(mut self, mode: Option<String>) -> Self {
        self.determinism_mode = mode;
        self
    }

    /// Add cache statistics to the event
    pub fn with_cache_stats(
        mut self,
        hits: u64,
        misses: u64,
        eviction_blocked_pinned: u64,
    ) -> Self {
        self.cache_hits = hits;
        self.cache_misses = misses;
        self.eviction_blocked_pinned = eviction_blocked_pinned;
        self
    }
}
