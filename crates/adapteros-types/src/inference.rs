//! Shared inference request/receipt types.
//!
//! These definitions are intentionally generic so that higher-level crates
//! (API, worker, UI) can specialize hash/backend representations without
//! duplicating the schema.

use crate::adapters::metadata::RoutingDeterminismMode;
use crate::coreml::CoreMLMode;
use crate::fusion::FusionInterval;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Q15 denominator for stop controller thresholds (matches router pattern)
pub const STOP_Q15_DENOM: f32 = 32767.0;

/// Exhaustive stop reason codes for inference termination.
///
/// This enum forms a complete partition of all possible termination states:
/// - Normal completion: Length, BudgetMax, CompletionConfident, RepetitionGuard, StopSequence
/// - Abnormal termination: Cancelled, SystemError
///
/// Every generation run MUST terminate with one of these codes, which is then
/// bound into the receipt for verifiable proof of why generation ended.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StopReasonCode {
    /// Generation stopped due to max_tokens limit being reached
    Length,
    /// Hard budget cap exceeded (output_max_tokens in StopPolicySpec)
    BudgetMax,
    /// EOS token probability exceeded completion_threshold_q15
    CompletionConfident,
    /// N-gram repetition detected within sliding window
    RepetitionGuard,
    /// Explicit stop sequence detected
    StopSequence,
    /// Generation was cancelled by client or system request
    Cancelled,
    /// Hardware or system error interrupted generation
    SystemError,
}

impl std::fmt::Display for StopReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length => write!(f, "LENGTH"),
            Self::BudgetMax => write!(f, "BUDGET_MAX"),
            Self::CompletionConfident => write!(f, "COMPLETION_CONFIDENT"),
            Self::RepetitionGuard => write!(f, "REPETITION_GUARD"),
            Self::StopSequence => write!(f, "STOP_SEQUENCE"),
            Self::Cancelled => write!(f, "CANCELLED"),
            Self::SystemError => write!(f, "SYSTEM_ERROR"),
        }
    }
}

impl std::str::FromStr for StopReasonCode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "LENGTH" => Ok(Self::Length),
            "BUDGET_MAX" => Ok(Self::BudgetMax),
            "COMPLETION_CONFIDENT" => Ok(Self::CompletionConfident),
            "REPETITION_GUARD" => Ok(Self::RepetitionGuard),
            "STOP_SEQUENCE" => Ok(Self::StopSequence),
            "CANCELLED" => Ok(Self::Cancelled),
            "SYSTEM_ERROR" => Ok(Self::SystemError),
            _ => Err(format!("Unknown stop reason code: {}", s)),
        }
    }
}

/// Source of inference cancellation for audit trail.
///
/// Categorizes why an inference was cancelled, enabling:
/// - Audit trail completeness (cancelled requests generate receipts)
/// - Operational metrics (track cancellation patterns)
/// - Debugging (distinguish client vs system cancellations)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancelSource {
    /// Client explicitly disconnected or cancelled request
    ClientDisconnect,
    /// Request exceeded configured timeout
    RequestTimeout,
    /// Policy enforcement triggered cancellation
    PolicyViolation,
    /// Operator or admin manually cancelled
    ManualCancel,
    /// Memory pressure or resource exhaustion
    ResourceExhaustion,
}

impl std::fmt::Display for CancelSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClientDisconnect => write!(f, "CLIENT_DISCONNECT"),
            Self::RequestTimeout => write!(f, "REQUEST_TIMEOUT"),
            Self::PolicyViolation => write!(f, "POLICY_VIOLATION"),
            Self::ManualCancel => write!(f, "MANUAL_CANCEL"),
            Self::ResourceExhaustion => write!(f, "RESOURCE_EXHAUSTION"),
        }
    }
}

impl std::str::FromStr for CancelSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CLIENT_DISCONNECT" | "client_cancelled" => Ok(Self::ClientDisconnect),
            "REQUEST_TIMEOUT" | "timeout" => Ok(Self::RequestTimeout),
            "POLICY_VIOLATION" | "policy" => Ok(Self::PolicyViolation),
            "MANUAL_CANCEL" | "manual" => Ok(Self::ManualCancel),
            "RESOURCE_EXHAUSTION" | "resource" => Ok(Self::ResourceExhaustion),
            _ => Ok(Self::ClientDisconnect), // Default for unknown reasons
        }
    }
}

/// State captured when inference is cancelled.
///
/// This struct carries the partial output and cancellation metadata,
/// allowing the caller to build an error receipt before returning to the client.
#[derive(Debug, Clone)]
pub struct CancellationState {
    /// Trace ID for this inference request
    pub trace_id: String,
    /// Number of tokens generated before cancellation
    pub tokens_generated: usize,
    /// Token IDs generated before cancellation (if available)
    pub partial_token_ids: Vec<u32>,
    /// Source/reason for the cancellation
    pub source: CancelSource,
    /// Human-readable reason string from cancel token
    pub reason_message: String,
    /// Timestamp when cancellation was detected
    pub cancelled_at: std::time::Instant,
}

impl CancellationState {
    /// Create a new cancellation state.
    pub fn new(
        trace_id: String,
        tokens_generated: usize,
        source: CancelSource,
        reason_message: String,
    ) -> Self {
        Self {
            trace_id,
            tokens_generated,
            partial_token_ids: Vec::new(),
            source,
            reason_message,
            cancelled_at: std::time::Instant::now(),
        }
    }

    /// Add partial token IDs generated before cancellation.
    pub fn with_partial_tokens(mut self, token_ids: Vec<u32>) -> Self {
        self.partial_token_ids = token_ids;
        self
    }
}

/// Inference request (API surface)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(
    rename_all = "snake_case",
    deny_unknown_fields,
    bound(
        deserialize = "Backend: DeserializeOwned, Interval: DeserializeOwned, StopPolicy: DeserializeOwned"
    )
)]
pub struct InferRequest<Backend = String, Interval = FusionInterval, StopPolicy = serde_json::Value>
{
    /// Raw prompt text or chat payload.
    pub prompt: String,
    /// Explicit model identifier to target (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Per-request override for router determinism (e.g., "deterministic", "adaptive")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Adapter stack identifier to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Optional domain hint to bias package/adapters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Fusion interval policy (per_request|per_segment|per_token)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub fusion_interval: Option<Interval>,
    /// Maximum number of tokens to generate (fallbacks to manifest defaults).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Sampling temperature override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-K sampling cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Random seed for deterministic sampling.
    ///
    /// Required when the effective determinism mode is strict and tenant policy
    /// sets `determinism.require_seed=true`. Missing or invalid seeds are rejected
    /// as determinism violations (no partial result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Explicit backend preference (auto|coreml|mlx|metal|cpu)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    /// CoreML mode for backend selection (coreml_strict|coreml_preferred|backend_auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,
    /// Enable server-sent event streaming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Require evidence payloads in receipts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_evidence: Option<bool>,
    /// Enable reasoning-aware routing and hot-swaps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_mode: Option<bool>,
    /// Explicit adapter list to use for inference (legacy field)
    ///
    /// Historically used as a "stack" placeholder, this now represents a concrete
    /// adapter list when provided. Prefer `stack_id` for named stacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use (alternative to adapter_stack)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Effective adapter set computed by the control plane (debug/audit only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Chat session ID for trace linkage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Tenant ID (usually extracted from JWT claims, but can be explicit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Enable RAG context retrieval for this inference request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_enabled: Option<bool>,
    /// Collection ID for scoped RAG retrieval (requires rag_enabled = true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Dataset version ID for deterministic dataset pinning
    ///
    /// When provided, the inference is scoped to this exact dataset version,
    /// enabling deterministic replay and explicit dataset pinning in receipts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    /// Stop policy specification for deterministic stop behavior
    ///
    /// If not provided, a default policy is constructed from max_tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<StopPolicy>,
    // =========================================================================
    // Expected Digests for Verification (PRD: Receipt Completeness v4)
    // =========================================================================
    /// Expected Metal library (metallib) digest for verification.
    ///
    /// If provided, the server verifies the active metallib matches this digest.
    /// Mismatches result in a determinism violation error.
    /// Format: 64-character hex BLAKE3 digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_metallib_digest_b3_hex: Option<String>,
    /// Expected kernel digest for verification.
    ///
    /// If provided, the server verifies the compiled kernel matches this digest.
    /// Format: 64-character hex BLAKE3 digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_kernel_digest_b3_hex: Option<String>,
}

/// Verifiable run receipt (hash chain over per-token decisions)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RunReceipt<Hash = String> {
    /// Unique trace identifier for this run.
    pub trace_id: String,
    /// Head hash for the run's trace chain.
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub run_head_hash: Hash,
    /// Digest over output tokens.
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub output_digest: Hash,
    /// Digest over receipt contents.
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub receipt_digest: Hash,
    /// Optional signature covering the receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Optional attestation payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
    /// Total logical prompt tokens before cache reuse
    pub logical_prompt_tokens: u32,
    /// Tokens satisfied by prefix cache reuse
    pub prefix_cached_token_count: u32,
    /// Billed input tokens (logical - cached, floored at 0)
    pub billed_input_tokens: u32,
    /// Tokens produced logically (excludes eos)
    pub logical_output_tokens: u32,
    /// Billed output tokens (v1 = logical output tokens)
    pub billed_output_tokens: u32,
    // =========================================================================
    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    // =========================================================================
    /// Stop reason code explaining why generation terminated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used for this inference
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub stop_policy_digest_b3: Option<Hash>,
    // =========================================================================
    // KV Quota/Residency fields for evidence (PRD: KvResidencyAndQuotas v1)
    // =========================================================================
    /// Tenant's allocated KV cache quota in bytes
    #[serde(default)]
    pub tenant_kv_quota_bytes: u64,
    /// Actual KV cache bytes used for this inference
    #[serde(default)]
    pub tenant_kv_bytes_used: u64,
    /// Number of KV cache evictions that occurred during inference
    #[serde(default)]
    pub kv_evictions: u32,
    /// ID of the residency policy governing KV cache management
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_residency_policy_id: Option<String>,
    /// Whether KV quota enforcement was active for this inference
    #[serde(default)]
    pub kv_quota_enforced: bool,
    // =========================================================================
    // Prefix KV Cache fields (PRD: PrefixKvCache v1)
    // =========================================================================
    /// Cryptographic key for the prefix KV cache entry used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub prefix_kv_key_b3: Option<Hash>,
    /// Whether the prefix KV cache was hit (true) or missed (false)
    #[serde(default)]
    pub prefix_cache_hit: bool,
    /// Bytes of cached KV tensors for the prefix
    #[serde(default)]
    pub prefix_kv_bytes: u64,
    // =========================================================================
    // Model Cache Identity v2 (PRD-06: ModelCacheIdentity v2 Canonicalization)
    // =========================================================================
    /// BLAKE3 digest of ModelCacheIdentityV2 canonical bytes.
    /// Binds the receipt to the exact kernel/quant/fusion/tokenizer/tenant/worker combination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub model_cache_identity_v2_digest_b3: Option<Hash>,
    // =========================================================================
    // Cross-Run Lineage (Patent 3535886.0002 Claims 7-8: Temporal Ordering)
    // =========================================================================
    /// Previous receipt digest for cross-run lineage.
    ///
    /// Links this receipt to the prior inference in the same session/tenant,
    /// forming a cryptographic chain that proves temporal ordering of inferences.
    /// None for the first inference in a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub previous_receipt_digest: Option<Hash>,
    /// Session sequence number for temporal ordering.
    ///
    /// Monotonically increasing counter within a session, starting at 0.
    /// Combined with previous_receipt_digest, enables verification of
    /// inference ordering without access to the full trace.
    #[serde(default)]
    pub session_sequence: u64,
}
