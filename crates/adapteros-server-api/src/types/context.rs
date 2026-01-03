//! Execution context types for inference and replay.

use adapteros_api_types::inference::RunReceipt;
use adapteros_api_types::ReplayGuarantee;
use adapteros_core::{BackendKind, SeedMode};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::response::TokenUsage;
use super::sampling::PlacementTraceEntry;

/// Context for replay execution through InferenceCore
///
/// Contains the constraints and metadata needed to execute a deterministic
/// replay of a previous inference operation.
#[derive(Debug, Clone)]
pub struct ReplayContext {
    /// Original inference ID being replayed (for correlation/audit)
    pub original_inference_id: String,
    /// Required manifest hash - worker must match this exactly
    pub required_manifest_hash: String,
    /// Required backend (CoreML, MLX, Metal) - worker must be compatible
    pub required_backend: String,
    /// If true, don't capture new replay metadata for this execution
    /// (prevents recursive replay metadata creation)
    pub skip_metadata_capture: bool,
    /// Original policy ID that was in effect during the original inference
    pub original_policy_id: Option<String>,
    /// Original policy version that was in effect
    pub original_policy_version: Option<i64>,
}

/// Internal unified inference request used by all handlers.
///
/// This is the canonical representation that flows through `route_and_infer()`.
/// All HTTP handlers map their external request types into this internal model.
#[derive(Debug, Clone)]
pub struct InferenceRequestInternal {
    // === Core Fields ===
    /// Unique request ID for tracing and correlation
    pub request_id: String,
    /// Control plane ID (tenant identifier)
    pub cpid: String,
    /// Input prompt text
    pub prompt: String,
    /// Canonical execution envelope for determinism/audit
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// Enable reasoning-aware routing and hot-swaps
    pub reasoning_mode: bool,
    /// Admin override flag to bypass cluster routing restrictions
    pub admin_override: bool,

    // === Delivery Mode ===
    /// Whether to stream tokens via SSE
    pub stream: bool,
    /// Require token-by-token step inference (no bulk-only backends)
    pub require_step: bool,
    /// Require deterministic-capable backend
    pub require_determinism: bool,
    /// Allow backend fallback when requested backend is unavailable
    pub allow_fallback: bool,
    /// Batch item ID (for batch requests only)
    pub batch_item_id: Option<String>,

    // === RAG Options ===
    /// Enable RAG context retrieval
    pub rag_enabled: bool,
    /// Collection ID for scoped RAG retrieval
    pub rag_collection_id: Option<String>,
    /// Dataset version ID for deterministic dataset pinning
    pub dataset_version_id: Option<String>,

    // === Adapter Selection ===
    /// Adapter stack to use for inference
    ///
    /// Legacy: this is an explicit list of adapter IDs, **not** a stack_id alias.
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use (alternative to adapter_stack)
    ///
    /// Explicit adapter IDs for this request. Takes precedence over stack_id.
    pub adapters: Option<Vec<String>>,
    /// Adapter stack identifier (preferred over adapter_stack list)
    ///
    /// References a stack in the DB; resolved to adapter IDs before sending to the worker.
    pub stack_id: Option<String>,
    /// Optional domain hint for routing/package selection
    pub domain_hint: Option<String>,
    /// Stack version for telemetry/audit (populated when stack_id resolves)
    pub stack_version: Option<i64>,
    /// Determinism mode configured on the resolved stack (if any)
    pub stack_determinism_mode: Option<String>,
    /// Routing determinism mode configured on the resolved stack (if any)
    pub stack_routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Effective adapter IDs after control plane resolution
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Per-adapter strength overrides (session/request scoped)
    ///
    /// Values multiply the adapter's configured lora_strength. Defaults to 1.0.
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Resolved determinism mode applied to this request
    pub determinism_mode: Option<String>,
    /// Routing determinism mode applied to this request (deterministic/adaptive)
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Seed mode requested for per-request RNG derivation
    pub seed_mode: Option<SeedMode>,
    /// Request-scoped seed derived by control plane
    pub request_seed: Option<[u8; 32]>,
    /// Backend profile selected for execution
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode selected for this request
    pub coreml_mode: Option<super::CoreMLMode>,

    // === Sampling Parameters ===
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Sampling temperature
    pub temperature: f32,
    /// Top-K sampling
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling
    pub top_p: Option<f32>,
    /// Random seed for reproducibility (PRD-02: deterministic sampling)
    pub seed: Option<u64>,
    /// Router seed for audit purposes (PRD-02: replay)
    ///
    /// **Note:** The router uses a deterministic algorithm (sorted by score,
    /// then by index for tie-breaking). This seed is stored for audit trail
    /// purposes but does NOT currently affect routing decisions. Replays
    /// produce identical routing given identical inputs.
    pub router_seed: Option<String>,

    // === Evidence & Session ===
    /// Require evidence recording
    pub require_evidence: bool,
    /// Chat session ID for trace linkage
    pub session_id: Option<String>,
    /// Pinned adapter IDs for this inference (session-level preference, CHAT-PIN-02)
    ///
    /// These adapters receive PINNED_BOOST added to their priors during routing,
    /// making them more likely to be selected while still allowing non-pinned
    /// adapters to win with sufficiently high feature scores. When an effective
    /// adapter set is present, pins must also be members of that set.
    pub pinned_adapter_ids: Option<Vec<String>>,
    /// BLAKE3 hash of sorted message IDs for multi-turn context verification
    ///
    /// When a session_id is provided and multi-turn context is built, this hash
    /// enables deterministic replay verification. Stored in replay_metadata.
    pub chat_context_hash: Option<String>,
    /// User claims for policy enforcement
    pub claims: Option<crate::auth::Claims>,
    /// BLAKE3 digest of policy decisions applied during request processing
    ///
    /// This captures the policy enforcement state for deterministic replay.
    /// Computed from the sorted policy_pack_ids, hooks, and decisions.
    pub policy_mask_digest_b3: Option<[u8; 32]>,

    // === Model Selection ===
    /// Model identifier (if specific model requested)
    pub model: Option<String>,

    // === Stop Controller ===
    /// Stop policy specification (PRD: Hard Deterministic Stop Controller)
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,

    // === Timing ===
    /// Request creation timestamp
    pub created_at: std::time::Instant,
    /// Optional auth token used to reach the worker (ApiKey)
    pub worker_auth_token: Option<String>,

    // === Streaming Options ===
    /// Enable UTF-8 token healing (default: true)
    /// When enabled, incomplete multi-byte UTF-8 sequences are buffered until complete
    pub utf8_healing: Option<bool>,
}

impl InferenceRequestInternal {
    /// Create a new internal request with generated ID
    pub fn new(cpid: String, prompt: String) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid,
            prompt,
            run_envelope: None,
            reasoning_mode: false,
            admin_override: false,
            stream: false,
            require_step: false,
            require_determinism: false,
            allow_fallback: true,
            batch_item_id: None,
            rag_enabled: false,
            rag_collection_id: None,
            dataset_version_id: None,
            adapter_stack: None,
            adapters: None,
            stack_id: None,
            domain_hint: None,
            stack_version: None,
            stack_determinism_mode: None,
            stack_routing_determinism_mode: None,
            effective_adapter_ids: None,
            adapter_strength_overrides: None,
            determinism_mode: None,
            routing_determinism_mode: None,
            seed_mode: None,
            request_seed: None,
            backend_profile: None,
            coreml_mode: None,
            max_tokens: 100,
            temperature: 0.7,
            top_k: None,
            top_p: None,
            seed: None,
            router_seed: None,
            require_evidence: false,
            session_id: None,
            pinned_adapter_ids: None,
            chat_context_hash: None,
            claims: None,
            policy_mask_digest_b3: None,
            model: None,
            stop_policy: None,
            created_at: std::time::Instant::now(),
            worker_auth_token: None,
            utf8_healing: None,
        }
    }

    /// Set streaming mode
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        if stream {
            self.require_step = true;
        }
        self
    }

    /// Set RAG options
    pub fn with_rag(mut self, collection_id: String) -> Self {
        self.rag_enabled = true;
        self.rag_collection_id = Some(collection_id);
        self
    }
}

/// Result from inference execution via InferenceCore
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferenceResult {
    /// Generated text
    pub text: String,
    /// Number of tokens generated
    pub tokens_generated: usize,
    /// Verifiable run receipt (when available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    /// Token usage computed by the worker tokenizer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    /// Reason for stopping (e.g., "stop", "length", "error")
    pub finish_reason: String,
    /// Adapters used during inference
    pub adapters_used: Vec<String>,
    /// Router decisions made during inference
    pub router_decisions: Vec<RouterDecisionRecord>,
    /// Cryptographically chained router decisions (per token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// Model type for this trace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
    /// RAG evidence if RAG was used
    pub rag_evidence: Option<RagEvidence>,
    /// Source citations derived from training files or RAG
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<adapteros_api_types::inference::Citation>,
    /// Total latency in milliseconds
    pub latency_ms: u64,
    /// Request ID for correlation
    pub request_id: String,
    /// Pinned adapter IDs that were unavailable (CHAT-PIN-02)
    ///
    /// These are adapters that were in the session's pinned set but were not
    /// available in the candidate adapter set. Returned for UI warning display.
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable (PRD-6A)
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    pub pinned_routing_fallback: Option<String>,
    /// Effective adapter set applied for this inference (if any)
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Backend used to execute the inference (e.g., coreml, metal, mlx)
    pub backend_used: Option<String>,
    /// Deterministic receipt for audit/replay metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deterministic_receipt: Option<adapteros_api_types::inference::DeterministicReceipt>,
    /// Whether backend fallback occurred during execution
    pub fallback_triggered: bool,
    /// Requested CoreML compute preference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this inference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Determinism mode applied after resolution
    pub determinism_mode_applied: Option<String>,
    /// Replay guarantee level computed for this inference
    pub replay_guarantee: Option<ReplayGuarantee>,
    /// Canonical run envelope for this execution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// Placement trace returned by worker (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_trace: Option<Vec<PlacementTraceEntry>>,

    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,
}

/// Router decision record for audit trail
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterDecisionRecord {
    /// Token generation step
    pub step: usize,
    /// Input token ID that triggered this decision
    pub input_token_id: Option<u32>,
    /// Candidate adapters considered
    pub candidates: Vec<RouterCandidateRecord>,
    /// Shannon entropy of gate distribution
    pub entropy: f64,
    /// Selected adapter IDs
    pub selected_adapters: Vec<String>,
    /// Fusion interval identifier active for this decision
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_id: Option<String>,
}

/// Router candidate record for decision audit
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterCandidateRecord {
    /// Adapter index
    pub adapter_idx: u16,
    /// Raw score before softmax
    pub raw_score: f32,
    /// Quantized gate value (Q15)
    pub gate_q15: i16,
}

/// RAG evidence for provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RagEvidence {
    /// Collection ID used for retrieval
    pub collection_id: String,
    /// Chunks used for context
    pub chunks_used: Vec<ChunkReference>,
    /// BLAKE3 hash of the combined context
    pub context_hash: String,
}

/// Reference to a document chunk used in RAG
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChunkReference {
    /// Document ID
    pub document_id: String,
    /// Chunk ID within document
    pub chunk_id: String,
    /// Page number (if applicable)
    pub page_number: Option<i32>,
    /// Relevance score
    pub relevance_score: f32,
    /// Rank in retrieval results
    pub rank: usize,
}
