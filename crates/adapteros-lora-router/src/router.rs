use crate::features::CodeFeatures;
use crate::orthogonal::OrthogonalConstraints;
use crate::policy_mask::PolicyMask;
use crate::{
    quantize_gate, AdapterInfo, Decision, DecisionCandidate, DecisionHash, RouterAbstainReason,
    RouterDeterminismConfig, RouterWeights, RoutingDecision, ScoringExplanation,
};
use adapteros_core::{determinism::DeterminismContext, AosError, B3Hash, Result};
use adapteros_numerics::check_numerics;
use adapteros_policy::packs::router::RouterConfig;
use adapteros_telemetry::events::{RouterCandidate as TelemetryCandidate, RouterDecisionEvent};
use adapteros_telemetry::writer::RouterDecisionWriter;
use adapteros_types::routing::RouterModelType;
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::constants::{FRAMEWORK_SPECIALIZATION_MULTIPLIER, LANGUAGE_AFFINITY_MULTIPLIER, MAX_K};
use crate::{framework_routing, path_routing, scoring};

/// Backend context for routing decisions.
///
/// This struct encapsulates backend identity information that is used to bind
/// routing decisions to a specific backend. For V5+ receipts, backend binding
/// is required to prevent cross-backend replay attacks.
///
/// # Example
///
/// ```
/// use adapteros_lora_router::BackendContext;
///
/// // Create with just backend_id
/// let ctx = BackendContext::new("mlx");
///
/// // Create with backend_id and kernel version
/// let ctx = BackendContext::with_kernel("coreml", "1.0.0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendContext {
    /// Backend identifier (e.g., "mlx", "coreml", "metal")
    pub backend_id: String,
    /// Optional kernel version identifier
    pub kernel_version_id: Option<String>,
}

impl BackendContext {
    /// Create a new backend context with just a backend_id.
    ///
    /// # Arguments
    /// * `backend_id` - The backend identifier (must be non-empty)
    ///
    /// # Panics
    /// Panics if `backend_id` is empty.
    pub fn new(backend_id: impl Into<String>) -> Self {
        let id = backend_id.into();
        assert!(!id.is_empty(), "backend_id must not be empty");
        Self {
            backend_id: id,
            kernel_version_id: None,
        }
    }

    /// Create a new backend context with backend_id and kernel version.
    ///
    /// # Arguments
    /// * `backend_id` - The backend identifier (must be non-empty)
    /// * `kernel_version_id` - The kernel version identifier
    ///
    /// # Panics
    /// Panics if `backend_id` is empty.
    pub fn with_kernel(
        backend_id: impl Into<String>,
        kernel_version_id: impl Into<String>,
    ) -> Self {
        let id = backend_id.into();
        assert!(!id.is_empty(), "backend_id must not be empty");
        Self {
            backend_id: id,
            kernel_version_id: Some(kernel_version_id.into()),
        }
    }

    /// Try to create a backend context, returning None if backend_id is empty.
    pub fn try_new(backend_id: impl Into<String>) -> Option<Self> {
        let id = backend_id.into();
        if id.is_empty() {
            None
        } else {
            Some(Self {
                backend_id: id,
                kernel_version_id: None,
            })
        }
    }

    /// Validate this backend context.
    ///
    /// # Returns
    /// * `Ok(())` if the backend context is valid
    /// * `Err` with a description of the validation failure
    pub fn validate(&self) -> Result<()> {
        if self.backend_id.is_empty() {
            return Err(AosError::DeterminismViolation(
                "Backend ID cannot be empty for routing decisions".to_string(),
            ));
        }
        Ok(())
    }
}

fn determinism_debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| match std::env::var("AOS_DEBUG_DETERMINISM") {
        Ok(val) => {
            let normalized = val.to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        }
        Err(_) => false,
    })
}

/// Ensure k-sparse is within valid bounds.
/// Clamps to 1-MAX_K range and warns if out of bounds.
fn sanitize_k(k: usize) -> usize {
    if k == 0 {
        tracing::warn!(k = k, "k_sparse must be at least 1; clamping to 1");
        1
    } else if k > MAX_K {
        tracing::warn!(
            k = k,
            max_k = MAX_K,
            "k_sparse exceeds maximum; clamping to {}",
            MAX_K
        );
        MAX_K
    } else {
        k
    }
}

/// Ensure entropy floor is valid for gate calculation.
/// Falls back to default when eps is non-finite or out of range.
fn sanitize_eps(eps: f32) -> f32 {
    use adapteros_core::defaults::DEFAULT_ENTROPY_FLOOR;

    if !eps.is_finite() {
        tracing::warn!(
            eps = eps,
            "entropy_floor must be finite; falling back to {}",
            DEFAULT_ENTROPY_FLOOR
        );
        DEFAULT_ENTROPY_FLOOR
    } else if !(0.0..=1.0).contains(&eps) {
        tracing::warn!(
            eps = eps,
            "entropy_floor must be between 0 and 1; falling back to {}",
            DEFAULT_ENTROPY_FLOOR
        );
        DEFAULT_ENTROPY_FLOOR
    } else {
        eps
    }
}

/// Ensure temperature is usable for deterministic softmax.
/// Falls back to 1.0 when tau is non-positive or non-finite to avoid
/// degenerate routing.
fn sanitize_tau(tau: f32) -> f32 {
    if !tau.is_finite() {
        tracing::warn!(
            tau = tau,
            "Router temperature must be positive and finite; falling back to 1.0"
        );
        1.0
    } else if tau <= 0.0 {
        tracing::warn!(
            tau = tau,
            "Router temperature must be positive; falling back to 1.0"
        );
        1.0
    } else {
        tau
    }
}

// =============================================================================
// Deterministic Tie-Breaking Sort
// =============================================================================

/// Tie event captured during deterministic sorting.
///
/// Records when two adapters had equal scores and tie-breaking was applied.
#[derive(Debug, Clone, Copy)]
pub struct TieEvent {
    /// Index of first adapter in tie
    pub idx_a: usize,
    /// Index of second adapter in tie
    pub idx_b: usize,
    /// Score of both adapters (equal by definition)
    pub score: f32,
}

/// Sort adapter scores with deterministic tie-breaking using stable_id.
///
/// Uses composite key `(-score, stable_id)` for total ordering:
/// - Primary: score descending (higher scores first)
/// - Secondary: stable_id ascending (lower stable_id wins ties)
///
/// # Determinism Guarantees
///
/// - Uses `f32::total_cmp()` for IEEE 754 total ordering
/// - NaN values sort consistently (after all finite values in descending order)
/// - Handles -0.0 consistently (equals +0.0 in total ordering)
/// - stable_id provides immutable tie-breaker across adapter lifecycle
///
/// # Arguments
///
/// * `scores` - Mutable slice of (adapter_index, score) tuples to sort in-place
/// * `adapter_info` - Adapter metadata for stable_id lookup
/// * `collect_ties` - Whether to collect tie events for diagnostics
///
/// # Returns
///
/// Vector of tie events (empty if `collect_ties` is false or no ties occurred).
///
/// # Receipt Binding
///
/// This sort order directly affects the `Decision.indices` which are hashed
/// into the receipt chain via `hash_token_decision()`. Deterministic ordering
/// is critical for receipt reproducibility.
pub fn sort_scores_deterministic(
    scores: &mut [(usize, f32)],
    adapter_info: &[AdapterInfo],
    collect_ties: bool,
) -> Vec<TieEvent> {
    let mut tie_events = Vec::new();

    scores.sort_by(|a, b| {
        // Primary: score descending using total_cmp for IEEE 754 total ordering
        let score_cmp = b.1.total_cmp(&a.1);

        if score_cmp == std::cmp::Ordering::Equal {
            // Exact tie: use stable_id for deterministic tie-breaking
            if collect_ties {
                tie_events.push(TieEvent {
                    idx_a: a.0,
                    idx_b: b.0,
                    score: a.1,
                });
            }
            // stable_id is assigned at registration and never changes
            adapter_info[a.0]
                .stable_id
                .cmp(&adapter_info[b.0].stable_id)
        } else {
            score_cmp
        }
    });

    tie_events
}

/// Router for selecting K adapters with quantized gates
pub struct Router {
    /// Feature weights for scoring
    feature_weights: RouterWeights,
    /// Number of top adapters to select
    k: usize,
    /// Temperature for softmax
    tau: f32,
    /// Entropy floor to prevent collapse
    eps: f32,
    /// Token counter for sampling (reserved for token-level telemetry)
    _token_count: usize,
    /// Log first N tokens fully (default: 128 per Telemetry Ruleset #9)
    full_log_tokens: usize,

    // DIR (Deterministic Inference Runtime) enhancements
    // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    /// Orthogonal constraints tracker
    orthogonal_constraints: Option<OrthogonalConstraints>,
    /// Whether orthogonal constraints are enabled
    orthogonal_enabled: bool,
    /// Compression ratio for multi-path outputs
    compression_ratio: f32,
    /// Whether shared downsample is enabled
    shared_downsample: bool,

    // Adapter stack support
    /// Currently active stack name (if any)
    active_stack_name: Option<String>,
    /// Adapter IDs that are part of the active stack
    active_stack_adapter_ids: Option<Vec<String>>,
    /// Cached hash of the active stack configuration
    active_stack_hash: Option<B3Hash>,

    // Telemetry
    /// Optional telemetry writer for routing decisions (non-blocking)
    telemetry_writer: Option<RouterDecisionWriter>,
    /// Current step counter for telemetry correlation
    step_counter: usize,

    // Determinism controls
    /// Determinism configuration for reproducible routing
    determinism_config: RouterDeterminismConfig,
    /// Whether adaptive (seeded, reproducible) tie-break routing is enabled
    adaptive_routing: bool,

    // Abstain thresholds
    /// Entropy threshold above which to abstain (high uncertainty)
    abstain_entropy_threshold: Option<f32>,
    /// Confidence threshold below which to abstain (low max gate)
    abstain_confidence_threshold: Option<f32>,
    /// Optional telemetry writer for abstain events
    abstain_telemetry_writer: Option<std::sync::Arc<adapteros_telemetry::TelemetryWriter>>,
    /// Last abstain events emitted during routing
    abstain_events: Vec<adapteros_telemetry::events::AbstainEvent>,
    /// Optional context attached to abstain events for active-learning loop
    abstain_context: Option<AbstainContext>,

    // Base-model awareness
    /// Routing bias supplied by the base model configuration (defaults to 1.0)
    routing_bias: f32,
    /// Whether the current base model is a Mixture-of-Experts model
    is_moe_model: bool,

    // Diagnostics
    /// Optional router diagnostics emitter for fine-grained decision events
    router_diag: Option<crate::router_diag::RouterDiag>,
}

/// Request-level context captured when abstaining so we can route the prompt
/// into an active-learning queue without affecting router determinism.
#[derive(Debug, Clone, Default)]
pub struct AbstainContext {
    pub request_id: Option<String>,
    pub stack_id: Option<String>,
    pub stack_version: Option<i64>,
    pub prompt_digest_b3: Option<String>,
    pub prompt_chars: Option<usize>,
    pub prompt: Option<String>,
    pub tenant_id: Option<String>,
}

impl Router {
    /// Create a new router with custom feature weights
    pub fn new_with_weights(feature_weights: RouterWeights, k: usize, tau: f32, eps: f32) -> Self {
        let k = sanitize_k(k);
        let tau = sanitize_tau(tau);
        let eps = sanitize_eps(eps);
        Self {
            feature_weights,
            k,
            tau,
            eps,
            _token_count: 0,
            full_log_tokens: 128, // Per Telemetry Ruleset #9
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
            telemetry_writer: None,
            step_counter: 0,
            determinism_config: RouterDeterminismConfig::default(),
            adaptive_routing: false,
            abstain_entropy_threshold: None,
            abstain_confidence_threshold: None,
            abstain_telemetry_writer: None,
            abstain_events: Vec::new(),
            abstain_context: None,
            routing_bias: 1.0,
            is_moe_model: false,
            router_diag: None,
        }
    }

    /// Create a new router from policy configuration
    ///
    /// This constructor reads the entropy floor and other parameters from the policy configuration
    /// instead of hardcoding values, ensuring consistency across the system.
    ///
    /// # Arguments
    /// * `feature_weights` - Custom router weights for feature importance
    /// * `k` - Number of top adapters to select (K-sparse parameter)
    /// * `tau` - Temperature for softmax
    /// * `policy_config` - Router policy configuration containing entropy floor and other settings
    ///
    /// # Returns
    /// New Router instance configured from policy
    pub fn new_with_policy_config(
        feature_weights: RouterWeights,
        k: usize,
        tau: f32,
        policy_config: &RouterConfig,
    ) -> Self {
        // Sanitize both k and policy k_sparse, then take minimum
        let sanitized_k = sanitize_k(k);
        let policy_k = sanitize_k(policy_config.k_sparse);
        let final_k = sanitized_k.min(policy_k);

        // Warn if original k exceeded policy limit (but was within absolute bounds)
        if k <= MAX_K && k > policy_config.k_sparse {
            tracing::warn!(
                "Requested k={} exceeds policy maximum k_sparse={}, clamping to policy maximum",
                k,
                policy_config.k_sparse
            );
        }

        Self {
            feature_weights,
            k: final_k,
            tau: sanitize_tau(tau),
            eps: sanitize_eps(policy_config.entropy_floor),
            _token_count: 0,
            full_log_tokens: policy_config.sample_tokens_full,
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
            telemetry_writer: None,
            step_counter: 0,
            determinism_config: RouterDeterminismConfig::default(),
            adaptive_routing: false,
            abstain_entropy_threshold: policy_config.abstain_entropy_threshold,
            abstain_confidence_threshold: policy_config.abstain_confidence_threshold,
            abstain_telemetry_writer: None,
            abstain_events: Vec::new(),
            abstain_context: None,
            routing_bias: 1.0,
            is_moe_model: false,
            router_diag: None,
        }
    }

    /// Set the telemetry writer for routing decision events
    pub fn set_telemetry_writer(&mut self, writer: RouterDecisionWriter) {
        self.telemetry_writer = Some(writer);
    }

    /// Clear the telemetry writer (useful for testing)
    pub fn clear_telemetry_writer(&mut self) {
        self.telemetry_writer = None;
    }

    /// Set the telemetry writer for abstain events
    pub fn set_abstain_telemetry_writer(
        &mut self,
        writer: std::sync::Arc<adapteros_telemetry::TelemetryWriter>,
    ) {
        self.abstain_telemetry_writer = Some(writer);
    }

    /// Set abstain thresholds
    pub fn set_abstain_thresholds(
        &mut self,
        entropy_threshold: Option<f32>,
        confidence_threshold: Option<f32>,
    ) {
        self.abstain_entropy_threshold = entropy_threshold;
        self.abstain_confidence_threshold = confidence_threshold;
    }

    /// Attach per-request abstain context for telemetry/active learning.
    pub fn set_abstain_context(&mut self, context: AbstainContext) {
        self.abstain_context = Some(context);
    }

    /// Clear the stored abstain context.
    pub fn clear_abstain_context(&mut self) {
        self.abstain_context = None;
    }

    /// Set the router diagnostics emitter for fine-grained decision events.
    ///
    /// When set, the router will emit RoutingStart, GateComputed, KsparseSelected,
    /// TieBreakApplied, and RoutingEnd events during routing decisions.
    ///
    /// Events are only emitted when `diag.level >= router`.
    pub fn set_router_diag(&mut self, diag: crate::router_diag::RouterDiag) {
        self.router_diag = Some(diag);
    }

    /// Clear the router diagnostics emitter.
    pub fn clear_router_diag(&mut self) {
        self.router_diag = None;
    }

    /// Take abstain events emitted during the last routing call.
    pub fn take_abstain_events(&mut self) -> Vec<adapteros_telemetry::events::AbstainEvent> {
        std::mem::take(&mut self.abstain_events)
    }

    /// Access the current abstain context (if any).
    pub fn abstain_context(&self) -> Option<&AbstainContext> {
        self.abstain_context.as_ref()
    }

    /// Set the routing bias derived from the base model configuration.
    pub fn set_routing_bias(&mut self, bias: f32) {
        if bias.is_finite() && bias > 0.0 {
            self.routing_bias = bias;
        } else {
            tracing::warn!(bias = bias, "Invalid routing bias; keeping previous value");
        }
    }

    /// Mark whether the active base model is a Mixture-of-Experts model.
    pub fn set_model_is_moe(&mut self, is_moe: bool) {
        self.is_moe_model = is_moe;
    }

    /// Set determinism configuration
    pub fn set_determinism_config(&mut self, config: RouterDeterminismConfig) {
        self.determinism_config = config;
    }

    /// Configure routing determinism (adaptive uses seeded tie-breaks and stays reproducible
    /// when a determinism context is provided)
    pub fn set_routing_determinism_mode(&mut self, adaptive: bool) {
        self.adaptive_routing = adaptive;
    }

    /// Whether adaptive routing is enabled
    pub fn adaptive_routing(&self) -> bool {
        self.adaptive_routing
    }

    /// Get current determinism configuration
    pub fn determinism_config(&self) -> &RouterDeterminismConfig {
        &self.determinism_config
    }

    /// Get the full log token count (for testing)
    pub fn full_log_tokens(&self) -> usize {
        self.full_log_tokens
    }

    /// Get the top-K selection size (for testing)
    pub fn top_k(&self) -> usize {
        self.k
    }

    /// Get abstain entropy threshold (for testing)
    pub fn abstain_entropy_threshold(&self) -> Option<f32> {
        self.abstain_entropy_threshold
    }

    /// Get abstain confidence threshold (for testing)
    pub fn abstain_confidence_threshold(&self) -> Option<f32> {
        self.abstain_confidence_threshold
    }

    /// Ensure router inputs do not contain NaN or Inf to protect determinism.
    fn validate_router_inputs(&self, features: &[f32], priors: &[f32]) -> Result<()> {
        if let Err(err) = check_numerics(features) {
            return Err(AosError::DeterminismViolation(format!(
                "Non-finite router feature detected: {err}"
            )));
        }

        if let Err(err) = check_numerics(priors) {
            return Err(AosError::DeterminismViolation(format!(
                "Non-finite router prior detected: {err}"
            )));
        }

        Ok(())
    }

    /// Emit a router decision telemetry event (non-blocking)
    ///
    /// This method does NOT fail on errors - it logs and continues to avoid blocking the hot path.
    fn emit_decision_event(&mut self, decision: &Decision, input_token_id: Option<u32>) {
        if let Some(ref writer) = self.telemetry_writer {
            // Convert Decision to RouterDecisionEvent
            let candidates: Vec<TelemetryCandidate> = decision
                .candidates
                .iter()
                .map(|c| TelemetryCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                })
                .collect();

            let event = RouterDecisionEvent {
                step: self.step_counter,
                input_token_id,
                candidate_adapters: candidates,
                entropy: decision.entropy,
                tau: self.tau,
                entropy_floor: self.eps,
                stack_hash: self.active_stack_hash.map(|h| h.to_short_hex()),
                stack_id: self.active_stack_name.clone(),
                stack_version: None, // Will be populated by stack metadata
                model_type: RouterModelType::Dense,
                active_experts: None,
                backend_type: None, // Populated at inference time (PRD-DET-001: G6)
            };

            // Emit event (non-blocking, logs on error)
            if let Err(e) = writer.emit(event) {
                tracing::debug!(
                    error = %e,
                    step = self.step_counter,
                    "Router decision telemetry dropped (channel full)"
                );
            }

            // Increment step counter
            self.step_counter += 1;
        }
    }

    /// Check abstain conditions and emit AbstainEvent if triggered
    ///
    /// This method checks both entropy and confidence thresholds:
    /// - High entropy: indicates router uncertainty about adapter selection
    /// - Low confidence: max gate value is below threshold, indicating no strong selection
    ///
    /// Events are emitted via telemetry writer if thresholds are configured and exceeded.
    ///
    /// Note: Empty decisions (no adapters selected) are skipped - they represent
    /// policy-based abstention rather than uncertainty-based abstention.
    fn check_abstain_conditions(&mut self, entropy: f32, gates: &[f32]) {
        use adapteros_telemetry::events::AbstainEvent;

        // Always clear previous events at the start of each routing step
        // to prevent stale events from being reported for subsequent requests
        self.abstain_events.clear();

        // Skip abstain checks for empty decisions (already abstained via policy/no adapters)
        if gates.is_empty() {
            return;
        }
        let mut events = Vec::new();
        let context = self.abstain_context.clone();

        let writer = self.abstain_telemetry_writer.clone();

        // Check high entropy threshold
        if let Some(entropy_threshold) = self.abstain_entropy_threshold {
            if entropy > entropy_threshold {
                let event = AbstainEvent::high_entropy(entropy, entropy_threshold).with_context(
                    context.as_ref().and_then(|c| c.request_id.clone()),
                    context.as_ref().and_then(|c| c.stack_id.clone()),
                    context.as_ref().and_then(|c| c.stack_version),
                    context.as_ref().and_then(|c| c.prompt_digest_b3.clone()),
                    context.as_ref().and_then(|c| c.prompt_chars),
                    context.as_ref().and_then(|c| c.tenant_id.clone()),
                );
                if let Some(ref writer) = writer {
                    if let Err(e) = writer.log_abstain(event.clone()) {
                        tracing::debug!(
                            error = %e,
                            entropy = entropy,
                            threshold = entropy_threshold,
                            "Failed to emit high entropy abstain event"
                        );
                    } else {
                        tracing::info!(
                            entropy = entropy,
                            threshold = entropy_threshold,
                            "Router abstaining due to high entropy (uncertainty)"
                        );
                    }
                }
                events.push(event);
            }
        }

        // Check low confidence threshold (max gate below threshold)
        if let Some(confidence_threshold) = self.abstain_confidence_threshold {
            let max_gate = gates.iter().fold(0.0f32, |a, &b| a.max(b));
            if max_gate < confidence_threshold {
                let event = AbstainEvent::low_confidence(max_gate, confidence_threshold)
                    .with_context(
                        context.as_ref().and_then(|c| c.request_id.clone()),
                        context.as_ref().and_then(|c| c.stack_id.clone()),
                        context.as_ref().and_then(|c| c.stack_version),
                        context.as_ref().and_then(|c| c.prompt_digest_b3.clone()),
                        context.as_ref().and_then(|c| c.prompt_chars),
                        context.as_ref().and_then(|c| c.tenant_id.clone()),
                    );
                if let Some(ref writer) = writer {
                    if let Err(e) = writer.log_abstain(event.clone()) {
                        tracing::debug!(
                            error = %e,
                            max_gate = max_gate,
                            threshold = confidence_threshold,
                            "Failed to emit low confidence abstain event"
                        );
                    } else {
                        tracing::info!(
                            max_gate = max_gate,
                            threshold = confidence_threshold,
                            "Router abstaining due to low confidence (max gate below threshold)"
                        );
                    }
                }
                events.push(event);
            }
        }

        // Persist events for active-learning loop consumers
        if !events.is_empty() {
            self.abstain_events = events;
        }
    }

    /// Create a new router with default weights (for backward compatibility)
    pub fn new(_weights: Vec<f32>, k: usize, tau: f32, eps: f32, _seed: [u8; 32]) -> Result<Self> {
        if k > MAX_K {
            return Err(adapteros_core::AosError::Routing(
                format!(
                    "Adapter routing failed: K={} exceeds maximum allowed value of {}. Reduce the number of top adapters to select (K parameter) to {} or less.",
                    k, MAX_K, MAX_K
                )
            ));
        }
        // Legacy constructor - ignores old weights vector, uses default RouterWeights
        Ok(Self::new_with_weights(
            RouterWeights::default(),
            k,
            tau,
            eps,
        ))
    }

    /// Get temperature (tau) for telemetry
    pub fn tau(&self) -> f32 {
        self.tau
    }

    /// Get entropy floor (eps) for telemetry
    pub fn eps(&self) -> f32 {
        self.eps
    }

    /// Set full log token count
    pub fn set_full_log_tokens(&mut self, count: usize) {
        self.full_log_tokens = count;
    }

    /// Score framework adapters using the configured framework weight.
    pub fn score_frameworks(
        &self,
        query: &str,
        contexts: &[framework_routing::FrameworkRoutingContext],
    ) -> Vec<framework_routing::FrameworkRoutingScore> {
        framework_routing::compute_framework_scores(
            query,
            contexts,
            self.feature_weights.framework_weight,
        )
    }

    /// Score directory adapters using the configured path weight.
    pub fn score_paths(
        &self,
        query: &str,
        contexts: &[path_routing::DirectoryRoutingContext],
    ) -> Vec<path_routing::PathRoutingScore> {
        path_routing::compute_path_scores(query, contexts, self.feature_weights.path_tokens_weight)
    }

    /// Enable orthogonal constraints for DIR (Deterministic Inference Runtime)
    /// Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    pub fn set_orthogonal_constraints(
        &mut self,
        enabled: bool,
        similarity_threshold: f32,
        penalty_weight: f32,
        history_window: usize,
    ) {
        self.orthogonal_enabled = enabled;
        if enabled {
            self.orthogonal_constraints = Some(OrthogonalConstraints::new(
                similarity_threshold,
                penalty_weight,
                history_window,
            ));
        } else {
            self.orthogonal_constraints = None;
        }
    }

    /// Set compression ratio for multi-path outputs
    pub fn set_compression_ratio(&mut self, ratio: f32) {
        self.compression_ratio = ratio.clamp(0.1, 1.0);
    }

    /// Enable shared downsample matrix
    pub fn set_shared_downsample(&mut self, enabled: bool) {
        self.shared_downsample = enabled;
    }

    /// Set the active adapter stack for filtering
    pub fn set_active_stack(
        &mut self,
        stack_name: Option<String>,
        adapter_ids: Option<Vec<String>>,
        stack_hash: Option<B3Hash>,
    ) {
        tracing::debug!(
            "Setting active stack: {:?} with {} adapters, hash: {:?}",
            stack_name,
            adapter_ids.as_ref().map(|ids| ids.len()).unwrap_or(0),
            stack_hash.as_ref().map(|h| h.to_short_hex())
        );
        self.active_stack_name = stack_name;
        self.active_stack_adapter_ids = adapter_ids;
        self.active_stack_hash = stack_hash;
    }

    /// Get the currently active stack name
    pub fn active_stack(&self) -> Option<&String> {
        self.active_stack_name.as_ref()
    }

    /// Get the cached stack hash as hex string
    pub fn stack_hash(&self) -> Option<String> {
        self.active_stack_hash.map(|hash| hash.to_short_hex())
    }

    /// Filter adapter indices based on the active stack
    fn filter_by_stack(&self, adapter_info: &[AdapterInfo]) -> Vec<usize> {
        match &self.active_stack_adapter_ids {
            Some(allowed_ids) => {
                tracing::debug!(
                    "Filtering adapters by stack: {} allowed IDs",
                    allowed_ids.len()
                );
                adapter_info
                    .iter()
                    .enumerate()
                    .filter(|(_, info)| allowed_ids.contains(&info.id))
                    .map(|(idx, _)| idx)
                    .collect()
            }
            None => {
                // No stack active, all adapters are candidates
                (0..adapter_info.len()).collect()
            }
        }
    }

    /// Check if an adapter is in the active stack (reserved for stack-based filtering)
    fn _is_in_active_stack(&self, adapter_id: &str) -> bool {
        match &self.active_stack_adapter_ids {
            Some(allowed_ids) => allowed_ids.contains(&adapter_id.to_string()),
            None => true, // No stack = all adapters allowed
        }
    }

    /// Get current diversity score from orthogonal constraints
    pub fn diversity_score(&self) -> f32 {
        if let Some(ref constraints) = self.orthogonal_constraints {
            constraints.diversity_score()
        } else {
            1.0 // Maximum diversity when constraints are disabled
        }
    }

    /// Get the softmax temperature (tau) used by this router
    pub fn temperature(&self) -> f32 {
        self.tau
    }

    /// Get the entropy floor (epsilon) enforced by this router
    pub fn entropy_floor(&self) -> f32 {
        self.eps
    }

    /// Compute weighted feature score from 22-dimensional feature vector
    ///
    /// Feature vector layout:
    /// - [0..8]: language one-hot (8 dims)
    /// - [8..11]: framework scores (3 dims)
    /// - [11]: symbol hits (1 dim)
    /// - [12]: path tokens (1 dim)
    /// - [13..21]: prompt verb one-hot (8 dims)
    /// - [21]: attention entropy (1 dim)
    ///
    /// DEPRECATED: This method computes a global score that's the same for all adapters.
    /// Use `compute_adapter_feature_score` instead for per-adapter scoring.
    fn compute_weighted_score(&self, features: &[f32]) -> f32 {
        // Support both legacy (21) and new (22) feature vectors
        if features.len() != 21 && features.len() != 22 {
            // Fallback for unexpected feature vectors
            return features.iter().sum::<f32>() * 0.1;
        }

        let mut score = 0.0;

        // Language component (take max of one-hot as language strength)
        let lang_strength = features[0..8].iter().fold(0.0f32, |a, &b| a.max(b));
        score += lang_strength * self.feature_weights.language_weight;

        // Framework component (sum of top 3 framework scores)
        let framework_strength = features[8..11].iter().sum::<f32>();
        score += framework_strength * self.feature_weights.framework_weight;

        // Symbol hits component
        score += features[11] * self.feature_weights.symbol_hits_weight;

        // Path tokens component
        score += features[12] * self.feature_weights.path_tokens_weight;

        // Prompt verb component (max of one-hot)
        let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));
        score += verb_strength * self.feature_weights.prompt_verb_weight;

        score
    }

    /// Compute per-adapter feature score based on adapter metadata
    ///
    /// This is the correct scoring method that produces different scores for each adapter
    /// based on how well the adapter's metadata matches the detected features.
    ///
    /// # Arguments
    /// * `features` - Global 22-dimensional feature vector from prompt/context
    /// * `adapter_info` - Adapter metadata (framework, languages, tier)
    ///
    /// # Returns
    /// Adapter-specific feature relevance score
    fn compute_adapter_feature_score(&self, features: &[f32], adapter_info: &AdapterInfo) -> f32 {
        if features.len() != 21 && features.len() != 22 && features.len() != 25 {
            return 0.0;
        }

        let mut score = 0.0;

        // Language affinity: Check if adapter supports the detected language
        // features[0..8] is language one-hot encoding
        if !features.is_empty() && features.len() >= 8 {
            let detected_lang_idx = features[0..8]
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(idx, _)| idx);

            if let Some(lang_idx) = detected_lang_idx {
                if adapter_info.supports_language(lang_idx) && features[lang_idx] > 0.0 {
                    // Boost score if adapter supports the detected language
                    score += features[lang_idx]
                        * self.feature_weights.language_weight
                        * LANGUAGE_AFFINITY_MULTIPLIER;
                }
            }
        }

        // Framework affinity: Check if adapter's framework matches detected framework
        // features[8..11] are framework scores
        if features.len() >= 11 {
            if let Some(ref _adapter_framework) = adapter_info.framework {
                // Framework scores are already extracted in features[8..11]
                // We give a boost if the adapter has a framework specialization
                let framework_strength = features[8..11].iter().sum::<f32>();
                if framework_strength > 0.0 {
                    // Boost adapters that have framework specialization when frameworks are detected
                    score += framework_strength
                        * self.feature_weights.framework_weight
                        * FRAMEWORK_SPECIALIZATION_MULTIPLIER;
                }
            }
        }

        // Symbol hits: All adapters benefit equally from symbol density
        if features.len() >= 12 {
            score += features[11] * self.feature_weights.symbol_hits_weight;
        }

        // Path tokens: All adapters benefit equally from path context
        if features.len() >= 13 {
            score += features[12] * self.feature_weights.path_tokens_weight;
        }

        // Prompt verb: All adapters benefit equally from verb classification
        if features.len() >= 21 {
            let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));
            score += verb_strength * self.feature_weights.prompt_verb_weight;
        }

        // Tier-based boost: Higher tiers get slight boost
        use crate::types::{AdapterTier, LoraTier};
        use std::str::FromStr;

        let tier_boost = AdapterTier::from_str(&adapter_info.tier)
            .unwrap_or_default()
            .boost();
        score += tier_boost;

        if let Some(ref lora_tier) = adapter_info.lora_tier {
            let tier_bonus = lora_tier
                .parse::<LoraTier>()
                .ok()
                .map(|t| t.boost())
                .unwrap_or(0.0);
            score += tier_bonus;
        }

        score
    }

    /// Apply deterministic prior boost based on adapter reasoning specialties.
    ///
    /// This is used by `route_on_reasoning` to bias routing decisions based on
    /// the model's generated rationale rather than the original prompt.
    fn apply_reasoning_specialty_boost(
        &self,
        priors: &mut [f32],
        rationale: &str,
        adapter_info: &[AdapterInfo],
    ) {
        if rationale.is_empty() || priors.len() != adapter_info.len() {
            return;
        }

        let rationale_lower = rationale.to_ascii_lowercase();
        for (prior, info) in priors.iter_mut().zip(adapter_info.iter()) {
            if info.reasoning_specialties.is_empty() {
                continue;
            }
            let mut hits = 0;
            for specialty in &info.reasoning_specialties {
                if specialty.is_empty() {
                    continue;
                }
                let needle = specialty.to_ascii_lowercase();
                if rationale_lower.contains(&needle) {
                    hits += 1;
                }
            }

            if hits > 0 {
                let coverage = hits as f32 / (info.reasoning_specialties.len() as f32).max(1.0);
                // Reuse prompt_verb_weight to keep boosts aligned with existing weighting scale.
                let boost = coverage * self.feature_weights.prompt_verb_weight * self.routing_bias;
                *prior += boost;
            }
        }
    }

    /// Score and select top-K adapters
    ///
    /// # DEPRECATED - Use route_with_adapter_info() instead
    ///
    /// This method uses a global feature score that doesn't distinguish between adapters.
    /// It's maintained for backward compatibility only.
    ///
    /// For proper per-adapter scoring:
    /// ```rust
    /// use adapteros_lora_router::{AdapterInfo, Router, RouterWeights, policy_mask::PolicyMask};
    ///
    /// // Old (deprecated):
    /// let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    /// let features = vec![0.0f32; 22];
    /// let priors = vec![0.5f32, 0.3f32, 0.2f32];
    /// #[allow(deprecated)]
    /// let _decision = router
    ///     .route(&features, &priors)
    ///     .into_selected()
    ///     .expect("routing decision");
    ///
    /// // New (recommended):
    /// let adapter_info: Vec<AdapterInfo> = (0..priors.len())
    ///     .map(|i| AdapterInfo {
    ///         id: format!("adapter_{}", i),
    ///         framework: None,
    ///         languages: vec![],
    ///         tier: "default".to_string(),
    ///         scope_path: None,
    ///         lora_tier: None,
    ///         base_model: None,
    ///         ..Default::default()
    ///     })
    ///     .collect();
    /// let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    /// let mask = PolicyMask::allow_all(&adapter_ids, None);
    /// let decision = router
    ///     .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
    ///     .expect("router decision");
    /// assert_eq!(decision.indices.len(), 2);
    /// ```
    ///
    /// The new API enables:
    /// - Per-adapter feature scoring (language affinity, framework specialization)
    /// - Proper orthogonality penalties during selection
    /// - Stack-based filtering
    /// - DIR diversity controls
    ///
    /// See [ROUTER_MIGRATION.md](../../docs/ROUTER_MIGRATION.md) for complete migration steps.
    /// Returns `RoutingDecision::Abstain` when no adapter qualifies or configuration is empty.
    #[deprecated(
        since = "0.1.1",
        note = "Use route_with_adapter_info() for per-adapter scoring"
    )]
    pub fn route(&mut self, features: &[f32], priors: &[f32]) -> RoutingDecision {
        tracing::warn!(
            "Router::route() is deprecated, use route_with_adapter_info() instead. \
             See docs/ROUTER_MIGRATION.md for migration guide"
        );

        if let Err(err) = self.validate_router_inputs(features, priors) {
            tracing::warn!(
                error = %err,
                "Router::route rejected input due to non-finite values"
            );
            return RoutingDecision::Abstain(RouterAbstainReason::InvalidNumerics(err.to_string()));
        }

        if priors.is_empty() {
            return RoutingDecision::Abstain(RouterAbstainReason::EmptyRouterConfig);
        }

        // Compute weighted feature score once
        let feature_score = self.compute_weighted_score(features);

        // Compute scores for each adapter combining prior and features
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
            .map(|(i, &prior)| {
                let score = prior + feature_score;
                (i, score)
            })
            .collect();

        // If every score falls below the abstain threshold, return explicit abstention.
        const SCORE_ABSTAIN_THRESHOLD: f32 = 0.1;
        let max_score = scores
            .iter()
            .map(|(_, score)| *score)
            .fold(f32::NEG_INFINITY, f32::max);
        if max_score <= SCORE_ABSTAIN_THRESHOLD {
            return RoutingDecision::Abstain(RouterAbstainReason::ScoresBelowThreshold {
                threshold: SCORE_ABSTAIN_THRESHOLD,
                max_score,
            });
        }

        // Sort by score descending, then by index for determinism (tie-breaker keeps per-token decisions stable)
        // Issue D-2 Fix: Use total_cmp for IEEE 754 total ordering (handles NaN deterministically)
        //
        // NOTE: This deprecated function uses array index for tie-breaking, not stable_id.
        // Use route_with_adapter_info() for proper stable_id-based tie-breaking.
        scores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Softmax with temperature using deterministic f64 + Kahan path
        let mut gates: Vec<f32> = Self::deterministic_softmax(&top_k, self.tau);
        let active_k = gates.len().max(1);
        let min_gate = self.eps / active_k as f32;
        for g in &mut gates {
            *g = g.max(min_gate);
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        let entropy = Self::compute_entropy(&gates);

        let mut candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates.iter())
            .map(|((adapter_idx, raw_score), &gate_f32)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15: quantize_gate(gate_f32),
            })
            .collect();

        Self::sort_candidates_by_quantized_gate(&mut candidate_entries);

        let gates_q15: SmallVec<[i16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.gate_q15)
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        // Assert 1:1 mapping
        assert_eq!(
            indices.len(),
            gates_q15.len(),
            "RouterRing must match gate count"
        );
        assert!(
            indices.len() == indices.iter().collect::<HashSet<_>>().len(),
            "Indices must be unique"
        );

        // Apply orthogonal constraints if enabled
        if self.orthogonal_enabled {
            if let Some(ref mut constraints) = self.orthogonal_constraints {
                // Update activation history for diversity tracking
                // Note: Penalty-based rescoring deferred to post-alpha (DIR full implementation)
                // See: https://openreview.net/pdf?id=jqz6Msm3AF
                constraints.update_history(&indices, &gates_q15);
            }
        }

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash: None, // Deprecated method doesn't use decision hashing
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        RoutingDecision::Selected(decision)
    }

    /// Compute Shannon entropy of gate distribution
    fn compute_entropy(gates: &[f32]) -> f32 {
        gates
            .iter()
            .filter(|&&g| g > 0.0)
            .map(|&g| -g * g.log2())
            .sum()
    }

    /// Order candidates by quantized gate (desc), then raw score, then index for deterministic ties.
    fn sort_candidates_by_quantized_gate(candidates: &mut Vec<DecisionCandidate>) {
        candidates.sort_by(|a, b| {
            b.gate_q15
                .cmp(&a.gate_q15)
                .then_with(|| b.raw_score.total_cmp(&a.raw_score))
                .then_with(|| a.adapter_idx.cmp(&b.adapter_idx))
        });
    }

    /// Deterministic softmax using f64 intermediate precision and Kahan summation
    ///
    /// This implementation provides IEEE 754 deterministic behavior by:
    /// 1. Using f64 for intermediate computations to reduce rounding errors
    /// 2. Kahan summation for numerically stable sum calculation
    /// 3. Consistent ordering of operations
    ///
    /// # Arguments
    /// * `logits` - Input scores (raw logits)
    /// * `tau` - Temperature parameter for softmax
    ///
    /// # Returns
    /// Softmax probabilities as f32 (converted from f64 intermediate precision)
    pub(crate) fn deterministic_softmax(logits: &[(usize, f32)], tau: f32) -> Vec<f32> {
        if logits.is_empty() {
            return Vec::new();
        }
        if tau == 0.0 {
            // Hard routing: pick top score, tie-break by adapter index ASC.
            let mut best: Option<(usize, f32)> = None;
            for (adapter_idx, score) in logits.iter() {
                match best {
                    None => best = Some((*adapter_idx, *score)),
                    Some((best_idx, best_score)) => {
                        // Use total_cmp() for strict determinism with floating-point comparison.
                        // This ensures consistent tie-breaking behavior regardless of NaN or ordering edge cases.
                        if score.total_cmp(&best_score) == std::cmp::Ordering::Greater
                            || (score.total_cmp(&best_score) == std::cmp::Ordering::Equal
                                && *adapter_idx < best_idx)
                        {
                            best = Some((*adapter_idx, *score));
                        }
                    }
                }
            }
            // SAFETY: `best` is always Some here because:
            // 1. We check `logits.is_empty()` at the start and return early
            // 2. The loop above always sets `best = Some(...)` on the first iteration
            // Using unwrap_or_else with a uniform fallback as defensive coding,
            // though this branch is unreachable in practice.
            let winner = match best {
                Some(w) => w,
                None => {
                    // Unreachable: logits was already confirmed non-empty
                    debug_assert!(false, "best should always be Some for non-empty logits");
                    return vec![1.0 / logits.len() as f32; logits.len()];
                }
            };
            return logits
                .iter()
                .map(|(idx, _)| if *idx == winner.0 { 1.0 } else { 0.0 })
                .collect();
        }

        let tau = sanitize_tau(tau);

        debug_assert!(
            logits.iter().all(|(_, s)| s.is_finite()),
            "deterministic_softmax received non-finite logits"
        );

        // Find max for numerical stability (use f64 for intermediate computation)
        let max = logits
            .iter()
            .map(|(_, score)| *score as f64)
            .fold(f64::NEG_INFINITY, f64::max);

        // Compute exponentials and sum using Kahan summation for numerical stability
        let mut sum = 0.0f64;
        let mut c = 0.0f64; // Kahan summation compensation
        let exps: Vec<f64> = logits
            .iter()
            .map(|(_, score)| {
                let exp = (((*score as f64) - max) / (tau as f64)).exp();

                // Kahan summation: accumulate with compensation for lost low-order bits
                let y = exp - c;
                let t = sum + y;
                c = (t - sum) - y;
                sum = t;

                exp
            })
            .collect();

        // Normalize and convert back to f32
        let result = if sum == 0.0 {
            // All logits were -inf, return uniform distribution
            let uniform = 1.0f32 / logits.len() as f32;
            vec![uniform; logits.len()]
        } else {
            exps.iter().map(|&e| (e / sum) as f32).collect()
        };

        if determinism_debug_enabled() {
            let entropy = result
                .iter()
                .filter(|&&p| p > 0.0)
                .map(|&p| -p * p.ln())
                .sum::<f32>();
            tracing::debug!(
                target: "determinism",
                tau = tau,
                logit_count = logits.len(),
                sum_f64 = sum,
                entropy = entropy,
                max_gate = result.iter().copied().fold(0.0f32, f32::max),
                "Softmax computed (AOS_DEBUG_DETERMINISM=1)"
            );
        }

        result
    }

    /// Compute decision hash for audit and reproducibility verification
    ///
    /// Hashes both inputs and outputs to create a verifiable audit trail.
    ///
    /// # Arguments
    /// * `features` - Input feature vector
    /// * `priors` - Input prior scores
    /// * `indices` - Output adapter indices
    /// * `gates_q15` - Output quantized gates
    /// * `reasoning_hash` - Optional hash of reasoning buffer for dynamic routing
    /// * `backend_identity_hash` - Optional hash of backend identity for determinism binding (PRD-DET-001)
    ///
    /// # Returns
    /// DecisionHash containing input, output, and combined hashes
    fn compute_decision_hash(
        &self,
        features: &[f32],
        priors: &[f32],
        indices: &[u16],
        gates_q15: &[i16],
        reasoning_hash: Option<&B3Hash>,
        backend_identity_hash: Option<&B3Hash>,
    ) -> DecisionHash {
        // Hash inputs (features + priors)
        let mut input_bytes = Vec::new();
        for &f in features {
            input_bytes.extend_from_slice(&f.to_le_bytes());
        }
        for &p in priors {
            input_bytes.extend_from_slice(&p.to_le_bytes());
        }
        if let Some(reasoning) = reasoning_hash {
            input_bytes.extend_from_slice(reasoning.as_bytes());
        }
        let input_hash = B3Hash::hash(&input_bytes);

        // Hash outputs (indices + gates)
        let mut output_bytes = Vec::new();
        for &idx in indices {
            output_bytes.extend_from_slice(&idx.to_le_bytes());
        }
        for &gate in gates_q15 {
            output_bytes.extend_from_slice(&gate.to_le_bytes());
        }
        if let Some(reasoning) = reasoning_hash {
            output_bytes.extend_from_slice(reasoning.as_bytes());
        }
        let output_hash = B3Hash::hash(&output_bytes);

        // Combine all hashes for compact verification (PRD-DET-001: include backend)
        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(input_hash.as_bytes());
        combined_bytes.extend_from_slice(output_hash.as_bytes());
        if let Some(reasoning) = reasoning_hash {
            combined_bytes.extend_from_slice(reasoning.as_bytes());
        }
        if let Some(backend) = backend_identity_hash {
            combined_bytes.extend_from_slice(backend.as_bytes());
        }
        let combined_hash = B3Hash::hash(&combined_bytes);

        DecisionHash {
            input_hash: input_hash.to_short_hex(),
            output_hash: output_hash.to_short_hex(),
            reasoning_hash: reasoning_hash.map(|h| h.to_short_hex()),
            combined_hash: combined_hash.to_short_hex(),
            tau: self.tau,
            eps: self.eps,
            k: self.k,
            backend_identity_hash: backend_identity_hash.map(|h| h.to_short_hex()),
        }
    }

    /// Compute per-adapter orthogonality penalty
    ///
    /// This computes how much penalty each adapter should receive based on similarity
    /// to recent selections in the activation history.
    fn compute_adapter_orthogonal_penalty(&self, adapter_idx: usize) -> f32 {
        if !self.orthogonal_enabled {
            return 0.0;
        }

        if let Some(ref constraints) = self.orthogonal_constraints {
            let penalty = constraints.compute_adapter_penalty(adapter_idx);
            penalty * self.feature_weights.orthogonal_weight
        } else {
            0.0
        }
    }

    /// Score and select top-K adapters with per-adapter feature scoring and orthogonality penalties
    ///
    /// This is the corrected routing method that:
    /// 1. Computes per-adapter feature scores (different for each adapter)
    /// 2. Applies orthogonality penalties BEFORE selection
    /// 3. Ensures diversity controls actually affect selection
    ///
    /// # Arguments
    /// * `features` - Global feature vector from prompt/context
    /// * `priors` - Prior scores for each adapter
    /// * `adapter_info` - Metadata for each adapter
    ///
    /// # Returns
    /// Result containing a Decision with selected adapter indices and Q15 gates
    pub fn route_with_adapter_info(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
    ) -> Result<Decision> {
        self.route_with_adapter_info_with_ctx(features, priors, adapter_info, policy_mask, None)
    }

    /// Route with an explicit determinism context for deterministic tie-breaking.
    pub fn route_with_adapter_info_with_ctx(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        self.route_with_adapter_info_and_scope_with_ctx(
            features,
            priors,
            adapter_info,
            policy_mask,
            None,
            determinism,
        )
    }

    /// Scope-aware routing that filters adapters by scope_path when provided.
    /// If no adapters match the scope hint, routing falls back to the original priors.
    pub fn route_with_adapter_info_and_scope(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        scope_hint: Option<&str>,
    ) -> Result<Decision> {
        self.route_with_adapter_info_and_scope_with_ctx(
            features,
            priors,
            adapter_info,
            policy_mask,
            scope_hint,
            None,
        )
    }

    /// Scope-aware routing that filters adapters by scope_path when provided with a determinism context.
    pub fn route_with_adapter_info_and_scope_with_ctx(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        scope_hint: Option<&str>,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        self.validate_router_inputs(features, priors)?;

        // Get current step index for diagnostic events
        // Note: step_counter is incremented in emit_decision_event after telemetry is emitted
        let step_idx = self.step_counter as u32;

        // Emit RoutingStart diagnostic event
        if let Some(ref diag) = self.router_diag {
            diag.emit_routing_start(step_idx, adapter_info.len() as u32, self.k as u32, features);
        }

        let mut filtered_priors: Option<Vec<f32>> = None;
        if let Some(hint) = scope_hint {
            let mut priors_copy = priors.to_vec();
            let mut matched = false;
            for (prior, info) in priors_copy.iter_mut().zip(adapter_info.iter()) {
                if info.scope_path.as_deref() == Some(hint) {
                    matched = true;
                } else {
                    // Keep numerics finite while effectively zeroing non-matching adapters
                    *prior = -1.0e9;
                }
            }
            if matched {
                filtered_priors = Some(priors_copy);
            }
        }

        let priors_for_routing: &[f32] = filtered_priors.as_deref().unwrap_or(priors);

        if self.adaptive_routing && determinism.is_none() {
            return Err(adapteros_core::AosError::Config(
                "Adaptive routing configuration error: determinism context is required for seeded tie-breaking when adaptive routing is enabled. Provide a determinism context in the request or disable adaptive routing."
                    .to_string(),
            ));
        }

        if priors_for_routing.len() != adapter_info.len() {
            tracing::error!(
                "Priors length ({}) != adapter_info length ({}); denying routing decision",
                priors_for_routing.len(),
                adapter_info.len()
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }

        if policy_mask.allowed.len() != adapter_info.len() {
            tracing::error!(
                "Policy mask length ({}) != adapter_info length ({}); denying routing decision",
                policy_mask.allowed.len(),
                adapter_info.len()
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }

        // Compute scores for each adapter with per-adapter feature scoring and penalties
        let mut scores: Vec<(usize, f32)> = Vec::with_capacity(priors_for_routing.len());
        for (i, &prior) in priors_for_routing.iter().enumerate() {
            if !policy_mask.allowed.get(i).copied().unwrap_or(false) {
                continue;
            }
            if !prior.is_finite() {
                return Err(AosError::DeterminismViolation(format!(
                    "Non-finite prior score for adapter {}",
                    i
                )));
            }

            // Compute adapter-specific feature score (DIFFERENT for each adapter)
            let adapter_feature_score =
                self.compute_adapter_feature_score(features, &adapter_info[i]);
            if !adapter_feature_score.is_finite() {
                return Err(AosError::DeterminismViolation(format!(
                    "Non-finite feature score for adapter {}",
                    i
                )));
            }

            // Compute orthogonality penalty (if enabled)
            let orthogonal_penalty = self.compute_adapter_orthogonal_penalty(i);
            if !orthogonal_penalty.is_finite() {
                return Err(AosError::DeterminismViolation(format!(
                    "Non-finite orthogonal penalty for adapter {}",
                    i
                )));
            }

            // Combine: prior + features - penalty, then apply model-aware bias
            let mut score = scoring::compute_score(
                prior + adapter_feature_score - orthogonal_penalty,
                self.routing_bias,
            );
            if self.is_moe_model && !adapter_info[i].recommended_for_moe {
                score *= 0.8;
            }
            if !score.is_finite() {
                return Err(AosError::DeterminismViolation(format!(
                    "Non-finite combined score for adapter {}",
                    i
                )));
            }

            // Emit GateComputed diagnostic event for this adapter
            if let Some(ref diag) = self.router_diag {
                diag.emit_gate_computed(step_idx, &adapter_info[i], score);
            }

            scores.push((i, score));
        }

        // Return empty decision if no adapters passed policy filtering
        if scores.is_empty() {
            tracing::debug!(
                target: "determinism",
                "No adapters passed policy filtering; returning empty decision"
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }

        // Prepare adaptive tie-breakers when adaptive routing is enabled
        // SAFETY: The guard at line 1477-1482 ensures `determinism.is_some()` when
        // `self.adaptive_routing` is true. We extract the seed once here to avoid
        // repeated Option unwrapping in debug paths below.
        let (tie_breakers, adaptive_seed): (Vec<u64>, Option<[u8; 32]>) = if self.adaptive_routing {
            // The earlier guard guarantees determinism.is_some() here.
            // Use a match for explicit handling instead of expect.
            let det_ctx = match determinism {
                Some(ctx) => ctx,
                None => {
                    // This branch is unreachable due to the guard at line 1477-1482,
                    // but we handle it defensively to avoid panics if code is refactored.
                    return Err(adapteros_core::AosError::Config(
                        "Internal error: adaptive routing requires determinism context".to_string(),
                    ));
                }
            };
            let seed = det_ctx.router_tiebreak_seed();
            let mut rng = ChaCha20Rng::from_seed(seed);
            let breakers: Vec<u64> = (0..adapter_info.len()).map(|_| rng.gen()).collect();
            (breakers, Some(seed))
        } else {
            (Vec::new(), None)
        };
        let log_ties = determinism_debug_enabled();
        let emit_diag_ties = self.router_diag.is_some();
        if log_ties {
            tracing::info!(
                target: "determinism",
                adapter_count = adapter_info.len(),
                scores_count = scores.len(),
                k = self.k,
                tau = self.tau,
                eps = self.eps,
                adaptive_routing = self.adaptive_routing,
                "Router scoring complete (AOS_DEBUG_DETERMINISM=1)"
            );
            if let Some(seed) = adaptive_seed {
                let seed_hash = B3Hash::hash(&seed);
                let seed_hex = seed_hash.to_hex();
                tracing::info!(
                    target: "determinism",
                    tie_seed_prefix = %seed_hex.get(..16).unwrap_or(&seed_hex),
                    tie_breakers = tie_breakers.len(),
                    "Adaptive routing tie-break seed"
                );
            }
        }
        // Sort by score descending with stable_id tie-breaking (Issue D-2 Fix)
        // See sort_scores_deterministic() for full determinism guarantees.
        let collect_ties = log_ties || emit_diag_ties;
        let tie_events = sort_scores_deterministic(&mut scores, adapter_info, collect_ties);

        // Log tie events if debug enabled
        if log_ties && !tie_events.is_empty() {
            tracing::info!(
                target: "determinism",
                tie_count = tie_events.len(),
                adaptive = self.adaptive_routing,
                "Router tie-break events (AOS_DEBUG_DETERMINISM=1)"
            );
            for event in tie_events.iter().take(10) {
                tracing::debug!(
                    target: "determinism",
                    a_idx = event.idx_a,
                    b_idx = event.idx_b,
                    score = event.score,
                    adaptive = self.adaptive_routing,
                    "Tie-break comparison"
                );
            }
        }

        // Emit TieBreakApplied diagnostic events
        if let Some(ref diag) = self.router_diag {
            for event in tie_events.iter() {
                // Determine winner and loser based on stable_id ordering
                let (winner_idx, loser_idx) =
                    if adapter_info[event.idx_a].stable_id < adapter_info[event.idx_b].stable_id {
                        (event.idx_a, event.idx_b)
                    } else {
                        (event.idx_b, event.idx_a)
                    };
                diag.emit_tie_break_applied(
                    step_idx,
                    &adapter_info[winner_idx],
                    &adapter_info[loser_idx],
                    event.score,
                );
            }
        }

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Apply softmax with temperature
        // Softmax with temperature using deterministic f64 + Kahan path
        let mut gates: Vec<f32> = Self::deterministic_softmax(&top_k, self.tau);

        let zero_temperature = self.tau == 0.0;

        if !zero_temperature {
            // Apply entropy floor
            let active_k = gates.len().max(1);
            let min_gate = self.eps / active_k as f32;
            for g in &mut gates {
                *g = g.max(min_gate);
            }
        }

        // Renormalize (guard against zero/empty sum for defensive safety)
        let sum_gates: f32 = gates.iter().sum();
        if sum_gates <= 0.0 || gates.is_empty() {
            tracing::warn!(
                target: "determinism",
                sum_gates = sum_gates,
                gates_len = gates.len(),
                "Gate normalization denominator is zero or gates empty; returning empty decision"
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }
        for g in &mut gates {
            *g /= sum_gates;
        }

        let entropy = Self::compute_entropy(&gates);

        // Check abstain conditions and emit telemetry if triggered
        self.check_abstain_conditions(entropy, &gates);

        let mut candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates.iter())
            .map(|((adapter_idx, raw_score), &gate_f32)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15: quantize_gate(gate_f32),
            })
            .collect();

        Self::sort_candidates_by_quantized_gate(&mut candidate_entries);

        let gates_q15: SmallVec<[i16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.gate_q15)
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        // Update activation history (orthogonality tracking)
        if self.orthogonal_enabled {
            if let Some(ref mut constraints) = self.orthogonal_constraints {
                constraints.update_history(&indices, &gates_q15);
            }
        }

        // Compute decision hash if enabled
        let decision_hash = if self.determinism_config.enable_decision_hashing {
            Some(self.compute_decision_hash(
                features,
                priors_for_routing,
                &indices,
                &gates_q15,
                None, // reasoning_hash
                None, // backend_identity_hash (PRD-DET-001: pass actual hash when available)
            ))
        } else {
            None
        };

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash,
            policy_mask_digest_b3: Some(policy_mask.digest),
            policy_overrides_applied: Some(policy_mask.overrides_applied.clone()),
        };

        // Emit KsparseSelected and RoutingEnd diagnostic events
        if let Some(ref diag) = self.router_diag {
            // Collect adapter info references for selected adapters
            let selected_adapters: Vec<&AdapterInfo> = decision
                .candidates
                .iter()
                .map(|c| &adapter_info[c.adapter_idx as usize])
                .collect();
            let gates_vec: Vec<i16> = decision.gates_q15.iter().copied().collect();

            // Compute decision hash for diagnostics
            // If decision hashing is enabled, use combined_hash from DecisionHash
            // Otherwise compute a simple hash of the output indices and gates
            let diag_decision_hash = match &decision.decision_hash {
                Some(dh) => {
                    // combined_hash is a hex string - hash it to get a B3Hash
                    B3Hash::hash(dh.combined_hash.as_bytes())
                }
                None => {
                    // Compute a fallback hash from indices and gates
                    let mut hash_input = Vec::new();
                    for idx in decision.indices.iter() {
                        hash_input.extend_from_slice(&idx.to_le_bytes());
                    }
                    for gate in decision.gates_q15.iter() {
                        hash_input.extend_from_slice(&gate.to_le_bytes());
                    }
                    B3Hash::hash(&hash_input)
                }
            };

            diag.emit_ksparse_selected(
                step_idx,
                &selected_adapters,
                &gates_vec,
                diag_decision_hash,
            );

            diag.emit_routing_end(
                step_idx,
                decision.candidates.len() as u32,
                diag_decision_hash,
                decision.policy_mask_digest_b3,
            );
        }

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        Ok(decision)
    }

    /// Route based on the model's generated reasoning instead of the input prompt.
    ///
    /// This uses the same weighting logic as `route_with_adapter_info` but derives
    /// features from the generated rationale and applies reasoning_specialties
    /// metadata to bias the priors deterministically.
    pub fn route_on_reasoning(
        &mut self,
        rationale: &str,
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        let features = CodeFeatures::from_context(rationale).to_vector();
        let mut boosted_priors = priors.to_vec();
        self.apply_reasoning_specialty_boost(&mut boosted_priors, rationale, adapter_info);

        let mut decision = self.route_with_adapter_info_and_scope_with_ctx(
            &features,
            &boosted_priors,
            adapter_info,
            policy_mask,
            None,
            determinism,
        )?;

        if self.determinism_config.enable_decision_hashing {
            let reasoning_hash = B3Hash::hash(rationale.as_bytes());
            decision.decision_hash = Some(self.compute_decision_hash(
                &features,
                &boosted_priors,
                &decision.indices,
                &decision.gates_q15,
                Some(&reasoning_hash),
                None, // backend_identity_hash (PRD-DET-001: pass actual hash when available)
            ));
        }

        Ok(decision)
    }

    /// Route with required backend context for V5+ receipt binding.
    ///
    /// This method is the same as `route_with_adapter_info_with_ctx` but requires
    /// a valid `BackendContext` to ensure that routing decisions can be properly
    /// bound to a backend for replay verification.
    ///
    /// # Arguments
    /// * `features` - Global feature vector from prompt/context
    /// * `priors` - Prior scores for each adapter
    /// * `adapter_info` - Metadata for each adapter
    /// * `policy_mask` - Policy mask to filter adapters
    /// * `backend` - Required backend context with backend_id
    /// * `determinism` - Optional determinism context for seeded tie-breaking
    ///
    /// # Returns
    /// * `Ok(Decision)` - The routing decision
    /// * `Err` - If backend validation fails or routing fails
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_lora_router::{BackendContext, Router};
    ///
    /// let backend = BackendContext::new("mlx");
    /// let decision = router.route_with_backend_context(
    ///     &features,
    ///     &priors,
    ///     &adapter_info,
    ///     &policy_mask,
    ///     &backend,
    ///     Some(&determinism_ctx),
    /// )?;
    /// ```
    #[must_use = "routing decisions should be committed to the execution context"]
    pub fn route_with_backend_context(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        backend: &BackendContext,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        // Validate backend context
        backend.validate()?;

        // Perform routing with backend identity hash included in decision hash
        let mut decision = self.route_with_adapter_info_and_scope_with_ctx(
            features,
            priors,
            adapter_info,
            policy_mask,
            None,
            determinism,
        )?;

        // Include backend identity in decision hash if hashing is enabled
        if self.determinism_config.enable_decision_hashing {
            let backend_identity_hash = B3Hash::hash(backend.backend_id.as_bytes());
            decision.decision_hash = Some(self.compute_decision_hash(
                features,
                priors,
                &decision.indices,
                &decision.gates_q15,
                None, // reasoning_hash
                Some(&backend_identity_hash),
            ));
        }

        Ok(decision)
    }

    fn empty_decision_with_mask(policy_mask: &PolicyMask) -> Decision {
        Decision {
            indices: SmallVec::new(),
            gates_q15: SmallVec::new(),
            entropy: 0.0,
            candidates: Vec::new(),
            decision_hash: None,
            policy_mask_digest_b3: Some(policy_mask.digest),
            policy_overrides_applied: Some(policy_mask.overrides_applied.clone()),
        }
    }

    /// Route with k0 detection (no adapters qualify)
    ///
    /// # DEPRECATED - Use route_with_adapter_info() instead
    ///
    /// This method is deprecated in favor of route_with_adapter_info() which provides
    /// better control over adapter selection and k0 detection through proper per-adapter scoring.
    ///
    /// See [ROUTER_MIGRATION.md](../../docs/ROUTER_MIGRATION.md) for migration steps.
    #[deprecated(
        since = "0.1.1",
        note = "Use route_with_adapter_info() for proper k0 detection"
    )]
    pub fn route_with_k0_detection(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        tracing::warn!(
            "Router::route_with_k0_detection() is deprecated, use route_with_adapter_info() instead. \
             See docs/ROUTER_MIGRATION.md for migration guide"
        );
        if let Err(err) = self.validate_router_inputs(features, priors) {
            tracing::warn!(
                error = %err,
                "Router::route_with_k0_detection rejected input due to non-finite values"
            );
            let _ = self.log_k0_event("invalid_numerics", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
            };
        }
        if priors.is_empty() {
            // Log k0 event
            let _ = self.log_k0_event("no_adapters_available", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
            };
        }

        // Compute weighted feature score once
        let feature_score = self.compute_weighted_score(features);

        // Compute scores for each adapter combining prior and features
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
            .map(|(i, &prior)| {
                let score = prior + feature_score;
                (i, score)
            })
            .collect();

        // Check if any adapter qualifies (score > threshold)
        let qualifying_count = scores.iter().filter(|(_, score)| *score > 0.1).count();

        if qualifying_count == 0 {
            // Log k0 event
            let _ = self.log_k0_event("no_adapters_qualify", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest_b3: None,
                policy_overrides_applied: None,
            };
        }

        // Sort by score descending, then by index for determinism
        // Issue D-2 Fix: Use total_cmp for IEEE 754 total ordering (handles NaN deterministically)
        //
        // NOTE: This deprecated function uses array index for tie-breaking, not stable_id.
        // Use route_with_adapter_info() for proper stable_id-based tie-breaking.
        scores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Softmax with temperature (deterministic path)
        let mut gates: Vec<f32> = Self::deterministic_softmax(&top_k, self.tau);
        let zero_temperature = self.tau == 0.0;

        if !zero_temperature {
            // Normalize and apply entropy floor
            let active_k = gates.len().max(1);
            let min_gate = self.eps / active_k as f32;
            for g in &mut gates {
                *g = g.max(min_gate);
            }
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        let entropy = Self::compute_entropy(&gates);

        let mut candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates.iter())
            .map(|((adapter_idx, raw_score), &gate_f32)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15: quantize_gate(gate_f32),
            })
            .collect();

        Self::sort_candidates_by_quantized_gate(&mut candidate_entries);

        let gates_q15: SmallVec<[i16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.gate_q15)
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash: None, // Deprecated method doesn't use decision hashing
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        decision
    }

    /// Log k0 event when no adapters are selected
    fn log_k0_event(&mut self, reason: &str, input: &[f32]) -> Result<()> {
        use adapteros_core::B3Hash;

        // Convert f32 slice to bytes for hashing
        let input_bytes: Vec<u8> = input.iter().flat_map(|&f| f.to_le_bytes()).collect();

        let input_hash = B3Hash::hash(&input_bytes);

        tracing::warn!(
            "Router k0 event: {} (input_hash: {})",
            reason,
            input_hash.to_short_hex()
        );

        Ok(())
    }

    /// Route using code features (convenience method)
    pub fn route_with_code_features(
        &mut self,
        code_features: &CodeFeatures,
        adapter_info: &[AdapterInfo],
    ) -> Result<Decision> {
        // Filter adapters by active stack first
        let allowed_indices = self.filter_by_stack(adapter_info);

        // Generate priors for each adapter based on code features
        let mut priors: Vec<f32> = vec![1.0; adapter_info.len()];

        // Apply framework priors
        for (i, adapter) in adapter_info.iter().enumerate() {
            // Skip if not in active stack
            if !allowed_indices.contains(&i) {
                priors[i] = 0.0; // Zero prior for excluded adapters
                continue;
            }

            if let Some(framework) = &adapter.framework {
                if let Some(&boost) = code_features.framework_prior.get(framework) {
                    priors[i] += boost * 0.5; // Scale framework boost
                }
            }
        }

        // Apply language priors
        let lang_idx = code_features
            .lang_one_hot
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(idx, _)| idx);

        if let Some(lang_idx) = lang_idx {
            for (i, adapter) in adapter_info.iter().enumerate() {
                // Skip if not in active stack
                if !allowed_indices.contains(&i) {
                    continue;
                }

                if adapter.supports_language(lang_idx) {
                    priors[i] += 0.3;
                }
            }
        }

        // Log stack filtering info
        if self.active_stack_name.is_some() {
            tracing::debug!(
                "Stack filtering: {} of {} adapters eligible",
                allowed_indices.len(),
                adapter_info.len()
            );
        }

        // Convert code features to vector
        let feature_vec = code_features.to_vector();

        // Route using the computed priors with adapter info
        let mask = PolicyMask::allow_all(
            &adapter_info
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<_>>(),
            None,
        );
        self.route_with_adapter_info(&feature_vec, &priors, adapter_info, &mask)
    }

    /// Deterministic dry-run routing for a preview/prompt text.
    ///
    /// This extracts code features from the provided text and runs the
    /// canonical router selection without mutating adapter state. It shares
    /// the same determinism guarantees (score DESC, index ASC) as the
    /// production routing path.
    pub fn dry_run(
        &mut self,
        preview_text: &str,
        adapter_info: &[AdapterInfo],
    ) -> Result<Decision> {
        let code_features = CodeFeatures::from_context(preview_text);
        self.route_with_code_features(&code_features, adapter_info)
    }

    /// Get scoring explanation for debugging/audit
    pub fn explain_score(&self, features: &[f32]) -> ScoringExplanation {
        // Accept both 21 (without entropy) and 22 (with entropy) dimensions
        if features.len() != 21 && features.len() != 22 {
            return ScoringExplanation {
                language_score: 0.0,
                framework_score: 0.0,
                symbol_hits_score: 0.0,
                path_tokens_score: 0.0,
                prompt_verb_score: 0.0,
                total_score: features.iter().sum::<f32>() * 0.1,
            };
        }

        let lang_strength = features[0..8].iter().fold(0.0f32, |a, &b| a.max(b));
        let framework_strength = features[8..11].iter().sum::<f32>();
        let symbol_hits = features[11];
        let path_tokens = features[12];
        let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));

        let language_score = lang_strength * self.feature_weights.language_weight;
        let framework_score = framework_strength * self.feature_weights.framework_weight;
        let symbol_hits_score = symbol_hits * self.feature_weights.symbol_hits_weight;
        let path_tokens_score = path_tokens * self.feature_weights.path_tokens_weight;
        let prompt_verb_score = verb_strength * self.feature_weights.prompt_verb_weight;

        ScoringExplanation {
            language_score,
            framework_score,
            symbol_hits_score,
            path_tokens_score,
            prompt_verb_score,
            total_score: language_score
                + framework_score
                + symbol_hits_score
                + path_tokens_score
                + prompt_verb_score,
        }
    }
}
