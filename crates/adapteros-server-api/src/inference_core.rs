//! Unified inference execution core
//!
//! This module provides `InferenceCore` - the ONLY path to execute inference.
//! All handlers (standard, streaming, batch) MUST use this module.
//!
//! # Routing Enforcement
//!
//! The routing guard ensures that all inference requests pass through
//! `route_and_infer()`. Any attempt to call the worker directly without
//! going through this module will result in a hard failure.
//!
//! # RAG Support
//!
//! RAG context retrieval is available on all inference paths (not just streaming).
//! Set `rag_enabled = true` and provide a `rag_collection_id` to enable RAG.
//!
//! # Deterministic Replay
//!
//! Replay uses the same inference path as normal requests. The routing is
//! deterministic by design (sorted by score, then by index for ties) - the
//! `router_seed` field is stored for audit purposes but does not affect
//! routing decisions.
//!
//! For replay, pass a `ReplayContext` to enforce manifest/backend compatibility
//! and skip metadata capture for the replay itself.

use crate::handlers::rag_common::{retrieve_rag_context, store_rag_evidence, RagContextResult};
use crate::state::AppState;
use crate::types::{
    ChunkReference, InferenceError, InferenceRequestInternal, InferenceResult, PlacementReplay,
    PlacementTraceEntry, RagEvidence, ReplayContext, RouterCandidateRecord, RouterDecisionRecord,
    SamplingParams, WorkerInferRequest, MAX_REPLAY_TEXT_SIZE, SAMPLING_ALGORITHM_VERSION,
};
use crate::uds_client::UdsClient;
use adapteros_api_types::inference::ReplayGuarantee;
use adapteros_config::PlacementConfig;
use adapteros_core::{identity::IdentityEnvelope, B3Hash};
use adapteros_db::{chat_sessions::ChatSession, CreateReplayMetadataParams};
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use hex;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, info_span, warn};

// =============================================================================
// Pinned Adapter Helpers (CHAT-PIN-02)
// =============================================================================

/// Parse pinned_adapter_ids JSON string to Vec<String>.
///
/// Returns None if the input is None or if parsing fails (malformed JSON
/// is treated as "no pinned adapters" rather than an error).
pub fn parse_pinned_adapter_ids(json: Option<&str>) -> Option<Vec<String>> {
    json.and_then(|s| serde_json::from_str(s).ok())
}

/// Ensure pinned adapters (if any) are within the effective adapter set when present.
fn validate_pinned_within_effective_set(
    effective_adapter_ids: &Option<Vec<String>>,
    pinned_adapter_ids: &Option<Vec<String>>,
) -> Result<(), InferenceError> {
    if let (Some(effective), Some(pinned)) = (effective_adapter_ids, pinned_adapter_ids) {
        for pinned_id in pinned {
            if !effective.iter().any(|id| id == pinned_id) {
                return Err(InferenceError::ValidationError(format!(
                    "Pinned adapter '{}' is not in effective_adapter_ids: {:?}",
                    pinned_id, effective
                )));
            }
        }
    }
    Ok(())
}

/// Determinism mode hierarchy for inference execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeterminismMode {
    Strict,
    BestEffort,
    Relaxed,
}

impl DeterminismMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::BestEffort => "besteffort",
            Self::Relaxed => "relaxed",
        }
    }
}

impl std::fmt::Display for DeterminismMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for DeterminismMode {
    fn from(value: &str) -> Self {
        let normalized = value.to_ascii_lowercase().replace(['_', '-'], "");
        match normalized.as_str() {
            "strict" => DeterminismMode::Strict,
            "besteffort" => DeterminismMode::BestEffort,
            "relaxed" => DeterminismMode::Relaxed,
            _ => DeterminismMode::Strict, // fail-safe to strict
        }
    }
}

impl std::str::FromStr for DeterminismMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace(['_', '-'], "");
        match normalized.as_str() {
            "strict" => Ok(DeterminismMode::Strict),
            "besteffort" => Ok(DeterminismMode::BestEffort),
            "relaxed" => Ok(DeterminismMode::Relaxed),
            _ => Err(format!(
                "Invalid determinism mode: {} (expected strict, besteffort, relaxed)",
                s
            )),
        }
    }
}

/// Resolve determinism mode using stack > tenant > global precedence
pub fn resolve_determinism_mode(
    stack_mode: Option<&str>,
    tenant_mode: Option<&str>,
    global_mode: &str,
) -> DeterminismMode {
    if let Some(mode) = stack_mode {
        return DeterminismMode::from(mode);
    }
    if let Some(mode) = tenant_mode {
        return DeterminismMode::from(mode);
    }
    DeterminismMode::from(global_mode)
}

/// Compute strict_mode flag for worker/coordinator behavior
pub fn compute_strict_mode(mode: DeterminismMode, allow_fallback: bool) -> bool {
    mode == DeterminismMode::Strict || !allow_fallback
}

/// Validate strict mode requirements (seed required)
pub fn validate_strict_mode_constraints(
    mode: DeterminismMode,
    seed: Option<u64>,
) -> Result<(), InferenceError> {
    if mode == DeterminismMode::Strict && seed.is_none() {
        return Err(InferenceError::ValidationError(
            "Strict determinism mode requires a seed".to_string(),
        ));
    }
    Ok(())
}

/// Compute replay guarantee based on determinism mode and execution path
pub fn compute_replay_guarantee(
    mode: DeterminismMode,
    fallback_triggered: bool,
    prompt_truncated: bool,
    response_truncated: bool,
    seed_present: bool,
) -> ReplayGuarantee {
    match mode {
        DeterminismMode::Strict => {
            if fallback_triggered || prompt_truncated || response_truncated || !seed_present {
                ReplayGuarantee::Approximate
            } else {
                ReplayGuarantee::Exact
            }
        }
        DeterminismMode::BestEffort => ReplayGuarantee::Approximate,
        DeterminismMode::Relaxed => ReplayGuarantee::None,
    }
}

// =============================================================================
// TenantExecutionPolicy Resolution (Bundle E)
// =============================================================================

/// Resolved routing policy knobs
#[derive(Debug, Clone)]
pub struct RoutingPolicyResolved {
    /// Whether to use session's stack_id when no explicit stack is provided
    /// Enforced in resolve_effective_adapters() at line ~848
    pub use_session_stack_for_routing: bool,
    /// Whether pins outside effective set are allowed (always false per Bundle A)
    pub allow_pins_outside_effective_set: bool,
}

impl Default for RoutingPolicyResolved {
    fn default() -> Self {
        Self {
            use_session_stack_for_routing: false,
            allow_pins_outside_effective_set: false,
        }
    }
}

/// Resolved golden-run policy knobs
#[derive(Debug, Clone)]
pub struct GoldenPolicyResolved {
    /// Whether to fail inference when golden drift is detected
    /// Enforced in check_golden_drift() after worker response
    pub fail_on_drift: bool,
    /// Golden baseline ID to compare against (if any)
    pub golden_baseline_id: Option<String>,
    /// Epsilon threshold for floating-point comparison of gate values
    /// Note: Current implementation only checks adapter selection/order
    /// Gate epsilon comparison requires worker to return detailed routing decisions
    pub epsilon_threshold: f64,
}

impl Default for GoldenPolicyResolved {
    fn default() -> Self {
        Self {
            fail_on_drift: false,
            golden_baseline_id: None,
            epsilon_threshold: 1e-6,
        }
    }
}

/// Resolved execution policy combining all policy dimensions
///
/// This struct unifies the policy resolution for a tenant's inference request,
/// combining determinism, routing, and golden-run policies into a single source
/// of truth for the inference path.
///
/// # Policy Enforcement
///
/// All policies are actively enforced during inference:
/// - **Determinism**: Mode and strict_mode enforced at worker call (line ~524)
/// - **Routing**: use_session_stack_for_routing enforced in resolve_effective_adapters() (line ~848)
/// - **Golden**: fail_on_drift enforced in check_golden_drift() after worker response (line ~552)
#[derive(Debug, Clone)]
pub struct ResolvedExecutionPolicy {
    /// The underlying tenant execution policy
    pub policy: adapteros_api_types::TenantExecutionPolicy,
    /// The effective determinism mode after stack > tenant > global resolution
    pub effective_determinism_mode: DeterminismMode,
    /// Whether strict mode is active (for worker/coordinator behavior)
    pub strict_mode: bool,
    /// Resolved routing policy knobs (enforced)
    pub routing: RoutingPolicyResolved,
    /// Resolved golden-run policy knobs (enforced)
    pub golden: GoldenPolicyResolved,
}

/// Resolve execution policy for a tenant's inference request
///
/// Combines:
/// - Database-stored execution policy (determinism, routing, golden)
/// - Config-level defaults (use_session_stack_for_routing, global determinism mode)
/// - Stack-level overrides (determinism_mode on the stack)
///
/// Returns a unified ResolvedExecutionPolicy that can be used throughout the
/// inference path.
pub async fn resolve_tenant_execution_policy(
    db: &adapteros_db::Db,
    config: &crate::state::ApiConfig,
    tenant_id: &str,
    stack_determinism_mode: Option<&str>,
) -> Result<ResolvedExecutionPolicy, InferenceError> {
    // 1. Fetch tenant execution policy (or permissive default)
    let policy = db
        .get_execution_policy_or_default(tenant_id)
        .await
        .map_err(|e| {
            InferenceError::WorkerError(format!("Failed to load execution policy: {}", e))
        })?;

    // 2. Get global determinism mode from config
    let global_mode = config
        .general
        .as_ref()
        .and_then(|g| g.determinism_mode.clone())
        .unwrap_or_else(|| "besteffort".to_string());

    // 3. Resolve determinism mode (stack > tenant > global)
    let effective_determinism_mode = resolve_determinism_mode(
        stack_determinism_mode,
        Some(policy.determinism.default_mode.as_str()),
        global_mode.as_str(),
    );

    // 4. Compute strict mode
    let strict_mode = compute_strict_mode(
        effective_determinism_mode,
        policy.determinism.allow_fallback,
    );

    // 5. Resolve routing policy knobs
    let routing = RoutingPolicyResolved {
        use_session_stack_for_routing: config.use_session_stack_for_routing,
        // Per Bundle A: pins outside effective set are never allowed
        allow_pins_outside_effective_set: false,
    };

    // 6. Resolve golden policy knobs from policy or defaults
    let golden = if let Some(ref golden_policy) = policy.golden {
        GoldenPolicyResolved {
            fail_on_drift: golden_policy.fail_on_drift,
            golden_baseline_id: golden_policy.golden_baseline_id.clone(),
            epsilon_threshold: golden_policy.epsilon_threshold,
        }
    } else {
        GoldenPolicyResolved::default()
    };

    Ok(ResolvedExecutionPolicy {
        policy,
        effective_determinism_mode,
        strict_mode,
        routing,
        golden,
    })
}

/// Inference core that enforces router execution.
///
/// This is the single entry point for all inference operations.
/// All handlers MUST use this struct to execute inference.
pub struct InferenceCore<'a> {
    state: &'a AppState,
}

impl<'a> InferenceCore<'a> {
    /// Create a new InferenceCore instance
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Execute inference through the unified pipeline.
    ///
    /// This is the ONLY function that should be used for inference.
    /// It ensures:
    /// 1. Routing is always executed (in the worker)
    /// 2. RAG context is retrieved if enabled
    /// 3. Evidence is recorded
    /// 4. Session activity is updated
    /// 5. Replay metadata is captured (unless replaying)
    ///
    /// # Arguments
    /// * `request` - The unified internal inference request
    /// * `replay_context` - Optional replay context for deterministic replay
    ///
    /// # Returns
    /// * `Ok(InferenceResult)` - Successful inference result
    /// * `Err(InferenceError)` - Error during inference
    pub async fn route_and_infer(
        &self,
        mut request: InferenceRequestInternal,
        replay_context: Option<ReplayContext>,
    ) -> Result<InferenceResult, InferenceError> {
        let start_time = std::time::Instant::now();
        let is_replay = replay_context.is_some();

        let inference_span = info_span!(
            "inference",
            request_id = %request.request_id,
            tenant_id = %request.cpid,
            stream = request.stream,
            replay = is_replay,
            stack_id = request.stack_id.as_deref().unwrap_or(""),
            model = request.model.as_deref().unwrap_or(""),
            adapters = tracing::field::Empty,
            adapters_used = tracing::field::Empty,
            backend = tracing::field::Empty,
            router_seed = tracing::field::Empty,
            rag_context_hash = tracing::field::Empty,
            session_id = request.session_id.as_deref().unwrap_or("")
        );

        if let Some(seed) = request.router_seed.as_deref() {
            inference_span.record("router_seed", &tracing::field::display(seed));
        }

        let _inference_span_guard = inference_span.enter();

        if let Some(ref ctx) = replay_context {
            info!(
                request_id = %request.request_id,
                original_inference_id = %ctx.original_inference_id,
                required_manifest = %ctx.required_manifest_hash,
                required_backend = %ctx.required_backend,
                "Starting replay inference"
            );
        }

        // 0. Preload chat session (for pinned adapters and optional stack fallback)
        let session = if let Some(ref session_id) = request.session_id {
            match self.state.db.get_chat_session(session_id).await {
                Ok(Some(session)) => {
                    if session.tenant_id != request.cpid {
                        return Err(InferenceError::PermissionDenied(format!(
                            "Session '{}' does not belong to tenant '{}'",
                            session_id, request.cpid
                        )));
                    }
                    Some(session)
                }
                Ok(None) => {
                    debug!(session_id = %session_id, "Session not found for request");
                    None
                }
                Err(e) => {
                    warn!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to fetch session for pinned adapters/stack"
                    );
                    None
                }
            }
        } else {
            None
        };

        // 0.5 Resolve effective adapter set and stack metadata
        self.resolve_effective_adapters(&mut request, session.as_ref())
            .await?;

        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            inference_span.record("adapters", &tracing::field::display(effective.join(",")));
        } else if let Some(adapters) = request.adapters.as_ref() {
            inference_span.record("adapters", &tracing::field::display(adapters.join(",")));
        }

        // 0.6 Resolve execution policy (determinism, routing, golden)
        // Uses the unified resolve_tenant_execution_policy function which handles:
        // - Database-stored execution policy
        // - Config-level defaults
        // - Stack-level overrides
        let config = self
            .state
            .config
            .read()
            .ok()
            .map(|guard| (*guard).clone())
            .unwrap_or_default();

        let resolved_policy = resolve_tenant_execution_policy(
            &self.state.db,
            &config,
            &request.cpid,
            request.stack_determinism_mode.as_deref(),
        )
        .await?;

        // 0.7 Resolve execution profile (seed_mode + backend_profile) and derive request seed
        let seed_mode = request.seed_mode.unwrap_or(config.seed_mode);
        let backend_profile = request.backend_profile.unwrap_or(config.backend_profile);

        let manifest_hash = self
            .state
            .manifest_hash
            .as_deref()
            .and_then(|h| B3Hash::from_hex(h).ok());

        let global_seed = manifest_hash
            .as_ref()
            .cloned()
            .unwrap_or_else(|| B3Hash::hash(b"adapteros-request-global"));

        let determinism_ctx = crate::determinism_context::DeterminismContext::from_request(
            &request,
            manifest_hash.as_ref(),
            &global_seed,
            seed_mode,
            config.worker_id,
        )?;

        request.seed_mode = Some(seed_mode);
        request.backend_profile = Some(backend_profile);
        request.request_seed = Some(determinism_ctx.request_seed());
        request.router_seed = Some(determinism_ctx.router_seed_hex().to_string());
        request.seed = Some(determinism_ctx.request_seed_low64());

        // Validate strict mode constraints (seed required for strict mode)
        validate_strict_mode_constraints(resolved_policy.effective_determinism_mode, request.seed)?;
        request.determinism_mode = Some(
            resolved_policy
                .effective_determinism_mode
                .as_str()
                .to_string(),
        );

        // Extract values for use later in the function
        let resolved_mode = resolved_policy.effective_determinism_mode;
        let strict_mode = resolved_policy.strict_mode;
        let execution_policy = resolved_policy.policy.clone();

        // 0. Validate adapters are loadable (not archived/purged)
        // This is a defense-in-depth check - handlers should also validate
        self.validate_adapters_loadable(&request).await?;

        // 0.1 Gate on base model readiness (cluster-aggregated)
        let base_statuses = self
            .state
            .db
            .list_base_model_statuses()
            .await
            .map_err(|e| {
                InferenceError::WorkerError(format!("Failed to fetch base model status: {}", e))
            })?;

        let filtered: Vec<_> = base_statuses
            .iter()
            .filter(|s| s.tenant_id == request.cpid)
            .filter(|s| {
                if let Some(ref model_id) = request.model {
                    &s.model_id == model_id
                } else {
                    true
                }
            })
            .collect();

        let records: Vec<_> = if filtered.is_empty() {
            base_statuses.iter().collect()
        } else {
            filtered
        };

        let aggregated = crate::model_status::aggregate_status(records.iter().copied());
        if !aggregated.status.is_ready() {
            return Err(InferenceError::ModelNotReady(format!(
                "Base model not ready (status: {})",
                aggregated.status.as_str()
            )));
        }
        let base_model_id = aggregated.latest.map(|s| s.model_id.clone());

        // Set the routing guard - this allows the UDS client to proceed
        crate::uds_client::enter_routed_context();

        // Use scopeguard to ensure we always exit the routed context
        let _guard = scopeguard::guard((), |_| {
            crate::uds_client::exit_routed_context();
        });

        // 1. Resolve worker UDS path
        // For replay: enforce manifest/backend constraints
        // For normal: use standard tenant-based resolution
        let uds_path = if let Some(ref ctx) = replay_context {
            self.resolve_worker_path_for_replay(
                &request.cpid,
                &ctx.required_manifest_hash,
                &ctx.required_backend,
            )
            .await?
        } else {
            self.resolve_worker_path(&request.cpid).await?
        };

        // 2. Retrieve RAG context if enabled
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ RAG Pipeline: Query-time context augmentation                   │
        // │ - Retrieves documents deterministically (score DESC, doc_id ASC)│
        // │ - Evidence stored for replay via rag_snapshot_hash              │
        // │ - RAG provides transient context; adapters provide persistent   │
        // │   behavior. Both can run together in a single inference.        │
        // └─────────────────────────────────────────────────────────────────┘
        let (augmented_prompt, rag_evidence) = if request.rag_enabled {
            self.retrieve_and_augment_rag(&request).await?
        } else {
            (request.prompt.clone(), None)
        };

        if let Some(ref evidence) = rag_evidence {
            inference_span.record(
                "rag_context_hash",
                &tracing::field::display(&evidence.context_hash),
            );
        }

        // 2.5. Resolve pinned adapters from session (CHAT-PIN-02)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Pinned Adapter Pipeline: Session-level preferences              │
        // │ - Pinned adapters receive PINNED_BOOST to their priors          │
        // │ - Non-pinned adapters can still be selected with high scores    │
        // │ - Unavailable pinned adapters are tracked for UI warning        │
        // └─────────────────────────────────────────────────────────────────┘
        let (pinned_adapter_ids, unavailable_pinned_adapters) = if let Some(session) =
            session.as_ref()
        {
            let pinned_ids = if request.pinned_adapter_ids.is_some() {
                request.pinned_adapter_ids.clone()
            } else {
                parse_pinned_adapter_ids(session.pinned_adapter_ids.as_deref())
            };
            (pinned_ids, None::<Vec<String>>)
        } else if let Some(ref session_id) = request.session_id {
            // Fallback: session not preloaded (shouldn't happen, but keep legacy path)
            let pinned_ids = if request.pinned_adapter_ids.is_some() {
                request.pinned_adapter_ids.clone()
            } else {
                match self.state.db.get_chat_session(session_id).await {
                    Ok(Some(session)) => {
                        parse_pinned_adapter_ids(session.pinned_adapter_ids.as_deref())
                    }
                    Ok(None) => {
                        debug!(session_id = %session_id, "Session not found for pinned adapters");
                        None
                    }
                    Err(e) => {
                        warn!(session_id = %session_id, error = ?e, "Failed to fetch session for pinned adapters");
                        None
                    }
                }
            };

            (pinned_ids, None::<Vec<String>>)
        } else {
            (None, None)
        };

        // Enforce pinned adapters must be within effective set (if provided)
        validate_pinned_within_effective_set(&request.effective_adapter_ids, &pinned_adapter_ids)?;

        // 3. Create worker request with full sampling parameters
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Adapter Pipeline: Worker will apply K-sparse routing            │
        // │ - Adapters provide persistent learned behavior (trained weights)│
        // │ - Router selects adapters via deterministic K-sparse gating     │
        // │ - Both RAG context and adapter selection are captured for replay│
        // │ - Pinned adapters receive PINNED_BOOST in the worker (CHAT-PIN-02)│
        // └─────────────────────────────────────────────────────────────────┘
        let worker_request = WorkerInferRequest {
            cpid: request.cpid.clone(),
            prompt: augmented_prompt.clone(),
            max_tokens: request.max_tokens,
            require_evidence: request.require_evidence,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            seed: request.seed,
            router_seed: request.router_seed.clone(),
            seed_mode: request.seed_mode,
            request_seed: request.request_seed,
            backend_profile: request.backend_profile,
            pinned_adapter_ids: pinned_adapter_ids.clone(),
            strict_mode: Some(strict_mode),
            determinism_mode: request.determinism_mode.clone(),
            effective_adapter_ids: request.effective_adapter_ids.clone(),
            routing_policy: execution_policy.routing.clone(),
            placement: None,
        };

        // 4. Call worker via UDS
        // Longer timeout for replay to account for cold worker startup
        let timeout_secs = if is_replay { 120 } else { 60 };
        let uds_client = UdsClient::new(Duration::from_secs(timeout_secs));
        let worker_response = uds_client
            .infer(
                &uds_path,
                worker_request,
                request.worker_auth_token.as_deref(),
            )
            .await
            .map_err(|e| match e {
                crate::uds_client::UdsClientError::WorkerNotAvailable(msg) => {
                    InferenceError::WorkerNotAvailable(msg)
                }
                crate::uds_client::UdsClientError::Timeout(msg) => InferenceError::Timeout(msg),
                crate::uds_client::UdsClientError::RoutingBypass(msg) => {
                    InferenceError::RoutingBypass(msg)
                }
                other => InferenceError::WorkerError(other.to_string()),
            })?;

        // 5. Extract routing decisions from worker response
        let router_decisions = self.extract_router_decisions(&worker_response);
        let router_decision_chain = self.extract_router_decision_chain(&worker_response);

        // 5b. Persist per-token routing chain for audit
        if let Some(chain) = router_decision_chain.as_ref() {
            let records: Vec<adapteros_db::RoutingDecisionChainRecord> = chain
                .iter()
                .map(|entry| {
                    let hash_json = entry
                        .decision_hash
                        .as_ref()
                        .and_then(|h| serde_json::to_string(h).ok());
                    adapteros_db::make_chain_record_from_api(
                        &request.cpid,
                        &request.request_id,
                        Some(&request.request_id),
                        entry,
                        hash_json,
                    )
                })
                .collect();

            if let Err(e) = self
                .state
                .db
                .insert_routing_decision_chain_batch(&records)
                .await
            {
                warn!(
                    error = %e,
                    request_id = %request.request_id,
                    "Failed to persist routing decision chain"
                );
            }
        }

        // 5.5. Golden drift detection (if policy requires it)
        if resolved_policy.golden.fail_on_drift {
            self.check_golden_drift(&resolved_policy.golden, &worker_response)
                .await?;
        }

        // 6. Update session activity if session_id provided
        if let Some(session_id) = &request.session_id {
            self.update_session_activity(session_id, &worker_response)
                .await;
        }

        // 7. Log inference completion
        let latency_ms = start_time.elapsed().as_millis() as u64;
        let response_text = worker_response.text.clone().unwrap_or_default();
        let prompt_truncated = augmented_prompt.len() > MAX_REPLAY_TEXT_SIZE;
        let response_truncated = response_text.len() > MAX_REPLAY_TEXT_SIZE;
        let backend_used = worker_response
            .backend_used
            .clone()
            .or_else(|| self.state.backend_name.clone());
        let fallback_triggered = worker_response.fallback_triggered;

        if let Some(ref backend_name) = backend_used {
            inference_span.record("backend", &tracing::field::display(backend_name));
        }

        let adapters_used_field = worker_response.trace.router_summary.adapters_used.join(",");
        if !adapters_used_field.is_empty() {
            inference_span.record(
                "adapters_used",
                &tracing::field::display(adapters_used_field),
            );
        }

        let determinism_mode_applied = worker_response
            .determinism_mode_applied
            .clone()
            .or_else(|| request.determinism_mode.clone())
            .unwrap_or_else(|| resolved_mode.as_str().to_string());
        let replay_guarantee = compute_replay_guarantee(
            resolved_mode,
            fallback_triggered,
            prompt_truncated,
            response_truncated,
            request.seed.is_some(),
        );

        if is_replay {
            info!(
                request_id = %request.request_id,
                latency_ms = latency_ms,
                adapters_used = ?worker_response.trace.router_summary.adapters_used,
                "Replay inference completed"
            );
        } else {
            info!(
                request_id = %request.request_id,
                latency_ms = latency_ms,
                adapters_used = ?worker_response.trace.router_summary.adapters_used,
                rag_enabled = request.rag_enabled,
                "Inference completed via route_and_infer"
            );
        }

        // 8. Capture replay metadata for deterministic replay
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Combined Evidence: Both RAG and adapter decisions are recorded  │
        // │ - rag_snapshot_hash: BLAKE3 hash of sorted doc hashes           │
        // │ - adapter_ids_json: Adapters specified in request               │
        // │ This enables full audit trail regardless of pipeline path used. │
        // └─────────────────────────────────────────────────────────────────┘
        // Skip for replay-of-replay to avoid recursive metadata creation
        let should_capture = match &replay_context {
            None => true,
            Some(ctx) => !ctx.skip_metadata_capture,
        };

        if should_capture {
            self.capture_replay_metadata(
                &request,
                &augmented_prompt,
                &response_text,
                backend_used.as_deref(),
                backend_used.clone(),
                base_model_id.clone(),
                &rag_evidence,
                latency_ms,
                prompt_truncated,
                response_truncated,
                &determinism_mode_applied,
                fallback_triggered,
                worker_response.placement_trace.as_ref(),
                &replay_guarantee,
                Some(&execution_policy.id),
                Some(execution_policy.version),
            )
            .await;
        } else {
            debug!(
                request_id = %request.request_id,
                "Skipping replay metadata capture (skip_metadata_capture=true)"
            );
        }

        // 9. Get unavailable pinned adapters from worker response
        // The worker computes this by checking pinned IDs against loaded adapters
        let unavailable_pinned = worker_response
            .unavailable_pinned_adapters
            .clone()
            .or(unavailable_pinned_adapters);

        // Compute pinned_routing_fallback based on unavailability
        // - None: All pinned adapters were available (or no pins configured)
        // - "partial": Some pinned adapters unavailable, using available pins + stack
        // - "stack_only": All pinned adapters unavailable, routing uses stack only
        let pinned_routing_fallback = match (&pinned_adapter_ids, &unavailable_pinned) {
            (Some(pinned), Some(unavailable)) if !pinned.is_empty() && !unavailable.is_empty() => {
                if unavailable.len() >= pinned.len() {
                    // All pinned adapters are unavailable
                    Some("stack_only".to_string())
                } else {
                    // Some pinned adapters are unavailable
                    Some("partial".to_string())
                }
            }
            _ => None, // No pins configured or all pins available
        };

        // Get fallback from worker response if available, otherwise use computed value
        let pinned_routing_fallback = worker_response
            .pinned_routing_fallback
            .clone()
            .or(pinned_routing_fallback);

        // Log warning (not debug) when pinned adapters are missing for observability
        if let Some(ref unavailable) = unavailable_pinned {
            let fallback = pinned_routing_fallback.as_deref().unwrap_or("none");
            warn!(
                request_id = %request.request_id,
                cpid = %request.cpid,
                missing_pins = ?unavailable,
                fallback = %fallback,
                pinned_count = pinned_adapter_ids.as_ref().map(|p| p.len()).unwrap_or(0),
                missing_count = unavailable.len(),
                "Pinned adapters unavailable - using fallback routing"
            );

            // Emit structured telemetry event for missing pinned adapters
            let identity = IdentityEnvelope::new(
                request.cpid.clone(),
                "inference_core".to_string(),
                "pinned_adapters_unavailable".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            );
            let pinned_count = pinned_adapter_ids.as_ref().map(|p| p.len()).unwrap_or(0);
            let event_result = TelemetryEventBuilder::new(
                EventType::Custom("inference.pinned_adapters_unavailable".to_string()),
                LogLevel::Warn,
                format!(
                    "{} of {} pinned adapters unavailable - fallback: {}",
                    unavailable.len(),
                    pinned_count,
                    fallback
                ),
                identity,
            )
            .component("inference_core".to_string())
            .metadata(serde_json::json!({
                "request_id": request.request_id,
                "cpid": request.cpid,
                "session_id": request.session_id,
                "pinned_adapter_ids": pinned_adapter_ids,
                "unavailable_pinned_adapters": unavailable,
                "fallback_mode": fallback,
                "latency_ms": latency_ms,
            }))
            .build();

            // Push to telemetry buffer (fire-and-forget, don't block inference)
            if let Ok(event) = event_result {
                let telemetry_buffer = self.state.telemetry_buffer.clone();
                tokio::spawn(async move {
                    if let Err(e) = telemetry_buffer.push(event).await {
                        debug!(error = %e, "Failed to push telemetry event for missing pinned adapters");
                    }
                });
            }
        }

        // 10. Build and return result
        Ok(InferenceResult {
            text: worker_response.text.unwrap_or_default(),
            tokens_generated: 0, // Not tracked in current worker response
            finish_reason: worker_response.status,
            adapters_used: worker_response.trace.router_summary.adapters_used,
            router_decisions,
            router_decision_chain,
            rag_evidence,
            latency_ms,
            request_id: request.request_id,
            unavailable_pinned_adapters: unavailable_pinned,
            pinned_routing_fallback,
            effective_adapter_ids: request.effective_adapter_ids.clone(),
            backend_used,
            fallback_triggered,
            determinism_mode_applied: Some(determinism_mode_applied),
            replay_guarantee: Some(replay_guarantee),
            placement_trace: worker_response.placement_trace.clone(),
        })
    }

    /// Execute inference replay through the unified pipeline
    ///
    /// This is a convenience wrapper for `route_and_infer` with replay context.
    /// It enforces:
    /// - Strict manifest/backend compatibility checking
    /// - Router seed preservation (stored for audit, routing is deterministic)
    /// - Skipping of replay metadata capture (to avoid recursive records)
    ///
    /// # Arguments
    /// * `request` - The inference request with restored sampling params and router_seed
    /// * `replay_context` - Context containing replay constraints and metadata
    ///
    /// # Returns
    /// * `Ok(InferenceResult)` - Successful replay result
    /// * `Err(InferenceError)` - Error during replay (including NoCompatibleWorker)
    pub async fn route_and_infer_replay(
        &self,
        request: InferenceRequestInternal,
        replay_context: ReplayContext,
    ) -> Result<InferenceResult, InferenceError> {
        self.route_and_infer(request, Some(replay_context)).await
    }

    /// Validate that all specified adapters are loadable (not archived/purged)
    ///
    /// This is a defense-in-depth check to prevent inference on archived adapters.
    /// Returns Ok(()) if all adapters are loadable, or an error if any are archived/purged.
    async fn validate_adapters_loadable(
        &self,
        request: &InferenceRequestInternal,
    ) -> Result<(), InferenceError> {
        // Collect adapter IDs with priority: effective set > adapters > adapter_stack
        let adapter_ids: Vec<&str> = if let Some(effective) = request.effective_adapter_ids.as_ref()
        {
            effective.iter().map(|s| s.as_str()).collect()
        } else if let Some(adapters) = request.adapters.as_ref() {
            adapters.iter().map(|s| s.as_str()).collect()
        } else if let Some(stack_list) = request.adapter_stack.as_ref() {
            stack_list.iter().map(|s| s.as_str()).collect()
        } else {
            Vec::new()
        };

        // Check each adapter
        for adapter_id in adapter_ids {
            match self.state.db.is_adapter_loadable(adapter_id).await {
                Ok(true) => continue, // Adapter is loadable
                Ok(false) => {
                    warn!(
                        adapter_id = %adapter_id,
                        request_id = %request.request_id,
                        "Rejected inference request: adapter is archived or purged"
                    );
                    return Err(InferenceError::AdapterNotFound(format!(
                        "Adapter '{}' is archived or purged and cannot be used for inference",
                        adapter_id
                    )));
                }
                Err(e) => {
                    warn!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "Rejected inference request: adapter not found or not loadable"
                    );
                    return Err(InferenceError::AdapterNotFound(format!(
                        "Adapter '{}' is not loadable: {}",
                        adapter_id, e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Resolve the effective adapter set for this request based on priority:
    /// 1) Explicit adapters (or legacy adapter_stack list)
    /// 2) stack_id
    /// 3) session.stack_id when enabled via config
    /// 4) Fallback to manifest (None effective set)
    async fn resolve_effective_adapters(
        &self,
        request: &mut InferenceRequestInternal,
        session: Option<&ChatSession>,
    ) -> Result<(), InferenceError> {
        // 1. Explicit adapters take precedence
        if let Some(adapters) = request.adapters.as_ref() {
            if adapters.is_empty() {
                return Err(InferenceError::ValidationError(
                    "adapters cannot be empty".to_string(),
                ));
            }
            request.effective_adapter_ids = Some(adapters.clone());
            return Ok(());
        }

        // 1b. Legacy adapter_stack acts as explicit adapter list
        if let Some(stack_list) = request.adapter_stack.as_ref() {
            if stack_list.is_empty() {
                return Err(InferenceError::ValidationError(
                    "adapter_stack cannot be empty".to_string(),
                ));
            }
            // Prefer explicit stack_id going forward; adapter_stack is deprecated
            request.effective_adapter_ids = Some(stack_list.clone());
            return Ok(());
        }

        // 2. stack_id provided on request
        let use_session_stack = self
            .state
            .config
            .read()
            .map(|c| c.use_session_stack_for_routing)
            .unwrap_or(false);

        let stack_id_candidate = request.stack_id.clone().or_else(|| {
            if use_session_stack {
                session.and_then(|s| s.stack_id.clone())
            } else {
                None
            }
        });

        if let Some(stack_id) = stack_id_candidate {
            let stack = self
                .state
                .db
                .get_stack(&request.cpid, &stack_id)
                .await
                .map_err(|e| {
                    InferenceError::AdapterNotFound(format!(
                        "Failed to load stack '{}': {}",
                        stack_id, e
                    ))
                })?
                .ok_or_else(|| {
                    InferenceError::AdapterNotFound(format!(
                        "Stack '{}' not found for tenant {}",
                        stack_id, request.cpid
                    ))
                })?;

            let adapter_ids: Vec<String> =
                serde_json::from_str(&stack.adapter_ids_json).map_err(|e| {
                    InferenceError::ValidationError(format!(
                        "Invalid adapter_ids_json for stack {}: {}",
                        stack_id, e
                    ))
                })?;

            if adapter_ids.is_empty() {
                return Err(InferenceError::ValidationError(format!(
                    "Stack '{}' has no adapters configured",
                    stack_id
                )));
            }

            request.stack_id = Some(stack_id);
            request.stack_version = Some(stack.version);
            request.stack_determinism_mode = stack.determinism_mode.clone();
            request.effective_adapter_ids = Some(adapter_ids);
            return Ok(());
        }

        // 3. Tenant default stack fallback (persisted routing configuration)
        if request.stack_id.is_none() {
            if let Some(default_stack_id) = self
                .state
                .db
                .get_default_stack(&request.cpid)
                .await
                .map_err(|e| {
                    InferenceError::AdapterNotFound(format!(
                        "Failed to load default stack for tenant {}: {}",
                        request.cpid, e
                    ))
                })?
            {
                let stack = self
                    .state
                    .db
                    .get_stack(&request.cpid, &default_stack_id)
                    .await
                    .map_err(|e| {
                        InferenceError::AdapterNotFound(format!(
                            "Failed to load default stack '{}': {}",
                            default_stack_id, e
                        ))
                    })?
                    .ok_or_else(|| {
                        InferenceError::AdapterNotFound(format!(
                            "Default stack '{}' not found for tenant {}",
                            default_stack_id, request.cpid
                        ))
                    })?;

                if stack.lifecycle_state.to_ascii_lowercase() != "active" {
                    return Err(InferenceError::AdapterNotFound(format!(
                        "Default stack '{}' is not active (state={})",
                        default_stack_id, stack.lifecycle_state
                    )));
                }

                let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json)
                    .map_err(|e| {
                        InferenceError::ValidationError(format!(
                            "Invalid adapter_ids_json for default stack {}: {}",
                            default_stack_id, e
                        ))
                    })?;

                if adapter_ids.is_empty() {
                    return Err(InferenceError::ValidationError(format!(
                        "Default stack '{}' has no adapters configured",
                        default_stack_id
                    )));
                }

                request.stack_id = Some(default_stack_id.clone());
                request.stack_version = Some(stack.version);
                request.stack_determinism_mode = stack.determinism_mode.clone();
                request.effective_adapter_ids = Some(adapter_ids);

                // Cache active stack mapping for telemetry/routing hints
                if let Ok(mut active) = self.state.active_stack.write() {
                    active.insert(request.cpid.clone(), Some(default_stack_id));
                }

                return Ok(());
            }
        }

        // 4. Fallback: no explicit set -> None (use manifest-wide in worker)
        request.effective_adapter_ids = None;
        request.stack_version = None;
        Ok(())
    }

    /// Resolve the worker UDS path from database or environment
    ///
    /// Uses manifest-based routing to select a compatible worker:
    /// 1. If AppState has a manifest_hash, filters workers by that hash + tenant
    /// 2. Otherwise, gets all serving workers and filters by tenant
    /// 3. Falls back to env override or default socket for dev mode
    ///
    /// This ensures workers only serve requests they're compatible with.
    async fn resolve_worker_path(&self, tenant_id: &str) -> Result<PathBuf, InferenceError> {
        let required_manifest = self.state.manifest_hash.as_deref().ok_or_else(|| {
            InferenceError::NoCompatibleWorker {
                required_hash: "unset".to_string(),
                tenant_id: tenant_id.to_string(),
                available_count: 0,
                reason: "Manifest hash not configured on control plane".to_string(),
            }
        })?;

        // Use manifest-aware worker selection
        let workers = self
            .state
            .db
            .list_compatible_workers_for_tenant(required_manifest, tenant_id)
            .await
            .map_err(|e| {
                InferenceError::WorkerError(format!("Failed to list compatible workers: {}", e))
            })?;

        // Select the best compatible worker (already sorted by latency)
        if let Some(worker) = workers.first() {
            debug!(
                tenant_id = %tenant_id,
                worker_id = %worker.id,
                manifest_hash = %worker.manifest_hash_b3.as_deref().unwrap_or("none"),
                required_manifest = %required_manifest,
                "Selected compatible worker for inference"
            );
            return Ok(PathBuf::from(&worker.uds_path));
        }

        // No compatible worker available - build detailed error
        // Determine the specific reason for failure
        let (all_count, reason) = {
            // Get all serving workers (already filtered by schema version)
            let all_serving = self
                .state
                .db
                .list_serving_workers()
                .await
                .unwrap_or_default();

            if all_serving.is_empty() {
                (
                    0,
                    "No serving workers available (check worker registration and health)"
                        .to_string(),
                )
            } else {
                let manifest_matched = all_serving
                    .iter()
                    .filter(|w| w.manifest_hash_b3.as_deref() == Some(required_manifest))
                    .count();

                if manifest_matched == 0 {
                    (
                        all_serving.len(),
                        format!(
                            "No workers match required manifest hash. {} serving workers exist with different manifests",
                            all_serving.len()
                        ),
                    )
                } else {
                    let tenant_matched = all_serving
                        .iter()
                        .filter(|w| {
                            w.tenant_id == tenant_id
                                && w.manifest_hash_b3.as_deref() == Some(required_manifest)
                        })
                        .count();

                    if tenant_matched == 0 {
                        (
                            all_serving.len(),
                            format!(
                                "{} workers match manifest but none belong to tenant '{}'",
                                manifest_matched, tenant_id
                            ),
                        )
                    } else {
                        (
                            all_serving.len(),
                            "Workers filtered out by schema version incompatibility".to_string(),
                        )
                    }
                }
            }
        };

        Err(InferenceError::NoCompatibleWorker {
            required_hash: required_manifest.to_string(),
            tenant_id: tenant_id.to_string(),
            available_count: all_count,
            reason,
        })
    }

    /// Resolve worker UDS path for replay with manifest/backend constraints
    ///
    /// Unlike `resolve_worker_path()`, this method enforces strict compatibility:
    /// - Current loaded manifest_hash must match required
    /// - Current backend must match required
    ///
    /// If the current AppState matches the requirements, we use standard worker
    /// resolution. If not, we return `InferenceError::NoCompatibleWorker`.
    ///
    /// # Design Note
    ///
    /// We check against AppState rather than querying worker metadata because:
    /// 1. AppState reflects what's actually loaded right now
    /// 2. Worker DB records may not have manifest_hash populated
    /// 3. If AppState matches, all workers in this process can serve the replay
    async fn resolve_worker_path_for_replay(
        &self,
        tenant_id: &str,
        required_manifest_hash: &str,
        required_backend: &str,
    ) -> Result<PathBuf, InferenceError> {
        // Check if current AppState matches the replay requirements
        let current_manifest = self.state.manifest_hash.as_deref().unwrap_or("unknown");
        let current_backend = self.state.backend_name.as_deref().unwrap_or("unknown");

        let manifest_ok = current_manifest == required_manifest_hash;
        let backend_ok = current_backend.eq_ignore_ascii_case(required_backend);

        if !manifest_ok || !backend_ok {
            let reason = if !manifest_ok && !backend_ok {
                format!(
                    "Replay requires manifest={} and backend={}, but current system has manifest={} and backend={}",
                    required_manifest_hash, required_backend, current_manifest, current_backend
                )
            } else if !manifest_ok {
                format!(
                    "Replay requires manifest={}, but current system has manifest={}",
                    required_manifest_hash, current_manifest
                )
            } else {
                format!(
                    "Replay requires backend={}, but current system has backend={}",
                    required_backend, current_backend
                )
            };

            warn!(
                required_manifest = %required_manifest_hash,
                required_backend = %required_backend,
                current_manifest = %current_manifest,
                current_backend = %current_backend,
                manifest_match = manifest_ok,
                backend_match = backend_ok,
                reason = %reason,
                "Current system state incompatible with replay requirements"
            );

            return Err(InferenceError::NoCompatibleWorker {
                required_hash: required_manifest_hash.to_string(),
                tenant_id: tenant_id.to_string(),
                available_count: 0,
                reason,
            });
        }

        debug!(
            tenant_id = %tenant_id,
            manifest_hash = %required_manifest_hash,
            backend = %required_backend,
            "Current system state matches replay requirements, using standard worker resolution"
        );

        // Current state matches - use standard worker resolution
        self.resolve_worker_path(tenant_id).await
    }

    /// Retrieve RAG context and augment the prompt
    ///
    /// Uses the shared rag_common module for deterministic retrieval.
    async fn retrieve_and_augment_rag(
        &self,
        request: &InferenceRequestInternal,
    ) -> Result<(String, Option<RagEvidence>), InferenceError> {
        let collection_id = match &request.rag_collection_id {
            Some(id) => id,
            None => {
                // RAG enabled but no collection - just return original prompt
                debug!(
                    request_id = %request.request_id,
                    "RAG enabled but no collection_id provided, skipping RAG retrieval"
                );
                return Ok((request.prompt.clone(), None));
            }
        };

        // Check if embedding model is available
        let embedding_model = match &self.state.embedding_model {
            Some(model) => model.clone(),
            None => {
                warn!(
                    request_id = %request.request_id,
                    "RAG requested but no embedding model configured"
                );
                return Ok((request.prompt.clone(), None));
            }
        };

        // Use the shared rag_common module for retrieval
        match retrieve_rag_context(
            self.state,
            &request.cpid,
            collection_id,
            &request.prompt,
            embedding_model,
            None, // Use default config
        )
        .await
        {
            Ok(rag_result) => {
                if rag_result.context.is_empty() {
                    Ok((request.prompt.clone(), None))
                } else {
                    // Store evidence (best effort, don't fail inference)
                    let _evidence_ids = store_rag_evidence(
                        self.state,
                        &rag_result,
                        &request.request_id,
                        request.session_id.as_deref(),
                    )
                    .await;

                    // Augment prompt with context
                    let augmented = format!(
                        "Use the following context to answer the question.\n\n\
                         Context:\n{}\n\n\
                         Question: {}",
                        rag_result.context, request.prompt
                    );

                    // Convert RagContextResult to RagEvidence
                    let evidence = self.convert_rag_result_to_evidence(&rag_result);

                    Ok((augmented, Some(evidence)))
                }
            }
            Err(e) => {
                error!(
                    request_id = %request.request_id,
                    error = %e,
                    "RAG context retrieval failed, proceeding without RAG"
                );
                // Don't fail the whole request, just proceed without RAG
                Ok((request.prompt.clone(), None))
            }
        }
    }

    /// Convert RagContextResult from rag_common to our RagEvidence type
    fn convert_rag_result_to_evidence(&self, result: &RagContextResult) -> RagEvidence {
        // Build ChunkReference list from the RagContextResult
        // Use chunk_indices to build full doc IDs with chunk suffix for replay
        let chunks_used: Vec<ChunkReference> = result
            .doc_ids
            .iter()
            .zip(result.chunk_indices.iter())
            .zip(result.scores.iter())
            .enumerate()
            .map(|(rank, ((doc_id, chunk_idx), score))| ChunkReference {
                document_id: doc_id.clone(),
                chunk_id: format!("{}__chunk_{}", doc_id, chunk_idx), // Full ID with chunk suffix
                page_number: None, // Not available in RagContextResult
                relevance_score: *score as f32,
                rank,
            })
            .collect();

        RagEvidence {
            collection_id: result.collection_id.clone(),
            chunks_used,
            context_hash: result.context_hash.clone(),
        }
    }

    /// Extract router decisions from worker response
    ///
    /// Converts the worker's router decisions (if present) into RouterDecisionRecord format
    /// for storage in the control plane's routing_decisions table.
    fn extract_router_decisions(
        &self,
        response: &crate::types::WorkerInferResponse,
    ) -> Vec<RouterDecisionRecord> {
        response
            .trace
            .router_decisions
            .as_ref()
            .map(|decisions| {
                decisions
                    .iter()
                    .map(|d| RouterDecisionRecord {
                        step: d.step,
                        input_token_id: d.input_token_id,
                        candidates: d
                            .candidate_adapters
                            .iter()
                            .map(|c| RouterCandidateRecord {
                                adapter_idx: c.adapter_idx,
                                raw_score: c.raw_score,
                                gate_q15: c.gate_q15,
                            })
                            .collect(),
                        entropy: d.entropy as f64,
                        selected_adapters: response.trace.router_summary.adapters_used.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract chained router decisions (if present) for audit persistence
    fn extract_router_decision_chain(
        &self,
        response: &crate::types::WorkerInferResponse,
    ) -> Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>> {
        response.trace.router_decision_chain.clone()
    }

    /// Check for golden drift by comparing current routing against a golden baseline
    ///
    /// # Golden Drift Detection
    ///
    /// Loads a golden baseline archive and compares the current worker response's
    /// adapter selection against the baseline. Three levels of drift detection:
    ///
    /// 1. **Adapter Selection Changed**: Different adapters selected (CI FAIL)
    /// 2. **Adapter Order Changed**: Same adapters, different order (CI FAIL)
    /// 3. **Gate Epsilon Drift**: Gate values differ beyond threshold (WARN only)
    ///
    /// # Arguments
    /// * `golden_policy` - The resolved golden policy with baseline ID and epsilon threshold
    /// * `worker_response` - The worker's inference response with adapters_used
    ///
    /// # Returns
    /// * `Ok(())` if no drift detected or drift is within policy tolerance
    /// * `Err(InferenceError)` if `fail_on_drift` is true and drift is detected
    ///
    /// # Current Limitation
    ///
    /// Worker response only includes `adapters_used` (adapter IDs), not full gate values.
    /// This limits drift detection to adapter selection/order changes. Gate epsilon
    /// comparison would require worker to return detailed routing decisions with Q15 gates.
    async fn check_golden_drift(
        &self,
        golden_policy: &GoldenPolicyResolved,
        worker_response: &crate::types::WorkerInferResponse,
    ) -> Result<(), InferenceError> {
        let Some(ref baseline_id) = golden_policy.golden_baseline_id else {
            // No baseline configured - skip drift check
            debug!("Golden drift check skipped: no baseline_id configured");
            return Ok(());
        };

        // Get current adapters from worker response
        let current_adapters = &worker_response.trace.router_summary.adapters_used;

        // Load golden baseline from disk/database
        // Note: Currently golden baselines are stored as filesystem archives
        // (see adapteros-verify crate). Future: migrate to database storage.
        let golden_runs_dir = std::path::PathBuf::from("var/golden_runs/baselines");
        let baseline_path = golden_runs_dir.join(baseline_id);

        if !baseline_path.exists() {
            warn!(
                baseline_id = %baseline_id,
                path = %baseline_path.display(),
                "Golden baseline not found - skipping drift check"
            );
            return Ok(());
        }

        // Load the baseline archive
        // Note: This requires the adapteros-verify crate to be available
        // For now, we'll implement a simplified check using only adapter IDs
        let baseline_adapters = match self.load_baseline_adapters(&baseline_path).await {
            Ok(adapters) => adapters,
            Err(e) => {
                warn!(
                    baseline_id = %baseline_id,
                    error = %e,
                    "Failed to load golden baseline - skipping drift check"
                );
                return Ok(());
            }
        };

        // Compare adapter selection
        let current_set: std::collections::HashSet<_> = current_adapters.iter().collect();
        let baseline_set: std::collections::HashSet<_> = baseline_adapters.iter().collect();

        let adapters_added: Vec<_> = current_set.difference(&baseline_set).collect();
        let adapters_removed: Vec<_> = baseline_set.difference(&current_set).collect();

        let selection_changed = !adapters_added.is_empty() || !adapters_removed.is_empty();
        let order_changed = !selection_changed && current_adapters != &baseline_adapters;

        if selection_changed || order_changed {
            let drift_msg = if selection_changed {
                format!(
                    "Golden drift detected: adapter selection changed. Added: {:?}, Removed: {:?}",
                    adapters_added, adapters_removed
                )
            } else {
                format!(
                    "Golden drift detected: adapter order changed. Baseline: {:?}, Current: {:?}",
                    baseline_adapters, current_adapters
                )
            };

            warn!(
                baseline_id = %baseline_id,
                current_adapters = ?current_adapters,
                baseline_adapters = ?baseline_adapters,
                selection_changed = selection_changed,
                order_changed = order_changed,
                "{}",
                drift_msg
            );

            if golden_policy.fail_on_drift {
                return Err(InferenceError::ValidationError(drift_msg));
            }
        } else {
            debug!(
                baseline_id = %baseline_id,
                adapters = ?current_adapters,
                "Golden drift check passed - adapters match baseline"
            );
        }

        Ok(())
    }

    /// Load baseline adapters from a golden run archive
    ///
    /// Reads the manifest.json or routing_decisions.json from the baseline directory
    /// to extract the expected adapter list.
    ///
    /// # Arguments
    /// * `baseline_path` - Path to the golden baseline directory
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - List of adapter IDs from the baseline
    /// * `Err(InferenceError)` - If baseline cannot be loaded
    async fn load_baseline_adapters(
        &self,
        baseline_path: &std::path::Path,
    ) -> Result<Vec<String>, InferenceError> {
        // Try to load from manifest.json first
        let manifest_path = baseline_path.join("manifest.json");
        if manifest_path.exists() {
            let manifest_content =
                tokio::fs::read_to_string(&manifest_path)
                    .await
                    .map_err(|e| {
                        InferenceError::WorkerError(format!(
                            "Failed to read baseline manifest: {}",
                            e
                        ))
                    })?;

            // Parse manifest to extract adapter list
            // Manifest structure: { "adapters": [...], ... }
            let manifest: serde_json::Value =
                serde_json::from_str(&manifest_content).map_err(|e| {
                    InferenceError::WorkerError(format!("Failed to parse baseline manifest: {}", e))
                })?;

            if let Some(adapters) = manifest.get("adapters").and_then(|a| a.as_array()) {
                let adapter_ids: Vec<String> = adapters
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();

                if !adapter_ids.is_empty() {
                    return Ok(adapter_ids);
                }
            }
        }

        // Fallback: try to load from routing_decisions.json
        let routing_path = baseline_path.join("routing_decisions.json");
        if routing_path.exists() {
            let routing_content = tokio::fs::read_to_string(&routing_path)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!(
                        "Failed to read baseline routing decisions: {}",
                        e
                    ))
                })?;

            // Parse routing decisions to extract unique adapters
            // Routing decisions structure: [{ "candidate_adapters": [...], ... }, ...]
            let decisions: Vec<serde_json::Value> = serde_json::from_str(&routing_content)
                .map_err(|e| {
                    InferenceError::WorkerError(format!(
                        "Failed to parse baseline routing decisions: {}",
                        e
                    ))
                })?;

            // Extract unique adapter indices/names from all routing decisions
            let mut adapter_set = std::collections::HashSet::new();
            for decision in decisions {
                if let Some(candidates) = decision
                    .get("candidate_adapters")
                    .and_then(|c| c.as_array())
                {
                    for candidate in candidates {
                        if let Some(idx) = candidate.get("adapter_idx").and_then(|i| i.as_u64()) {
                            // Store as string index (adapter names not available in routing decisions)
                            adapter_set.insert(format!("adapter-{}", idx));
                        }
                    }
                }
            }

            if !adapter_set.is_empty() {
                let mut adapters: Vec<String> = adapter_set.into_iter().collect();
                adapters.sort(); // Ensure deterministic order
                return Ok(adapters);
            }
        }

        Err(InferenceError::WorkerError(format!(
            "No adapter list found in golden baseline at {}",
            baseline_path.display()
        )))
    }

    /// Update session activity and link adapters
    async fn update_session_activity(
        &self,
        session_id: &str,
        response: &crate::types::WorkerInferResponse,
    ) {
        // Link adapters used to session
        for adapter_id in &response.trace.router_summary.adapters_used {
            if let Err(e) = self
                .state
                .db
                .add_session_trace(session_id, "adapter", adapter_id)
                .await
            {
                warn!(
                    session_id = %session_id,
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to link adapter trace to session"
                );
            }
        }

        // Update session activity timestamp
        if let Err(e) = crate::security::update_session_activity(&self.state.db, session_id).await {
            warn!(
                session_id = %session_id,
                error = %e,
                "Failed to update session activity"
            );
        }
    }

    /// Capture replay metadata for deterministic replay
    ///
    /// Stores all parameters needed to replay this inference exactly:
    /// - Sampling parameters (temperature, top_k, top_p, max_tokens, seed)
    /// - Router seed (stored for audit; routing is deterministic by design)
    /// - RAG document IDs and snapshot hash
    /// - Prompt and response text (truncated to 64KB with flags)
    ///
    /// This is called after every successful inference to enable replay.
    /// Failures are logged but don't fail the inference.
    ///
    /// # Routing Determinism Note
    ///
    /// The router uses a deterministic algorithm (sorted by score, then by index
    /// for tie-breaking). The `router_seed` is stored for audit purposes but
    /// does not currently affect routing decisions. This means replays will
    /// produce identical routing given identical inputs and model state.
    async fn capture_replay_metadata(
        &self,
        request: &InferenceRequestInternal,
        prompt_text: &str,
        response_text: &str,
        backend_used: Option<&str>,
        rag_evidence: &Option<RagEvidence>,
        latency_ms: u64,
        prompt_truncated: bool,
        response_truncated: bool,
        determinism_mode: &str,
        fallback_triggered: bool,
        placement_trace: Option<&Vec<PlacementTraceEntry>>,
        replay_guarantee: &ReplayGuarantee,
        execution_policy_id: Option<&str>,
        execution_policy_version: Option<i64>,
    ) {
        // Get manifest hash from state (current loaded manifest)
        let manifest_hash = self
            .state
            .manifest_hash
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        // Get backend from state
        let backend = backend_used
            .map(|s| s.to_string())
            .or_else(|| self.state.backend_name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let replay_guarantee_str = match replay_guarantee {
            ReplayGuarantee::Exact => "exact",
            ReplayGuarantee::Approximate => "approximate",
            ReplayGuarantee::None => "none",
        };

        let placement_replay = placement_trace.map(|trace| {
            let cfg = PlacementConfig::from_env();
            let mode_str = match cfg.mode {
                adapteros_config::PlacementMode::Balanced => "balanced",
                adapteros_config::PlacementMode::Latency => "latency",
                adapteros_config::PlacementMode::Energy => "energy",
                adapteros_config::PlacementMode::Thermal => "thermal",
                adapteros_config::PlacementMode::Off => "off",
            };
            PlacementReplay {
                mode: mode_str.to_string(),
                weights: cfg.weights.into(),
                trace: trace.clone(),
            }
        });

        // Build sampling params JSON
        let sampling_params = SamplingParams {
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            seed: request.seed,
            seed_mode: request.seed_mode,
            backend_profile: request.backend_profile,
            request_seed_hex: request.request_seed.as_ref().map(|s| hex::encode(s)),
            placement: placement_replay.or_else(|| {
                let cfg = PlacementConfig::from_env();
                let mode_str = match cfg.mode {
                    adapteros_config::PlacementMode::Balanced => "balanced",
                    adapteros_config::PlacementMode::Latency => "latency",
                    adapteros_config::PlacementMode::Energy => "energy",
                    adapteros_config::PlacementMode::Thermal => "thermal",
                    adapteros_config::PlacementMode::Off => "off",
                };
                Some(PlacementReplay {
                    mode: mode_str.to_string(),
                    weights: cfg.weights.into(),
                    trace: Vec::new(),
                })
            }),
        };
        let sampling_params_json = serde_json::to_string(&sampling_params).unwrap_or_default();

        // Compute RAG snapshot hash and extract doc IDs with chunk indices
        let (rag_snapshot_hash, rag_doc_ids) = if let Some(evidence) = rag_evidence {
            // Use chunk_id which contains the full "{document_id}__chunk_{index}" format
            let doc_ids: Vec<String> = evidence
                .chunks_used
                .iter()
                .map(|c| c.chunk_id.clone())
                .collect();
            // Use the context_hash if available, otherwise compute from doc IDs
            let hash = if !evidence.context_hash.is_empty() {
                Some(evidence.context_hash.clone())
            } else if !doc_ids.is_empty() {
                // Simple hash of sorted doc IDs
                let mut sorted = doc_ids.clone();
                sorted.sort();
                Some(
                    blake3::hash(sorted.join(",").as_bytes())
                        .to_hex()
                        .to_string(),
                )
            } else {
                None
            };
            (hash, Some(doc_ids))
        } else {
            (None, None)
        };

        // Truncate prompt and response if needed
        let prompt_for_storage = if prompt_truncated {
            prompt_text.chars().take(MAX_REPLAY_TEXT_SIZE).collect()
        } else {
            prompt_text.to_string()
        };
        let response_for_storage = if response_truncated {
            response_text.chars().take(MAX_REPLAY_TEXT_SIZE).collect()
        } else {
            response_text.to_string()
        };

        // Get adapter IDs from effective set (preferred) or requested adapters
        let adapter_ids = request
            .effective_adapter_ids
            .clone()
            .or_else(|| request.adapters.clone());

        // Determine replay status based on truncation
        let replay_status = if prompt_truncated || response_truncated {
            "approximate"
        } else {
            "available"
        };

        // Estimate tokens (rough: ~4 chars per token)
        let tokens_generated = Some((response_text.len() / 4).max(1) as i32);

        // Build params for DB storage
        let params = CreateReplayMetadataParams {
            inference_id: request.request_id.clone(),
            tenant_id: request.cpid.clone(),
            manifest_hash,
            base_model_id: base_model_id.clone(),
            router_seed: request.router_seed.clone(),
            sampling_params_json,
            backend,
            backend_version: backend_used.clone(),
            sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
            rag_snapshot_hash,
            adapter_ids,
            prompt_text: prompt_for_storage,
            prompt_truncated,
            response_text: Some(response_for_storage),
            response_truncated,
            rag_doc_ids,
            chat_context_hash: request.chat_context_hash.clone(),
            replay_status: Some(replay_status.to_string()),
            latency_ms: Some(latency_ms as i32),
            tokens_generated,
            determinism_mode: Some(determinism_mode.to_string()),
            fallback_triggered,
            replay_guarantee: Some(replay_guarantee_str.to_string()),
            execution_policy_id: execution_policy_id.map(|s| s.to_string()),
            execution_policy_version: execution_policy_version.map(|v| v as i32),
        };

        // Store to database (best effort - don't fail inference on capture error)
        if let Err(e) = self.state.db.create_replay_metadata(params).await {
            warn!(
                request_id = %request.request_id,
                error = %e,
                "Failed to capture replay metadata"
            );
        } else {
            debug!(
                request_id = %request.request_id,
                "Replay metadata captured successfully"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PathsConfig;
    use crate::state::{ApiConfig, MetricsConfig};
    use crate::telemetry::MetricsRegistry;
    use adapteros_core::{BackendProfile, SeedMode};
    use adapteros_db::chat_sessions::CreateChatSessionParams;
    use adapteros_db::traits::CreateStackRequest;
    use adapteros_db::Db;
    use adapteros_metrics_exporter::MetricsExporter;
    use adapteros_telemetry::MetricsCollector;
    use std::fs;
    use std::sync::{Arc, RwLock};
    use tempfile::Builder as TempDirBuilder;
    use uuid::Uuid;

    fn stack_name() -> String {
        format!("stack.test.{}", Uuid::new_v4().simple())
    }

    #[test]
    fn test_replay_context_structure() {
        // Verify ReplayContext has all required fields
        let ctx = ReplayContext {
            original_inference_id: "test-123".to_string(),
            required_manifest_hash: "abc123".to_string(),
            required_backend: "mlx".to_string(),
            skip_metadata_capture: true,
            original_policy_id: None,
            original_policy_version: None,
        };
        assert!(ctx.skip_metadata_capture);
        assert_eq!(ctx.original_inference_id, "test-123");
        assert_eq!(ctx.required_manifest_hash, "abc123");
        assert_eq!(ctx.required_backend, "mlx");
    }

    #[test]
    fn test_replay_context_for_normal_inference() {
        // Normal inference should not skip metadata capture
        let ctx = ReplayContext {
            original_inference_id: "original-001".to_string(),
            required_manifest_hash: "manifest-hash".to_string(),
            required_backend: "CoreML".to_string(),
            skip_metadata_capture: false,
            original_policy_id: None,
            original_policy_version: None,
        };
        assert!(!ctx.skip_metadata_capture);
    }

    async fn insert_stack(db: &Db, tenant: &str, adapter_ids: &[&str]) -> String {
        let req = CreateStackRequest {
            tenant_id: tenant.to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: adapter_ids.iter().map(|s| s.to_string()).collect(),
            workflow_type: None,
            determinism_mode: None,
        };
        db.insert_stack(&req).await.expect("insert stack")
    }

    #[tokio::test]
    async fn test_resolve_effective_adapters_adapters_only() {
        let state = build_test_state(false).await;
        let core = InferenceCore::new(&state);
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
        req.adapters = Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]);

        core.resolve_effective_adapters(&mut req, None)
            .await
            .expect("resolve");

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
        );
        assert!(req.stack_id.is_none());
    }

    #[tokio::test]
    async fn test_resolve_effective_adapters_stack_only() {
        let state = build_test_state(false).await;
        let stack_id = insert_stack(&state.db, "tenant-1", &["adapter-a", "adapter-c"]).await;
        let core = InferenceCore::new(&state);
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
        req.stack_id = Some(stack_id.clone());

        core.resolve_effective_adapters(&mut req, None)
            .await
            .expect("resolve");

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["adapter-a".to_string(), "adapter-c".to_string()])
        );
        assert_eq!(req.stack_id, Some(stack_id.clone()));
        assert!(req.stack_version.is_some());
    }

    #[tokio::test]
    async fn test_session_stack_fallback_disabled() {
        let state = build_test_state(false).await;
        let session = adapteros_db::chat_sessions::ChatSession {
            id: "s1".to_string(),
            tenant_id: "tenant-1".to_string(),
            user_id: None,
            created_by: None,
            stack_id: Some("stack-session".to_string()),
            collection_id: None,
            document_id: None,
            name: "test".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            last_activity_at: "now".to_string(),
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
        };
        let core = InferenceCore::new(&state);
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
        req.session_id = Some(session.id.clone());

        core.resolve_effective_adapters(&mut req, Some(&session))
            .await
            .expect("resolve");

        assert!(
            req.effective_adapter_ids.is_none(),
            "fallback disabled should not use session stack"
        );
    }

    #[tokio::test]
    async fn test_session_stack_fallback_enabled() {
        let state = build_test_state(true).await;
        let stack_id = insert_stack(&state.db, "tenant-1", &["adapter-a", "adapter-c"]).await;
        let session = adapteros_db::chat_sessions::ChatSession {
            id: "s1".to_string(),
            tenant_id: "tenant-1".to_string(),
            user_id: None,
            created_by: None,
            stack_id: Some(stack_id.clone()),
            collection_id: None,
            document_id: None,
            name: "test".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            last_activity_at: "now".to_string(),
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
        };
        let core = InferenceCore::new(&state);
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
        req.session_id = Some(session.id.clone());

        core.resolve_effective_adapters(&mut req, Some(&session))
            .await
            .expect("resolve");

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["adapter-a".to_string(), "adapter-c".to_string()])
        );
        assert_eq!(req.stack_id, Some(stack_id));
    }

    #[test]
    fn test_sampling_params_serialization() {
        let params = SamplingParams {
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.9),
            max_tokens: 100,
            seed: Some(42),
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"seed\":42"));
        assert!(json.contains("\"top_k\":50"));
        assert!(json.contains("\"top_p\":0.9"));
        assert!(json.contains("\"max_tokens\":100"));

        // Verify round-trip
        let parsed: SamplingParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.temperature, 0.7);
        assert_eq!(parsed.seed, Some(42));
        assert_eq!(parsed.top_k, Some(50));
        assert_eq!(parsed.top_p, Some(0.9));
        assert_eq!(parsed.max_tokens, 100);
    }

    #[test]
    fn test_sampling_params_default_values() {
        // Test that default values work correctly
        let params = SamplingParams {
            temperature: 1.0,
            top_k: None,
            top_p: None,
            max_tokens: 256,
            seed: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
        };
        let json = serde_json::to_string(&params).unwrap();

        // None values should serialize as null
        let parsed: SamplingParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.top_k, None);
        assert_eq!(parsed.top_p, None);
        assert_eq!(parsed.seed, None);
    }

    #[test]
    fn test_sampling_params_greedy_decoding() {
        // Temperature 0 means greedy decoding
        let params = SamplingParams {
            temperature: 0.0,
            top_k: None,
            top_p: None,
            max_tokens: 100,
            seed: Some(0), // Seed still matters for tie-breaking
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
        };
        assert_eq!(params.temperature, 0.0);

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"temperature\":0.0"));
    }

    #[test]
    fn test_backend_comparison_case_insensitive() {
        // Backend comparison should be case-insensitive
        let required = "CoreML";
        let current = "coreml";
        assert!(current.eq_ignore_ascii_case(required));

        let required = "MLX";
        let current = "mlx";
        assert!(current.eq_ignore_ascii_case(required));
    }

    // =========================================================================
    // Effective adapter set resolution tests (bundle A)
    // =========================================================================
    async fn build_test_state(use_session_stack: bool) -> AppState {
        let base = std::path::Path::new("var/test-dbs");
        fs::create_dir_all(base).unwrap();
        let dir = TempDirBuilder::new()
            .prefix("aos-inference-core-")
            .tempdir_in(base)
            .unwrap();
        let db_path = dir.path().join("db.sqlite3");
        let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        // Keep the tempdir alive for the lifetime of the test database
        let _db_dir = dir.into_path();
        // Seed tenant
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: true,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: use_session_stack,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            performance: Default::default(),
            paths: PathsConfig {
                artifacts_root: "var/artifacts".into(),
                bundles_root: "var/bundles".into(),
                adapters_root: "var/adapters/repo".into(),
                plan_dir: "var/plan".into(),
                datasets_root: "var/datasets".into(),
                documents_root: "var/documents".into(),
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendProfile::AutoDev,
            worker_id: 0,
        }));

        let metrics_exporter = Arc::new(MetricsExporter::new(vec![0.1]).unwrap());
        let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let uma_monitor = Arc::new(adapteros_lora_worker::memory::UmaPressureMonitor::new(
            15, None,
        ));

        AppState::new(
            db,
            b"test-jwt-secret-for-effective-adapters".to_vec(),
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        )
        .with_manifest_info("test-manifest-hash".to_string(), "mlx".to_string())
    }

    #[tokio::test]
    async fn test_effective_adapters_explicit_list() {
        let state = build_test_state(false).await;
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.adapters = Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]);

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
        );
        assert!(req.stack_id.is_none());
    }

    #[tokio::test]
    async fn resolve_worker_path_requires_manifest_hash() {
        let state = build_test_state(false).await;
        let core = InferenceCore::new(&state);
        let err = core.resolve_worker_path("tenant-1").await.unwrap_err();
        match err {
            InferenceError::NoCompatibleWorker { required_hash, .. } => {
                assert_eq!(required_hash, "test-manifest-hash")
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_effective_adapters_from_stack_id() {
        let state = build_test_state(false).await;
        let stack_req = CreateStackRequest {
            tenant_id: "tenant-1".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["stack-a".to_string(), "stack-b".to_string()],
            workflow_type: None,
            determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.stack_id = Some(stack_id.clone());

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["stack-a".to_string(), "stack-b".to_string()])
        );
        assert_eq!(req.stack_id, Some(stack_id));
        assert_eq!(req.stack_version, Some(1));
    }

    #[tokio::test]
    async fn test_effective_adapters_default_stack_fallback() {
        let state = build_test_state(false).await;
        let stack_req = CreateStackRequest {
            tenant_id: "tenant-1".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["default-a".to_string(), "default-b".to_string()],
            workflow_type: None,
            determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();
        state
            .db
            .set_default_stack("tenant-1", &stack_id)
            .await
            .unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["default-a".to_string(), "default-b".to_string()])
        );
        assert_eq!(req.stack_id, Some(stack_id.clone()));
        assert_eq!(req.stack_version, Some(1));

        // Active stack cache should be populated for the tenant
        let active_map = state.active_stack.read().unwrap();
        assert_eq!(
            active_map.get("tenant-1").cloned().flatten(),
            Some(stack_id.clone())
        );
    }

    #[tokio::test]
    async fn test_stack_with_pinned_adapters_subset_allowed() {
        let state = build_test_state(false).await;
        let stack_req = CreateStackRequest {
            tenant_id: "tenant-1".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["stack-a".to_string(), "stack-b".to_string()],
            workflow_type: None,
            determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.stack_id = Some(stack_id);
        req.pinned_adapter_ids = Some(vec!["stack-b".to_string()]);

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        // Pinned adapter is part of the resolved effective set, so validation should pass
        validate_pinned_within_effective_set(&req.effective_adapter_ids, &req.pinned_adapter_ids)
            .expect("pinned adapters should be allowed");
    }

    #[tokio::test]
    async fn test_effective_adapters_from_session_stack_when_enabled() {
        let state = build_test_state(true).await;
        let stack_req = CreateStackRequest {
            tenant_id: "tenant-1".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["s1".to_string(), "s2".to_string()],
            workflow_type: None,
            determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

        // Create a session that references the stack
        let session_id = "session-1".to_string();
        let session_params = CreateChatSessionParams {
            id: session_id.clone(),
            tenant_id: "tenant-1".to_string(),
            user_id: None,
            created_by: None,
            stack_id: Some(stack_id.clone()),
            collection_id: None,
            document_id: None,
            name: "test".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
        };
        state.db.create_chat_session(session_params).await.unwrap();
        let session = state
            .db
            .get_chat_session(&session_id)
            .await
            .unwrap()
            .unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.session_id = Some(session_id.clone());

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, Some(&session))
            .await
            .unwrap();

        assert_eq!(
            req.effective_adapter_ids,
            Some(vec!["s1".to_string(), "s2".to_string()])
        );
        assert_eq!(req.stack_id, Some(stack_id));
    }

    #[tokio::test]
    async fn test_session_stack_ignored_when_disabled() {
        let state = build_test_state(false).await;
        let stack_req = CreateStackRequest {
            tenant_id: "tenant-1".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["s1".to_string()],
            workflow_type: None,
            determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

        let session_id = "session-2".to_string();
        let session_params = CreateChatSessionParams {
            id: session_id.clone(),
            tenant_id: "tenant-1".to_string(),
            user_id: None,
            created_by: None,
            stack_id: Some(stack_id),
            collection_id: None,
            document_id: None,
            name: "test".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
        };
        state.db.create_chat_session(session_params).await.unwrap();
        let session = state
            .db
            .get_chat_session(&session_id)
            .await
            .unwrap()
            .unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.session_id = Some(session_id);

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, Some(&session))
            .await
            .unwrap();

        // Without the flag, we should not inherit session.stack_id
        assert!(req.effective_adapter_ids.is_none());
        assert!(req.stack_id.is_none());
    }

    #[tokio::test]
    async fn test_pinned_not_in_effective_set_rejected_in_core() {
        let state = build_test_state(false).await;
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.adapters = Some(vec!["adapter-a".to_string()]);
        req.pinned_adapter_ids = Some(vec!["adapter-b".to_string()]);

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        let err = validate_pinned_within_effective_set(
            &req.effective_adapter_ids,
            &req.pinned_adapter_ids,
        )
        .expect_err("pinned adapter not in effective set should be rejected");

        match err {
            InferenceError::ValidationError(msg) => {
                assert!(
                    msg.contains("adapter-b"),
                    "error message should name the pinned adapter: {}",
                    msg
                );
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bad_adapter_id_rejected() {
        let state = build_test_state(false).await;
        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.adapters = Some(vec!["missing-adapter".to_string()]);

        let core = InferenceCore::new(&state);
        core.resolve_effective_adapters(&mut req, None)
            .await
            .unwrap();

        let err = core.validate_adapters_loadable(&req).await.unwrap_err();
        match err {
            InferenceError::AdapterNotFound(msg) => {
                assert!(msg.contains("missing-adapter"));
            }
            other => panic!("expected AdapterNotFound, got {:?}", other),
        }
    }
}
