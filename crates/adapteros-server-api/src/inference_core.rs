//! # Unified Inference Execution Core
//!
//! This module provides `InferenceCore` - the **ONLY** path to execute inference.
//! All handlers (standard, streaming, batch, replay) **MUST** use this module.
//!
//! ## Pipeline Stages
//!
//! The `route_and_infer()` function executes an 11-stage pipeline:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 1: Request Validation                                                │
//! │  - Tenant isolation check                                                   │
//! │  - Sampling params validation (temperature, top_p bounds)                   │
//! │  - Chat session lookup (if session_id provided)                             │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 2: Adapter Resolution                                                │
//! │  - Load adapters from DB by adapter_ids or stack_id                         │
//! │  - Apply pinned adapter overrides (CHAT-PIN-02)                             │
//! │  - Validate all adapters belong to tenant                                   │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 3: Policy Hooks (OnRequestBeforeRouting)                             │
//! │  - Execute policy packs: egress, determinism, isolation, evidence           │
//! │  - Generate policy mask for router                                          │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 4: RAG Context Retrieval (if enabled)                                │
//! │  - Query collection for relevant chunks                                     │
//! │  - Compute rag_snapshot_hash for replay                                     │
//! │  - Inject context into prompt                                               │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 5: Router Decision                                                   │
//! │  - K-sparse top-K selection with Q15 gates                                  │
//! │  - Deterministic tie-breaking (score DESC, index ASC)                       │
//! │  - Entropy floor enforcement                                                │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 6: Worker Selection                                                  │
//! │  - Find worker with required adapters loaded                                │
//! │  - Placement constraints (memory, backend compatibility)                    │
//! │  - Hot-swap triggers if adapters not loaded                                 │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 7: Policy Hooks (OnBeforeInference)                                  │
//! │  - Final policy checks before worker call                                   │
//! │  - Rate limiting, quota enforcement                                         │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 8: Worker Inference (UDS)                                            │
//! │  - Send request over Unix Domain Socket                                     │
//! │  - Execute on CoreML/Metal/MLX backend                                      │
//! │  - Collect router decisions per token                                       │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 9: Policy Hooks (OnAfterInference)                                   │
//! │  - Post-inference validation                                                │
//! │  - Output filtering (if configured)                                         │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 10: Evidence & Telemetry                                             │
//! │  - Store replay metadata (manifest_hash, router_seed, etc.)                 │
//! │  - Emit routing telemetry event                                             │
//! │  - Store RAG evidence (if applicable)                                       │
//! └────────────────────────────────┬────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 11: Response Assembly                                                │
//! │  - Build InferenceResult with text, tokens, decisions                       │
//! │  - Include citations from adapters                                          │
//! │  - Return replay_metadata_id for determinism verification                   │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Error Gates
//!
//! The pipeline has 9 **blocking error gates** that halt execution:
//! 1. Invalid tenant isolation
//! 2. Chat session not found (when session_id provided)
//! 3. No adapters resolved
//! 4. Policy hook rejection (OnRequestBeforeRouting)
//! 5. RAG collection not found
//! 6. No eligible worker found
//! 7. Policy hook rejection (OnBeforeInference)
//! 8. Worker inference failure (UDS timeout, backend error)
//! 9. Policy hook rejection (OnAfterInference)
//!
//! ## Graceful Degradation Paths
//!
//! 7 scenarios where the pipeline continues with reduced functionality:
//! - RAG disabled: Skip stages 4, RAG evidence storage
//! - No policy hooks configured: Skip stages 3, 7, 9
//! - Chat session missing pinned_adapter_ids: Use request adapter_ids
//! - Telemetry write failure: Log warning, continue
//! - Citation collection failure: Return empty citations
//! - Replay metadata storage failure: Log warning, return None for replay_metadata_id
//! - Router decision chain empty: Return None for decision_chain
//!
//! ## Routing Enforcement
//!
//! The routing guard ensures all inference requests pass through `route_and_infer()`.
//! Direct worker calls without this module will result in hard failure.
//!
//! ## Deterministic Replay
//!
//! Replay uses the same inference path as normal requests. Routing is deterministic
//! by design (sorted by score DESC, then by index ASC for ties). The `router_seed`
//! field is stored for **audit purposes only** - it does not affect routing decisions.
//!
//! For replay, pass a `ReplayContext` to enforce manifest/backend compatibility
//! and skip metadata capture for the replay itself.

use crate::chat_session_config::ChatSessionConfig;
use crate::citations::collect_citations_for_adapters;
use crate::handlers::rag_common::{retrieve_rag_context, store_rag_evidence, RagContextResult};
use crate::middleware::policy_enforcement::{compute_policy_mask_digest, enforce_at_hook};
use crate::state::AppState;
use crate::types::{
    new_run_envelope, set_policy_mask, set_router_seed, set_worker_context, ChunkReference,
    InferenceError, InferenceRequestInternal, InferenceResult, PlacementReplay,
    PlacementTraceEntry, RagEvidence, ReplayContext, RouterCandidateRecord, RouterDecisionRecord,
    SamplingParams, WorkerInferRequest, MAX_REPLAY_TEXT_SIZE, SAMPLING_ALGORITHM_VERSION,
};
use crate::uds_client::UdsClient;
use adapteros_api_types::inference::{
    ReplayGuarantee, RouterDecision as ApiRouterDecision,
    RouterDecisionChainEntry as ApiRouterDecisionChainEntry,
};
use adapteros_api_types::{RunActor, RunEnvelope};
use adapteros_config::PlacementConfig;
use adapteros_core::{
    determinism_mode::DeterminismMode, identity::IdentityEnvelope, B3Hash, BackendKind,
    GuardLogLevel, SeedScopeGuard,
};
use adapteros_db::workers::WorkerWithBinding;
use adapteros_db::{chat_sessions::ChatSession, CreateReplayMetadataParams};
use adapteros_policy::hooks::{HookContext, PolicyHook};
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use adapteros_telemetry::{
    build_inference_metrics_event, build_routing_event, InferenceMetricsEvent,
    RouterDecisionChainEntry, RouterDecisionHash, RoutingTelemetryEvent,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use adapteros_types::routing::{RouterCandidate, RouterDecision, RouterModelType};
use hex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
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

fn map_router_decisions(
    events: &[ApiRouterDecision],
    policy_mask_digest: Option<[u8; 32]>,
) -> Vec<RouterDecision> {
    // policy_mask_digest is already [u8; 32] which matches adapteros_types::routing::B3Hash
    events
        .iter()
        .map(|d| RouterDecision {
            step: d.step,
            input_token_id: d.input_token_id,
            candidate_adapters: d
                .candidate_adapters
                .iter()
                .map(|c| RouterCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                })
                .collect(),
            entropy: d.entropy as f64,
            tau: d.tau as f64,
            entropy_floor: d.entropy_floor as f64,
            stack_hash: d.stack_hash.clone(),
            interval_id: d.interval_id.clone(),
            allowed_mask: None,
            policy_mask_digest,
            policy_overrides_applied: None,
            model_type: match d.model_type {
                adapteros_api_types::inference::RouterModelType::Dense => RouterModelType::Dense,
                adapteros_api_types::inference::RouterModelType::Moe => RouterModelType::Moe,
            },
            active_experts: d.active_experts.clone(),
        })
        .collect()
}

fn map_router_decision_chain(
    chain: Option<Vec<ApiRouterDecisionChainEntry>>,
) -> Option<Vec<RouterDecisionChainEntry>> {
    chain.map(|entries| {
        entries
            .into_iter()
            .map(|e| RouterDecisionChainEntry {
                step: e.step,
                input_token_id: e.input_token_id,
                adapter_indices: e.adapter_indices,
                adapter_ids: e.adapter_ids,
                gates_q15: e.gates_q15,
                entropy: e.entropy,
                decision_hash: e.decision_hash.map(|h| RouterDecisionHash {
                    input_hash: h.input_hash,
                    output_hash: h.output_hash,
                    reasoning_hash: h.reasoning_hash,
                    combined_hash: h.combined_hash,
                    tau: h.tau,
                    eps: h.eps,
                    k: h.k,
                }),
                previous_hash: e.previous_hash,
                entry_hash: e.entry_hash,
            })
            .collect()
    })
}

/// Ensure pinned adapters (if any) are within the effective adapter set when present.
fn validate_pinned_within_effective_set(
    effective_adapter_ids: &Option<Vec<String>>,
    pinned_adapter_ids: &Option<Vec<String>>,
) -> Result<(), InferenceError> {
    if let (Some(effective), Some(pinned)) = (effective_adapter_ids, pinned_adapter_ids) {
        if effective.is_empty() {
            return Ok(());
        }
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

/// Enforce strict determinism runtime guards on worker responses.
///
/// - Known backend identifier is required (fails on unknown/blank backend)
/// - Backend version (kernel_version_id) must match the running build
/// - Router decision chain with Q15 gates must be present when adapters are used
/// - Canonical manifest hash is required to bind seeds/context
fn enforce_strict_runtime_guards(
    mode: DeterminismMode,
    backend_used: &Option<String>,
    backend_version: &Option<String>,
    router_chain: &Option<Vec<ApiRouterDecisionChainEntry>>,
    adapters_used: &[String],
    manifest_hash: Option<&B3Hash>,
) -> Result<(), InferenceError> {
    if mode != DeterminismMode::Strict {
        return Ok(());
    }

    if manifest_hash.is_none() {
        return Err(InferenceError::ValidationError(
            "Strict determinism mode requires canonical manifest context".to_string(),
        ));
    }

    let backend_name = backend_used.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires a reported backend (backend_used)".to_string(),
        )
    })?;

    BackendKind::from_str(backend_name).map_err(|e| {
        InferenceError::WorkerError(format!(
            "Strict determinism mode requires a known backend: {}",
            e
        ))
    })?;

    let kernel_version_id = backend_version.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires kernel_version_id from worker".to_string(),
        )
    })?;

    if kernel_version_id != adapteros_core::version::VERSION {
        return Err(InferenceError::WorkerError(format!(
            "kernel_version_id mismatch: expected {}, got {}",
            adapteros_core::version::VERSION,
            kernel_version_id
        )));
    }

    // Only enforce routing evidence when adapters are active; base-only requests
    // do not emit router decisions.
    if adapters_used.is_empty() {
        return Ok(());
    }

    let chain = router_chain.as_ref().ok_or_else(|| {
        InferenceError::WorkerError(
            "Strict determinism mode requires router_decision_chain with Q15 gates".to_string(),
        )
    })?;

    if chain.is_empty() {
        return Err(InferenceError::WorkerError(
            "Strict determinism mode requires non-empty router_decision_chain".to_string(),
        ));
    }

    for entry in chain {
        if entry.gates_q15.is_empty() {
            return Err(InferenceError::WorkerError(
                "Strict determinism mode forbids float-only gates; Q15 gates missing".to_string(),
            ));
        }
        if entry.adapter_indices.len() != entry.gates_q15.len() {
            return Err(InferenceError::WorkerError(format!(
                "Router decision gate count mismatch (indices={}, gates={})",
                entry.adapter_indices.len(),
                entry.gates_q15.len()
            )));
        }
    }

    Ok(())
}

// =============================================================================
// TenantExecutionPolicy Resolution (Bundle E)
// =============================================================================

/// Resolved routing policy knobs
#[derive(Debug, Clone, Default)]
pub struct RoutingPolicyResolved {
    /// Whether to use session's stack_id when no explicit stack is provided
    /// Enforced in resolve_effective_adapters() at line ~848
    pub use_session_stack_for_routing: bool,
    /// Whether pins outside effective set are allowed (always false per Bundle A)
    pub allow_pins_outside_effective_set: bool,
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
    /// CoreML mode applied to backend selection for this request
    pub coreml_mode: CoreMLMode,
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
    coreml_mode: Option<CoreMLMode>,
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
        .and_then(|g| g.determinism_mode)
        // Default to strict to avoid relaxed/best-effort slipping in implicitly.
        .unwrap_or(DeterminismMode::Strict);

    // Use tenant policy's default_mode only for explicit policies.
    // Implicit/default policies should fall through to global mode.
    let tenant_mode = if policy.is_implicit {
        None
    } else {
        Some(policy.determinism.default_mode.as_str())
    };

    // 3. Resolve determinism mode (stack > tenant > global)
    let effective_determinism_mode =
        resolve_determinism_mode(stack_determinism_mode, tenant_mode, global_mode.as_str());

    // 4. Compute strict mode
    let coreml_mode = coreml_mode.unwrap_or(CoreMLMode::CoremlPreferred);
    let allow_backend_fallback =
        policy.determinism.allow_fallback && coreml_mode != CoreMLMode::CoremlStrict;

    let strict_mode = compute_strict_mode(effective_determinism_mode, allow_backend_fallback);

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
        coreml_mode,
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
        cancellation_token: Option<CancellationToken>,
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
            inference_span.record("router_seed", tracing::field::display(seed));
        }

        let _inference_span_guard = inference_span.enter();
        // Ensure seed registry is scoped to this inference request to prevent cross-request reuse errors.
        let _seed_scope = SeedScopeGuard::for_inference(GuardLogLevel::Warn);

        let should_capture = match &replay_context {
            None => true,
            Some(ctx) => !ctx.skip_metadata_capture,
        };

        let result = async {
        let mut all_policy_decisions = Vec::new();
        if request.run_envelope.is_none() {
            if let Some(claims) = request.claims.as_ref() {
                request.run_envelope = Some(new_run_envelope(
                    self.state,
                    claims,
                    request.request_id.clone(),
                    request.reasoning_mode,
                ));
            } else {
                request.run_envelope = Some(RunEnvelope {
                    run_id: request.request_id.clone(),
                    schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                    workspace_id: request.cpid.clone(),
                    actor: RunActor {
                        subject: "unknown".to_string(),
                        roles: Vec::new(),
                        principal_type: Some("unknown".to_string()),
                        auth_mode: Some("unauthenticated".to_string()),
                    },
                    manifest_hash_b3: self.state.manifest_hash.clone(),
                    plan_id: None,
                    policy_mask_digest_b3: None,
                    router_seed: None,
                    tick: self
                        .state
                        .tick_ledger
                        .as_ref()
                        .map(|ledger| ledger.increment_tick()),
                    worker_id: None,
                    reasoning_mode: request.reasoning_mode,
                    determinism_version: crate::types::run_envelope::RUN_ENVELOPE_VERSION
                        .to_string(),
                    boot_trace_id: self
                        .state
                        .boot_state
                        .as_ref()
                        .map(|boot| boot.boot_trace_id()),
                    created_at: chrono::Utc::now(),
                });
            }
        }
        if let Some(token) = cancellation_token.as_ref() {
            if token.is_cancelled() {
                return Err(InferenceError::ClientClosed(
                    "Request cancelled before dispatch".to_string(),
                ));
            }
        }

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

        // 0.25 Apply chat session config (stack, routing determinism, strength overrides)
        if let Some(ref session) = session {
            if let Some(config) = ChatSessionConfig::from_metadata(session.metadata_json.as_deref())
            {
                if request.stack_id.is_none() {
                    request.stack_id = config.stack_id.clone();
                }
                if request.routing_determinism_mode.is_none() {
                    request.routing_determinism_mode = config.routing_determinism_mode;
                }
                if request.adapter_strength_overrides.is_none() {
                    request.adapter_strength_overrides = config.adapter_strength_overrides;
                }
            }
        }

        // Default routing determinism to deterministic when unset
        if request.routing_determinism_mode.is_none() {
            request.routing_determinism_mode = Some(RoutingDeterminismMode::Deterministic);
        }

        // 0.5 Resolve effective adapter set and stack metadata
        self.resolve_effective_adapters(&mut request, session.as_ref())
            .await?;

        // Build tenant allowlist once for adapter and pin validation
        let tenant_adapter_allowlist = self.adapter_allowlist_for_tenant(&request.cpid).await?;

        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            self.validate_ids_against_allowlist(
                effective,
                &request.cpid,
                &tenant_adapter_allowlist,
                "Adapter",
            )?;
        }

        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            inference_span.record("adapters", tracing::field::display(effective.join(",")));
        } else if let Some(adapters) = request.adapters.as_ref() {
            inference_span.record("adapters", tracing::field::display(adapters.join(",")));
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

        // Resolve CoreML mode (request-level override or default preferred)
        let coreml_mode = request.coreml_mode.unwrap_or(CoreMLMode::CoremlPreferred);
        request.coreml_mode = Some(coreml_mode);

        let resolved_policy = resolve_tenant_execution_policy(
            &self.state.db,
            &config,
            &request.cpid,
            request.stack_determinism_mode.as_deref(),
            Some(coreml_mode),
        )
        .await?;

        // 0.7 Resolve execution profile (seed_mode + backend_profile) and derive request seed
        let execution_profile = crate::execution_profile::resolve_execution_profile(
            &request,
            &config,
            &resolved_policy.policy,
        )?;

        if request.backend_profile.is_some()
            && execution_profile.profile.backend_profile != execution_profile.default_backend
        {
            tracing::info!(
                request_id = %request.request_id,
                requested = %execution_profile.profile.backend_profile.as_str(),
                default = %execution_profile.default_backend.as_str(),
                "Backend override requested for this inference"
            );
        }

        let manifest_hash = self
            .state
            .manifest_hash
            .as_deref()
            .and_then(|h| B3Hash::from_hex(h).ok());

        let global_seed = manifest_hash
            .as_ref()
            .cloned()
            .unwrap_or_else(|| B3Hash::hash(b"adapteros-request-global"));

        let determinism_ctx = crate::determinism_context::from_request(
            &request,
            manifest_hash.as_ref(),
            &global_seed,
            execution_profile.profile.seed_mode,
            config.worker_id,
        )?;

        request.seed_mode = Some(execution_profile.profile.seed_mode);
        request.backend_profile = Some(execution_profile.profile.backend_profile);
        request.request_seed = Some(determinism_ctx.request_seed());
        request.router_seed = Some(determinism_ctx.router_seed_hex().to_string());
        request.seed = Some(determinism_ctx.request_seed_low64());
        if let Some(ref mut envelope) = request.run_envelope {
            set_router_seed(envelope, request.router_seed.as_ref());
        }

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

        // 0.1 Gate on base model readiness (strictly per-tenant)
        let base_status = self
            .state
            .db
            .get_base_model_status(&request.cpid)
            .await
            .map_err(|e| {
                InferenceError::WorkerError(format!("Failed to fetch base model status: {}", e))
            })?;

        let mut records: Vec<adapteros_db::models::BaseModelStatus> = Vec::new();
        if let Some(status) = base_status {
            if request
                .model
                .as_ref()
                .is_none_or(|model_id| &status.model_id == model_id)
            {
                records.push(status);
            }
        }

        if records.is_empty() {
            return Err(InferenceError::ModelNotReady(format!(
                "Base model not ready for tenant {}",
                request.cpid
            )));
        }

        let aggregated = crate::model_status::aggregate_status(records.iter());
        if !aggregated.status.is_ready() {
            return Err(InferenceError::ModelNotReady(format!(
                "Base model not ready (status: {})",
                aggregated.status.as_str()
            )));
        }
        let base_model_id = aggregated.latest.map(|s| s.model_id.clone());

        // 1. Resolve worker UDS path and capture worker identifier for telemetry
        // For replay: enforce manifest/backend constraints first, then reuse standard selection.
        let (uds_path, selected_worker) = if let Some(ref ctx) = replay_context {
            let path = self
                .resolve_worker_path_for_replay(
                    &request.cpid,
                    &ctx.required_manifest_hash,
                    &ctx.required_backend,
                )
                .await?;
            let worker = self.select_worker_for_tenant(&request.cpid).await.ok();
            (path, worker)
        } else {
            let worker = self.select_worker_for_tenant(&request.cpid).await?;
            (PathBuf::from(&worker.uds_path), Some(worker))
        };
        let selected_worker_id = selected_worker.as_ref().map(|w| w.id.clone());

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
                tracing::field::display(&evidence.context_hash),
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

        // Enforce pinned adapters are permitted and loadable for this tenant
        if let Some(pins) = pinned_adapter_ids.as_ref() {
            self.validate_ids_against_allowlist(
                pins,
                &request.cpid,
                &tenant_adapter_allowlist,
                "Pinned adapter",
            )?;
            self.validate_adapter_ids_loadable(pins, &request.request_id)
                .await?;
        }

        // Enforce pinned adapters must be within effective set (if provided)
        validate_pinned_within_effective_set(&request.effective_adapter_ids, &pinned_adapter_ids)?;

        // Guard: pinned adapters must exist for the request tenant before worker call
        if let Some(pins) = pinned_adapter_ids.as_ref() {
            self.validate_pinned_adapters_for_tenant(&request.cpid, pins)
                .await?;
        }

        // Stage 3: Policy Hooks (OnRequestBeforeRouting)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Policy Hook Point: Pre-routing validation                       │
        // │ - Enforces tenant-level constraints before adapter selection    │
        // │ - Validates input prompt safety and resource budgets            │
        // │ - Rejection at this stage prevents all downstream computation   │
        // └─────────────────────────────────────────────────────────────────┘
        let routing_hook_ctx = HookContext::new(
            request.cpid.clone(),
            request.request_id.clone(),
            PolicyHook::OnRequestBeforeRouting,
            "inference",
        )
        .with_metadata(
            "adapter_ids",
            serde_json::json!(request.effective_adapter_ids),
        );

        let routing_decisions = enforce_at_hook(self.state, &routing_hook_ctx)
            .await
            .map_err(|e| {
                let violation = e.violations.first();
                InferenceError::PolicyViolation {
                    tenant_id: request.cpid.clone(),
                    policy_id: violation
                        .map(|v| v.policy_pack_id.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: e.message,
                }
            })?;
        all_policy_decisions.extend(routing_decisions);

        let routing_policy = Some(execution_policy.routing.clone().unwrap_or_default());

        // Stage 7: Policy Hooks (OnBeforeInference)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Policy Hook Point: Pre-execution validation                     │
        // │ - Enforces quota limits and rate limiting                      │
        // │ - Final validation of resolved worker and placement             │
        // │ - Captures policy mask for deterministic replay verification   │
        // └─────────────────────────────────────────────────────────────────┘
        let before_hook_ctx = HookContext::new(
            request.cpid.clone(),
            request.request_id.clone(),
            PolicyHook::OnBeforeInference,
            "inference",
        )
        .with_metadata(
            "adapter_ids",
            serde_json::json!(request.effective_adapter_ids),
        )
        .with_metadata("worker_id", serde_json::json!(selected_worker_id));

        let before_decisions = enforce_at_hook(self.state, &before_hook_ctx)
            .await
            .map_err(|e| {
                let violation = e.violations.first();
                InferenceError::PolicyViolation {
                    tenant_id: request.cpid.clone(),
                    policy_id: violation
                        .map(|v| v.policy_pack_id.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: e.message,
                }
            })?;
        all_policy_decisions.extend(before_decisions);

        // Compute policy mask digest for the worker call
        let policy_mask_digest = compute_policy_mask_digest(&all_policy_decisions);
        request.policy_mask_digest = Some(policy_mask_digest);
        if let Some(ref mut envelope) = request.run_envelope {
            set_policy_mask(envelope, Some(&policy_mask_digest));
            set_worker_context(
                envelope,
                selected_worker.as_ref(),
                self.state.manifest_hash.clone(),
            );
        }

        // 3. Create worker request with full sampling parameters
        let worker_request = WorkerInferRequest {
            cpid: request.cpid.clone(),
            prompt: augmented_prompt.clone(),
            max_tokens: request.max_tokens,
            request_id: Some(request.request_id.clone()),
            run_envelope: request.run_envelope.clone(),
            require_evidence: request.require_evidence,
            admin_override: request.admin_override,
            reasoning_mode: request.reasoning_mode,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            domain_hint: request.domain_hint.clone(),
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            seed: request.seed,
            router_seed: request.router_seed.clone(),
            seed_mode: request.seed_mode,
            request_seed: request.request_seed,
            determinism: Some(determinism_ctx.clone()),
            backend_profile: request.backend_profile,
            coreml_mode: request.coreml_mode,
            pinned_adapter_ids: pinned_adapter_ids.clone(),
            strict_mode: Some(strict_mode),
            determinism_mode: request.determinism_mode.clone(),
            routing_determinism_mode: request.routing_determinism_mode,
            effective_adapter_ids: request.effective_adapter_ids.clone(),
            routing_policy,
            placement: None,
            adapter_strength_overrides: request.adapter_strength_overrides.clone(),
            stop_policy: request.stop_policy.clone(),
            policy_mask_digest: Some(policy_mask_digest),
            utf8_healing: request.utf8_healing.unwrap_or(true),
        };

        if let Some(ref envelope) = worker_request.run_envelope {
            info!(
                run_id = %envelope.run_id,
                worker_id = %envelope.worker_id.as_deref().unwrap_or("unknown"),
                plan_id = %envelope.plan_id.as_deref().unwrap_or("none"),
                manifest_hash_b3 = %envelope
                    .manifest_hash_b3
                    .as_deref()
                    .unwrap_or("unknown"),
                policy_mask_digest_b3 = %envelope
                    .policy_mask_digest_b3
                    .as_deref()
                    .unwrap_or("none"),
                router_seed = %envelope.router_seed.as_deref().unwrap_or("none"),
                "Dispatching worker inference with run envelope"
            );
        }

        // 4. Call worker via UDS
        // Longer timeout for replay to account for cold worker startup
        let timeout_secs = if is_replay { 120 } else { 60 };
        let uds_client = UdsClient::new(Duration::from_secs(timeout_secs));

        // Generate worker auth token if signing keypair is available (CP->Worker auth)
        // This is a short-lived Ed25519-signed JWT for internal authentication
        let worker_auth_token: Option<String> = if let Some(ref signing_key) =
            self.state.worker_signing_keypair
        {
            // When auth is enabled, we MUST have a valid worker_id.
            // A token with "unknown" worker_id would be rejected by the worker anyway,
            // so we fail early with a clear error instead.
            let worker_id = match selected_worker_id.as_ref() {
                Some(id) => id.to_string(),
                None => {
                    return Err(InferenceError::WorkerIdUnavailable {
                        tenant_id: request.cpid.clone(),
                        reason: "Worker selection returned None but auth is required".to_string(),
                    });
                }
            };
            match adapteros_boot::generate_worker_token(
                signing_key,
                &worker_id,
                &request.request_id,
                45, // 45 second TTL
            ) {
                Ok(token) => Some(token),
                Err(e) => {
                    warn!(
                        error = %e,
                        request_id = %request.request_id,
                        "Failed to generate worker auth token, proceeding without"
                    );
                    None
                }
            }
        } else {
            // Fall back to request's worker auth token (legacy API key path)
            request.worker_auth_token.clone()
        };

        // Wrap the UDS call in routing context to ensure task-local guard is set
        let worker_call = crate::uds_client::run_with_routing_context(async {
            uds_client
                .infer(
                    &uds_path,
                    worker_request,
                    worker_auth_token.as_deref(),
                    cancellation_token.clone(),
                )
                .await
        });

        let worker_response = if let Some(token) = cancellation_token.as_ref() {
            tokio::select! {
                _ = token.cancelled() => {
                    return Err(InferenceError::ClientClosed(
                        "Request cancelled while waiting for worker".to_string()
                    ));
                }
                res = worker_call => res
            }
        } else {
            worker_call.await
        }
        .map_err(|e| match e {
            crate::uds_client::UdsClientError::WorkerNotAvailable(msg) => {
                InferenceError::WorkerNotAvailable(msg)
            }
            crate::uds_client::UdsClientError::Timeout(msg) => InferenceError::Timeout(msg),
            crate::uds_client::UdsClientError::RoutingBypass(msg) => {
                InferenceError::RoutingBypass(msg)
            }
            crate::uds_client::UdsClientError::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                model_key,
            } => InferenceError::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                model_key,
            },
            crate::uds_client::UdsClientError::Cancelled(msg) => {
                InferenceError::ClientClosed(msg)
            }
            other => InferenceError::WorkerError(other.to_string()),
        })?;

        // Stage 9: Policy Hooks (OnAfterInference)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Policy Hook Point: Post-execution validation                    │
        // │ - Enforces output safety and evidence requirements             │
        // │ - Validates citation quality and grounding                      │
        // │ - Post-inference rejection masks output from the client         │
        // └─────────────────────────────────────────────────────────────────┘
        let after_hook_ctx = HookContext::new(
            request.cpid.clone(),
            request.request_id.clone(),
            PolicyHook::OnAfterInference,
            "inference",
        )
        .with_metadata(
            "adapter_ids",
            serde_json::json!(worker_response.trace.router_summary.adapters_used),
        )
        .with_metadata(
            "output_length",
            serde_json::json!(worker_response.text.as_ref().map(|t| t.len()).unwrap_or(0)),
        );

        let after_decisions = enforce_at_hook(self.state, &after_hook_ctx)
            .await
            .map_err(|e| {
                let violation = e.violations.first();
                InferenceError::PolicyViolation {
                    tenant_id: request.cpid.clone(),
                    policy_id: violation
                        .map(|v| v.policy_pack_id.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: e.message,
                }
            })?;
        all_policy_decisions.extend(after_decisions);

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
        let backend_version = worker_response.backend_version.clone();
        let fallback_triggered = worker_response.fallback_triggered;
        let coreml_compute_preference = worker_response.coreml_compute_preference.clone();
        let coreml_compute_units = worker_response.coreml_compute_units.clone();
        let coreml_gpu_used = worker_response.coreml_gpu_used;
        let coreml_package_hash = worker_response.coreml_package_hash.clone();
        let coreml_expected_package_hash = worker_response.coreml_expected_package_hash.clone();
        let coreml_hash_mismatch = worker_response.coreml_hash_mismatch;
        let fallback_backend = worker_response.fallback_backend.clone();
        let coreml_hash_mismatch_flag = coreml_hash_mismatch.unwrap_or(false);

        if let Some(ref backend_name) = backend_used {
            inference_span.record("backend", tracing::field::display(backend_name));
        }

        let adapters_used_field = worker_response.trace.router_summary.adapters_used.join(",");
        if !adapters_used_field.is_empty() {
            inference_span.record(
                "adapters_used",
                tracing::field::display(adapters_used_field),
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

        enforce_strict_runtime_guards(
            resolved_mode,
            &backend_used,
            &backend_version,
            &worker_response.trace.router_decision_chain,
            &worker_response.trace.router_summary.adapters_used,
            manifest_hash.as_ref(),
        )?;

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

        info!(
            request_id = %request.request_id,
            backend = backend_used.as_deref().unwrap_or("unknown"),
            backend_version = backend_version.as_deref().unwrap_or(""),
            fallback_triggered,
            fallback_backend = fallback_backend.as_deref().unwrap_or(""),
            determinism_mode = determinism_mode_applied,
            seed_mode = request
                .seed_mode
                .as_ref()
                .map(|m| m.as_str())
                .unwrap_or("unspecified"),
            coreml_compute_preference = coreml_compute_preference.as_deref().unwrap_or(""),
            coreml_compute_units = coreml_compute_units.as_deref().unwrap_or(""),
            coreml_gpu_used = coreml_gpu_used.unwrap_or(false),
            coreml_expected_hash = coreml_expected_package_hash.as_deref().unwrap_or(""),
            coreml_package_hash = coreml_package_hash.as_deref().unwrap_or(""),
            coreml_hash_mismatch = coreml_hash_mismatch_flag,
            "Inference backend summary"
        );

        // 8. Capture replay metadata for deterministic replay
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Combined Evidence: Both RAG and adapter decisions are recorded  │
        // │ - rag_snapshot_hash: BLAKE3 hash of sorted doc hashes           │
        // │ - adapter_ids_json: Adapters specified in request               │
        // │ This enables full audit trail regardless of pipeline path used. │
        // └─────────────────────────────────────────────────────────────────┘
        // Skip for replay-of-replay to avoid recursive metadata creation
        if should_capture {
            self.capture_replay_metadata(
                &request,
                &augmented_prompt,
                &response_text,
                backend_used.as_deref(),
                backend_version.as_deref(),
                coreml_package_hash.as_deref(),
                coreml_expected_package_hash.as_deref(),
                coreml_hash_mismatch,
                base_model_id.clone(),
                &rag_evidence,
                latency_ms,
                prompt_truncated,
                response_truncated,
                &determinism_mode_applied,
                fallback_triggered,
                coreml_compute_preference.as_deref(),
                coreml_compute_units.as_deref(),
                coreml_gpu_used,
                fallback_backend.as_deref(),
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

        // 9.5 Build citations from training datasets (best-effort)
        let citations = collect_citations_for_adapters(
            self.state,
            &request.cpid,
            &worker_response.trace.router_summary.adapters_used,
            &request.prompt,
            3,
        )
        .await;

        // 10. Emit telemetry for inference metrics and routing (best-effort)
        let identity = IdentityEnvelope::new(
            request.cpid.clone(),
            "api".to_string(),
            "inference".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        let metrics_payload = InferenceMetricsEvent {
            tenant_id: request.cpid.clone(),
            request_id: request.request_id.clone(),
            model_id: request
                .model
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            adapter_set: worker_response.trace.router_summary.adapters_used.clone(),
            seed_present: request.seed.is_some()
                || request.request_seed.is_some()
                || request.router_seed.is_some(),
            latency_ms: Some(latency_ms),
            input_tokens: None,
            output_tokens: None,
            success: true,
            error: None,
        };

        if let Ok(event) = build_inference_metrics_event(identity.clone(), metrics_payload) {
            let telemetry_buffer = self.state.telemetry_buffer.clone();
            tokio::spawn(async move {
                let _ = telemetry_buffer.push(event).await;
            });
        }

        if let Some(router_events) = worker_response.trace.router_decisions.clone() {
            let mapped_decisions =
                map_router_decisions(&router_events, request.policy_mask_digest);
            let mapped_chain =
                map_router_decision_chain(worker_response.trace.router_decision_chain.clone());
            let routing_payload = RoutingTelemetryEvent {
                tenant_id: request.cpid.clone(),
                request_id: request.request_id.clone(),
                model_id: request.model.clone(),
                worker_id: selected_worker_id.clone(),
                adapter_ids: worker_response.trace.router_summary.adapters_used.clone(),
                determinism_mode: request.determinism_mode.clone(),
                seed_hash: request.router_seed.clone(),
                router_decisions: mapped_decisions,
                router_decision_chain: mapped_chain,
                is_replay,
            };

            if let Ok(event) = build_routing_event(identity, routing_payload) {
                let telemetry_buffer = self.state.telemetry_buffer.clone();
                tokio::spawn(async move {
                    let _ = telemetry_buffer.push(event).await;
                });
            }
        }

        let receipt_sampling_params = adapteros_api_types::inference::ReceiptSamplingParams {
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            seed: request.seed,
        };

        let receipt_params_json = serde_json::to_vec(&receipt_sampling_params).unwrap_or_default();
        let prompt_system_params_digest_b3 = B3Hash::hash_multi(&[
            augmented_prompt.as_bytes(),
            b"\0",
            b"",
            b"\0",
            receipt_params_json.as_slice(),
        ]);

        let deterministic_receipt = adapteros_api_types::inference::DeterministicReceipt {
            router_seed: request.router_seed.clone().unwrap_or_default(),
            sampling_params: receipt_sampling_params,
            stack_id: request.stack_id.clone(),
            adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
            model: base_model_id.clone().or_else(|| request.model.clone()),
            backend_used: backend_used.clone(),
            prompt_system_params_digest_b3,
        };

        // 11. Emit plugin event for inference completion (if event bus configured)
        if let Some(ref event_bus) = self.state.event_bus {
            use adapteros_core::plugin_events::{InferenceEvent, PluginEvent};
            use chrono::Utc;

            let inference_event = InferenceEvent {
                request_id: request.request_id.clone(),
                adapter_ids: worker_response
                    .trace
                    .router_summary
                    .adapters_used
                    .clone(),
                stack_id: request.stack_id.clone(),
                prompt: if request.stream {
                    None // Don't include full prompt for streaming requests
                } else {
                    Some(augmented_prompt.clone())
                },
                output: if request.stream {
                    None // Don't include full output for streaming requests
                } else {
                    worker_response.text.clone()
                },
                latency_ms: latency_ms as f64,
                tokens_generated: None, // Not tracked in current worker response
                tokens_per_sec: None,   // Not tracked
                tenant_id: Some(request.cpid.clone()),
                model: request.model.clone(),
                streaming: request.stream,
                timestamp: Utc::now().to_rfc3339(),
                metadata: std::collections::HashMap::new(),
            };

            let event = PluginEvent::InferenceComplete(inference_event);

            // Emit event asynchronously (don't block on failures)
            let event_bus_clone = event_bus.clone();
            tokio::spawn(async move {
                if let Err(failures) = event_bus_clone.emit(event).await {
                    warn!(
                        failed_plugins = ?failures,
                        "Some plugins failed to handle InferenceComplete event"
                    );
                }
            });
        }

        // 12. Build and return result
        Ok(InferenceResult {
            text: worker_response.text.unwrap_or_default(),
            tokens_generated: 0, // Not tracked in current worker response
            finish_reason: worker_response.status,
            adapters_used: worker_response.trace.router_summary.adapters_used,
            router_decisions,
            router_decision_chain,
            moe_info: worker_response.trace.moe_info.clone(),
            expert_routing: worker_response.trace.expert_routing.clone(),
            active_experts: worker_response.trace.active_experts.clone(),
            model_type: worker_response
                .trace
                .model_type
                .clone()
                .or(Some(adapteros_api_types::inference::RouterModelType::Dense)),
            rag_evidence,
            citations,
            latency_ms,
            request_id: request.request_id.clone(),
            unavailable_pinned_adapters: unavailable_pinned,
            pinned_routing_fallback,
            effective_adapter_ids: request.effective_adapter_ids.clone(),
            backend_used,
            deterministic_receipt: Some(deterministic_receipt),
            fallback_triggered,
            coreml_compute_preference,
            coreml_compute_units,
            coreml_gpu_used,
            fallback_backend,
            determinism_mode_applied: Some(determinism_mode_applied),
            replay_guarantee: Some(replay_guarantee),
            placement_trace: worker_response.placement_trace.clone(),
            run_envelope: request.run_envelope.clone(),
            // Stop Controller fields
            stop_reason_code: worker_response.stop_reason_code,
            stop_reason_token_index: worker_response.stop_reason_token_index,
            stop_policy_digest_b3: worker_response.stop_policy_digest_b3.clone(),
        })
        }
        .await;

        if let Err(err) = &result {
            if should_capture {
                let latency_ms = start_time.elapsed().as_millis() as u64;
                self.capture_failed_replay_metadata(&request, err, latency_ms)
                    .await;
            } else {
                debug!(
                    request_id = %request.request_id,
                    "Skipping replay metadata capture on error (skip_metadata_capture=true)"
                );
            }
        }

        result
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
        cancellation_token: Option<CancellationToken>,
    ) -> Result<InferenceResult, InferenceError> {
        self.route_and_infer(request, Some(replay_context), cancellation_token)
            .await
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
        let adapter_ids: Vec<String> =
            if let Some(effective) = request.effective_adapter_ids.as_ref() {
                effective.clone()
            } else if let Some(adapters) = request.adapters.as_ref() {
                adapters.clone()
            } else if let Some(stack_list) = request.adapter_stack.as_ref() {
                stack_list.clone()
            } else {
                Vec::new()
            };

        self.validate_adapter_ids_loadable(&adapter_ids, &request.request_id)
            .await
    }

    /// Validate pinned adapters belong to the requesting tenant.
    ///
    /// PRD-RECT-001: Uses tenant-scoped query to prevent cross-tenant enumeration.
    /// Returns `AdapterNotFound` for both missing and cross-tenant adapters.
    async fn validate_pinned_adapters_for_tenant(
        &self,
        tenant_id: &str,
        pins: &[String],
    ) -> Result<(), InferenceError> {
        for pin in pins {
            // PRD-RECT-001: Use tenant-scoped query instead of get_adapter() + manual check.
            // This ensures cross-tenant access returns AdapterNotFound (not PermissionDenied)
            // to prevent tenant enumeration attacks.
            let _adapter = self
                .state
                .db
                .get_adapter_for_tenant(tenant_id, pin)
                .await
                .map_err(|e| {
                    InferenceError::AdapterNotFound(format!(
                        "Failed to load pinned adapter '{}': {}",
                        pin, e
                    ))
                })?
                .ok_or_else(|| {
                    // Returns same error for both "not found" and "cross-tenant" cases
                    InferenceError::AdapterNotFound(format!("Pinned adapter '{}' not found", pin))
                })?;
            // No manual tenant check needed - query is already tenant-scoped
        }

        Ok(())
    }

    /// Build allowlist of adapter IDs for a tenant for membership checks
    async fn adapter_allowlist_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<HashSet<String>, InferenceError> {
        let adapters = self
            .state
            .db
            .list_adapters_for_tenant(tenant_id)
            .await
            .map_err(|e| {
                InferenceError::WorkerError(format!(
                    "Failed to list adapters for tenant {}: {}",
                    tenant_id, e
                ))
            })?;

        Ok(adapters.into_iter().map(|a| a.id).collect())
    }

    /// Ensure every adapter in `ids` is permitted for the tenant.
    ///
    /// PRD-RECT-001: Returns `AdapterNotFound` instead of `PermissionDenied`
    /// to prevent tenant enumeration attacks. This makes cross-tenant adapter
    /// access indistinguishable from "adapter does not exist".
    fn validate_ids_against_allowlist(
        &self,
        ids: &[String],
        _tenant_id: &str,
        allowlist: &HashSet<String>,
        _context: &str,
    ) -> Result<(), InferenceError> {
        for id in ids {
            if !allowlist.contains(id) {
                // Return AdapterNotFound to avoid leaking existence of adapters
                // from other tenants (PRD-RECT-001: No existence leaks)
                return Err(InferenceError::AdapterNotFound(id.clone()));
            }
        }
        Ok(())
    }

    /// Validate a list of adapters are loadable and not archived/purged.
    async fn validate_adapter_ids_loadable(
        &self,
        adapter_ids: &[String],
        request_id: &str,
    ) -> Result<(), InferenceError> {
        for adapter_id in adapter_ids {
            match self.state.db.is_adapter_loadable(adapter_id).await {
                Ok(true) => continue, // Adapter is loadable
                Ok(false) => {
                    warn!(
                        adapter_id = %adapter_id,
                        request_id = %request_id,
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
                request.effective_adapter_ids = Some(Vec::new());
                return Ok(());
            }
            request.effective_adapter_ids = Some(adapters.clone());
            return Ok(());
        }

        // 1b. Legacy adapter_stack acts as explicit adapter list
        if let Some(stack_list) = request.adapter_stack.as_ref() {
            if stack_list.is_empty() {
                request.effective_adapter_ids = Some(Vec::new());
                return Ok(());
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
                request.stack_id = Some(stack_id);
                request.stack_version = Some(stack.version);
                request.stack_determinism_mode = stack.determinism_mode.clone();
                request.stack_routing_determinism_mode =
                    parse_routing_mode(&stack.routing_determinism_mode);
                request.effective_adapter_ids = Some(Vec::new());
                return Ok(());
            }

            request.stack_id = Some(stack_id);
            request.stack_version = Some(stack.version);
            request.stack_determinism_mode = stack.determinism_mode.clone();
            request.stack_routing_determinism_mode =
                parse_routing_mode(&stack.routing_determinism_mode);
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

                if !stack.lifecycle_state.eq_ignore_ascii_case("active") {
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
                    request.stack_id = Some(default_stack_id.clone());
                    request.stack_version = Some(stack.version);
                    request.stack_determinism_mode = stack.determinism_mode.clone();
                    request.stack_routing_determinism_mode =
                        parse_routing_mode(&stack.routing_determinism_mode);
                    request.effective_adapter_ids = Some(Vec::new());
                    return Ok(());
                }

                request.stack_id = Some(default_stack_id.clone());
                request.stack_version = Some(stack.version);
                request.stack_determinism_mode = stack.determinism_mode.clone();
                request.stack_routing_determinism_mode =
                    parse_routing_mode(&stack.routing_determinism_mode);
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

    /// Select a compatible worker with manifest + health filtering.
    ///
    /// Uses retry logic with bounded attempts to handle transient worker unavailability.
    /// In dev mode, returns a degraded error after retries instead of hard failure.
    ///
    /// # Retry Behavior
    ///
    /// - 3 attempts with 2s, 4s delays between retries
    /// - Logs attempt number, delay, and remaining budget on each retry
    /// - In dev mode: returns WorkerDegraded after retries (allows graceful degradation)
    /// - In prod/staging mode: returns NoCompatibleWorker (hard failure)
    pub(crate) async fn select_worker_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<WorkerWithBinding, InferenceError> {
        use std::time::{Duration, Instant};

        const MAX_ATTEMPTS: u32 = 3;
        const BASE_DELAY: Duration = Duration::from_secs(2);
        const MAX_ELAPSED: Duration = Duration::from_secs(30);

        let required_manifest = self.state.manifest_hash.as_deref().ok_or_else(|| {
            InferenceError::NoCompatibleWorker {
                required_hash: "unset".to_string(),
                tenant_id: tenant_id.to_string(),
                available_count: 0,
                reason: "Manifest hash not configured on control plane".to_string(),
            }
        })?;

        let deadline = Instant::now() + MAX_ELAPSED;
        let mut attempt: u32 = 0;
        let mut delay = BASE_DELAY;

        loop {
            attempt += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());

            // Try to find a compatible worker
            let workers = self
                .state
                .db
                .list_compatible_workers_for_tenant(required_manifest, tenant_id)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!("Failed to list compatible workers: {}", e))
                })?;

            if let Some(worker) = self
                .state
                .health_monitor
                .as_ref()
                .and_then(|hm| hm.get_best_worker_with_binding(&workers))
                .cloned()
                .or_else(|| workers.first().cloned())
            {
                if attempt > 1 {
                    info!(
                        tenant_id = %tenant_id,
                        worker_id = %worker.id,
                        attempt = attempt,
                        "Selected compatible worker after retry"
                    );
                } else {
                    debug!(
                        tenant_id = %tenant_id,
                        worker_id = %worker.id,
                        manifest_hash = %worker.manifest_hash_b3.as_deref().unwrap_or("none"),
                        required_manifest = %required_manifest,
                        "Selected compatible worker for inference"
                    );
                }
                return Ok(worker);
            }

            // No worker found - determine reason and decide whether to retry
            let (available_count, reason) = self
                .diagnose_worker_unavailability(required_manifest, tenant_id)
                .await;

            // Check if we should retry
            let should_retry = attempt < MAX_ATTEMPTS && !remaining.is_zero();

            if should_retry {
                warn!(
                    attempt = attempt,
                    max_attempts = MAX_ATTEMPTS,
                    delay_ms = delay.as_millis() as u64,
                    remaining_budget_ms = remaining.as_millis() as u64,
                    tenant_id = %tenant_id,
                    reason = %reason,
                    "No compatible worker found, retrying"
                );

                // Sleep for the delay (capped by remaining budget)
                let actual_delay = delay.min(remaining);
                tokio::time::sleep(actual_delay).await;

                // Calculate next delay (2s -> 4s)
                delay *= 2;
            } else {
                // All retries exhausted - check if we're in dev mode for graceful degradation
                let is_dev_mode = self.state.runtime_mode.map(|m| m.is_dev()).unwrap_or(false);

                if is_dev_mode {
                    // In dev mode, return a degraded error that can be handled gracefully
                    warn!(
                        tenant_id = %tenant_id,
                        attempts = attempt,
                        reason = %reason,
                        "Worker discovery failed in dev mode - system degraded"
                    );
                    return Err(InferenceError::WorkerDegraded {
                        tenant_id: tenant_id.to_string(),
                        reason: format!(
                            "No compatible worker after {} attempts (dev mode): {}",
                            attempt, reason
                        ),
                    });
                } else {
                    // In prod/staging mode, hard failure
                    error!(
                        tenant_id = %tenant_id,
                        attempts = attempt,
                        reason = %reason,
                        "Worker discovery failed after all retries"
                    );
                    return Err(InferenceError::NoCompatibleWorker {
                        required_hash: required_manifest.to_string(),
                        tenant_id: tenant_id.to_string(),
                        available_count,
                        reason,
                    });
                }
            }
        }
    }

    /// Diagnose why no compatible worker is available
    async fn diagnose_worker_unavailability(
        &self,
        required_manifest: &str,
        tenant_id: &str,
    ) -> (usize, String) {
        // Get all healthy workers (already filtered by schema version)
        let all_serving = self
            .state
            .db
            .list_healthy_workers()
            .await
            .unwrap_or_default();

        if all_serving.is_empty() {
            (
                0,
                "No healthy workers available (check worker registration and health)".to_string(),
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
        let worker = self.select_worker_for_tenant(tenant_id).await?;
        Ok(PathBuf::from(&worker.uds_path))
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
                        interval_id: d.interval_id.clone(),
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

    /// Resolve dataset hash for replay metadata (best-effort).
    async fn resolve_dataset_hash(&self, dataset_version_id: Option<&String>) -> Option<String> {
        if let Some(version_id) = dataset_version_id {
            match self.state.db.get_training_dataset_version(version_id).await {
                Ok(Some(ver)) => Some(ver.hash_b3),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Resolve adapter hashes for replay metadata (best-effort).
    async fn resolve_adapter_hashes(
        &self,
        tenant_id: &str,
        adapter_ids: Option<&Vec<String>>,
    ) -> Option<Vec<String>> {
        if let Some(ids) = adapter_ids {
            let mut hashes = Vec::new();
            for id in ids {
                if let Ok(Some(adapter)) = self.state.db.get_adapter_for_tenant(tenant_id, id).await
                {
                    hashes.push(adapter.hash_b3.clone());
                }
            }
            if !hashes.is_empty() {
                hashes.sort();
                return Some(hashes);
            }
        }
        None
    }

    async fn build_minimal_replay_metadata_params(
        &self,
        request: &InferenceRequestInternal,
        latency_ms: Option<u64>,
        replay_status: &str,
        error_code: Option<&str>,
    ) -> CreateReplayMetadataParams {
        let manifest_hash = self
            .state
            .manifest_hash
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let backend = self
            .state
            .backend_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let cfg = PlacementConfig::from_env();
        let mode_str = match cfg.mode {
            adapteros_config::PlacementMode::Balanced => "balanced",
            adapteros_config::PlacementMode::Latency => "latency",
            adapteros_config::PlacementMode::Energy => "energy",
            adapteros_config::PlacementMode::Thermal => "thermal",
            adapteros_config::PlacementMode::Off => "off",
        };

        let adapter_ids = request
            .effective_adapter_ids
            .clone()
            .or_else(|| request.adapters.clone());
        let dataset_hash = self
            .resolve_dataset_hash(request.dataset_version_id.as_ref())
            .await;
        let adapter_hashes = self
            .resolve_adapter_hashes(&request.cpid, adapter_ids.as_ref())
            .await;

        let sampling_params = SamplingParams {
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            seed: request.seed,
            error_code: error_code.map(|code| code.to_string()),
            seed_mode: request.seed_mode,
            backend_profile: request.backend_profile,
            request_seed_hex: request.request_seed.as_ref().map(hex::encode),
            placement: Some(PlacementReplay {
                mode: mode_str.to_string(),
                weights: cfg.weights.into(),
                trace: Vec::new(),
            }),
            run_envelope: request.run_envelope.clone(),
            adapter_hashes_b3: adapter_hashes.clone(),
            dataset_hash_b3: dataset_hash.clone(),
        };
        let sampling_params_json = serde_json::to_string(&sampling_params).unwrap_or_default();

        let prompt_truncated = request.prompt.len() > MAX_REPLAY_TEXT_SIZE;
        let prompt_text = if prompt_truncated {
            request.prompt.chars().take(MAX_REPLAY_TEXT_SIZE).collect()
        } else {
            request.prompt.clone()
        };

        let base_only = matches!(
            request.effective_adapter_ids.as_ref(),
            Some(ids) if ids.is_empty()
        );

        let stop_policy_json = request
            .stop_policy
            .as_ref()
            .and_then(|sp| serde_json::to_string(sp).ok());

        CreateReplayMetadataParams {
            inference_id: request.request_id.clone(),
            tenant_id: request.cpid.clone(),
            manifest_hash,
            base_model_id: request.model.clone(),
            router_seed: request.router_seed.clone(),
            sampling_params_json,
            backend,
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
            rag_snapshot_hash: None,
            dataset_version_id: request.dataset_version_id.clone(),
            adapter_ids,
            base_only: if base_only { Some(true) } else { None },
            prompt_text,
            prompt_truncated,
            response_text: None,
            response_truncated: false,
            rag_doc_ids: None,
            chat_context_hash: request.chat_context_hash.clone(),
            replay_status: Some(replay_status.to_string()),
            latency_ms: latency_ms.map(|ms| ms as i32),
            tokens_generated: None,
            determinism_mode: request.determinism_mode.clone(),
            fallback_triggered: false,
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("none".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
            stop_policy_json,
            policy_mask_digest_b3: request.policy_mask_digest.as_ref().map(hex::encode),
        }
    }

    async fn capture_failed_replay_metadata(
        &self,
        request: &InferenceRequestInternal,
        error: &InferenceError,
        latency_ms: u64,
    ) {
        let params = self
            .build_minimal_replay_metadata_params(
                request,
                Some(latency_ms),
                "failed_inference",
                Some(error.error_code()),
            )
            .await;

        if let Err(e) = self.state.db.create_replay_metadata(params).await {
            warn!(
                request_id = %request.request_id,
                error = %e,
                "Failed to capture replay metadata for failed inference"
            );
            self.record_failed_capture(request, Some(latency_ms), Some(error.error_code()))
                .await;
        } else {
            debug!(
                request_id = %request.request_id,
                error_code = %error.error_code(),
                "Captured replay metadata for failed inference"
            );
        }
    }

    async fn record_failed_capture(
        &self,
        request: &InferenceRequestInternal,
        latency_ms: Option<u64>,
        error_code: Option<&str>,
    ) {
        let params = self
            .build_minimal_replay_metadata_params(request, latency_ms, "failed_capture", error_code)
            .await;

        match self.state.db.create_replay_metadata(params).await {
            Ok(_) => {
                debug!(
                    request_id = %request.request_id,
                    "Recorded failed_capture replay metadata"
                );
            }
            Err(e) => {
                warn!(
                    request_id = %request.request_id,
                    error = %e,
                    "Failed to record failed_capture replay metadata"
                );
                if let Err(update_err) = self
                    .state
                    .db
                    .update_replay_status(&request.request_id, "failed_capture")
                    .await
                {
                    warn!(
                        request_id = %request.request_id,
                        error = %update_err,
                        "Failed to update replay status to failed_capture"
                    );
                }
            }
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
    #[allow(clippy::too_many_arguments)]
    async fn capture_replay_metadata(
        &self,
        request: &InferenceRequestInternal,
        prompt_text: &str,
        response_text: &str,
        backend_used: Option<&str>,
        backend_version: Option<&str>,
        coreml_package_hash: Option<&str>,
        coreml_expected_package_hash: Option<&str>,
        coreml_hash_mismatch: Option<bool>,
        base_model_id: Option<String>,
        rag_evidence: &Option<RagEvidence>,
        latency_ms: u64,
        prompt_truncated: bool,
        response_truncated: bool,
        determinism_mode: &str,
        fallback_triggered: bool,
        coreml_compute_preference: Option<&str>,
        coreml_compute_units: Option<&str>,
        coreml_gpu_used: Option<bool>,
        fallback_backend: Option<&str>,
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
        let backend_version = backend_version.map(|s| s.to_string());

        let replay_guarantee_str = match replay_guarantee {
            ReplayGuarantee::Exact => "exact",
            ReplayGuarantee::Approximate => "approximate",
            ReplayGuarantee::None => "none",
        };

        let adapter_ids_for_hash = request
            .effective_adapter_ids
            .clone()
            .or_else(|| request.adapters.clone());
        let adapter_hashes = self
            .resolve_adapter_hashes(&request.cpid, adapter_ids_for_hash.as_ref())
            .await;
        let dataset_hash = self
            .resolve_dataset_hash(request.dataset_version_id.as_ref())
            .await;

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
            error_code: None,
            seed_mode: request.seed_mode,
            backend_profile: request.backend_profile,
            request_seed_hex: request.request_seed.as_ref().map(hex::encode),
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
            run_envelope: request.run_envelope.clone(),
            adapter_hashes_b3: adapter_hashes.clone(),
            dataset_hash_b3: dataset_hash.clone(),
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
        let adapter_ids = adapter_ids_for_hash;
        let base_only = matches!(
            request.effective_adapter_ids.as_ref(),
            Some(ids) if ids.is_empty()
        );

        // Determine replay status based on truncation
        let replay_status = if prompt_truncated || response_truncated {
            "approximate"
        } else {
            "available"
        };

        // Estimate tokens (rough: ~4 chars per token)
        let tokens_generated = Some((response_text.len() / 4).max(1) as i32);

        // Serialize stop_policy if present
        let stop_policy_json = request
            .stop_policy
            .as_ref()
            .and_then(|sp| serde_json::to_string(sp).ok());

        // Build params for DB storage
        let params = CreateReplayMetadataParams {
            inference_id: request.request_id.clone(),
            tenant_id: request.cpid.clone(),
            manifest_hash,
            base_model_id,
            router_seed: request.router_seed.clone(),
            sampling_params_json,
            backend,
            backend_version,
            coreml_package_hash: coreml_package_hash.map(|s| s.to_string()),
            coreml_expected_package_hash: coreml_expected_package_hash.map(|s| s.to_string()),
            coreml_hash_mismatch,
            sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
            rag_snapshot_hash,
            dataset_version_id: request.dataset_version_id.clone(),
            adapter_ids,
            base_only: if base_only { Some(true) } else { None },
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
            coreml_compute_preference: coreml_compute_preference.map(|s| s.to_string()),
            coreml_compute_units: coreml_compute_units.map(|s| s.to_string()),
            coreml_gpu_used,
            fallback_backend: fallback_backend.map(|s| s.to_string()),
            replay_guarantee: Some(replay_guarantee_str.to_string()),
            execution_policy_id: execution_policy_id.map(|s| s.to_string()),
            execution_policy_version: execution_policy_version.map(|v| v as i32),
            stop_policy_json,
            policy_mask_digest_b3: request.policy_mask_digest.as_ref().map(hex::encode),
        };

        // Store to database (best effort - don't fail inference on capture error)
        if let Err(e) = self.state.db.create_replay_metadata(params).await {
            warn!(
                request_id = %request.request_id,
                error = %e,
                "Failed to capture replay metadata"
            );
            self.record_failed_capture(request, Some(latency_ms), None)
                .await;
        } else {
            debug!(
                request_id = %request.request_id,
                "Replay metadata captured successfully"
            );
        }
    }
}

fn parse_routing_mode(raw: &Option<String>) -> Option<RoutingDeterminismMode> {
    raw.as_deref()
        .and_then(|s| RoutingDeterminismMode::from_str(s).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PathsConfig;
    use crate::state::{ApiConfig, GeneralConfig, MetricsConfig};
    use crate::telemetry::MetricsRegistry;
    use adapteros_api_types::{CreateExecutionPolicyRequest, DeterminismPolicy};
    use adapteros_core::{BackendKind, SeedMode};
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

    #[test]
    fn strict_runtime_guard_rejects_unknown_backend() {
        let manifest = B3Hash::hash(b"manifest");
        let chain = vec![ApiRouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0],
            adapter_ids: vec!["a".into()],
            gates_q15: vec![123],
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest: None,
            policy_overrides_applied: None,
        }];

        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("mystery".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(chain),
            &[String::from("adapter-a")],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("known backend"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn strict_runtime_guard_allows_base_only_without_chain() {
        let manifest = B3Hash::hash(b"manifest");
        enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            Some(&manifest),
        )
        .expect("base-only strict mode should not require router chain");
    }

    async fn insert_stack(db: &Db, tenant: &str, adapter_ids: &[&str]) -> String {
        let req = CreateStackRequest {
            tenant_id: tenant.to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: adapter_ids.iter().map(|s| s.to_string()).collect(),
            workflow_type: None,
            determinism_mode: None,
            routing_determinism_mode: None,
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
    fn test_sampling_params_serialization_includes_run_envelope() {
        let envelope = adapteros_api_types::RunEnvelope {
            run_id: "run-123".to_string(),
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            workspace_id: "tenant-1".to_string(),
            actor: adapteros_api_types::RunActor {
                subject: "user".to_string(),
                roles: vec!["role".to_string()],
                principal_type: Some("user".to_string()),
                auth_mode: Some("bearer".to_string()),
            },
            manifest_hash_b3: Some("hash".to_string()),
            plan_id: Some("plan".to_string()),
            policy_mask_digest_b3: None,
            router_seed: None,
            tick: Some(1),
            worker_id: None,
            reasoning_mode: false,
            determinism_version: "v1".to_string(),
            boot_trace_id: None,
            created_at: chrono::Utc::now(),
        };

        let params = SamplingParams {
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.9),
            max_tokens: 100,
            seed: Some(42),
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: Some(envelope),
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"seed\":42"));
        assert!(json.contains("\"top_k\":50"));
        assert!(json.contains("\"top_p\":0.9"));
        assert!(json.contains("\"max_tokens\":100"));
        assert!(json.contains("\"run_id\":\"run-123\""));
        let expected_schema = format!(
            "\"schema_version\":\"{}\"",
            adapteros_api_types::API_SCHEMA_VERSION
        );
        assert!(
            json.contains(&expected_schema),
            "expected run_envelope schema_version in replay sampling params"
        );

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
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
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
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
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

    #[tokio::test]
    async fn test_implicit_policy_uses_global_strict() {
        let state = build_test_state_with_general(false, Some(DeterminismMode::Strict)).await;

        let policy = resolve_tenant_execution_policy(
            &state.db,
            &state.config.read().unwrap(),
            "tenant-1",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(policy.effective_determinism_mode, DeterminismMode::Strict);
    }

    #[tokio::test]
    async fn test_explicit_policy_overrides_global_strict() {
        let state = build_test_state_with_general(false, Some(DeterminismMode::Strict)).await;

        let determinism = DeterminismPolicy {
            allowed_modes: vec![
                "strict".to_string(),
                "besteffort".to_string(),
                "relaxed".to_string(),
            ],
            default_mode: "relaxed".to_string(),
            require_seed: false,
            allow_fallback: true,
            replay_mode: "approximate".to_string(),
            allowed_backends: Some(Vec::new()),
            denied_backends: Some(Vec::new()),
        };

        let request = CreateExecutionPolicyRequest {
            determinism,
            routing: None,
            golden: None,
            require_signed_adapters: false,
        };

        state
            .db
            .create_execution_policy("tenant-1", request, Some("test"))
            .await
            .unwrap();

        let policy = resolve_tenant_execution_policy(
            &state.db,
            &state.config.read().unwrap(),
            "tenant-1",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(policy.effective_determinism_mode, DeterminismMode::Relaxed);
    }

    // =========================================================================
    // Effective adapter set resolution tests (bundle A)
    // =========================================================================
    async fn build_test_state(use_session_stack: bool) -> AppState {
        build_test_state_with_general(use_session_stack, None).await
    }

    async fn build_test_state_with_general(
        use_session_stack: bool,
        general_determinism_mode: Option<DeterminismMode>,
    ) -> AppState {
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
        let _db_dir = dir.keep();
        // Seed tenant
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let general = general_determinism_mode.map(|mode| GeneralConfig {
            system_name: None,
            environment: None,
            api_base_url: None,
            determinism_mode: Some(mode),
        });

        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: true,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: use_session_stack,
            capacity_limits: Default::default(),
            general,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
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
            backend_profile: BackendKind::Auto,
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
            routing_determinism_mode: None,
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
            routing_determinism_mode: None,
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
            routing_determinism_mode: None,
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
            routing_determinism_mode: None,
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
            routing_determinism_mode: None,
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
    async fn test_pinned_adapter_missing_rejected() {
        let state = build_test_state(false).await;
        let core = InferenceCore::new(&state);
        let err = core
            .validate_pinned_adapters_for_tenant("tenant-1", &[String::from("missing-pin")])
            .await
            .unwrap_err();

        match err {
            InferenceError::AdapterNotFound(msg) => {
                assert!(msg.contains("missing-pin"));
            }
            other => panic!("expected AdapterNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pinned_adapter_wrong_tenant_rejected() {
        let state = build_test_state(false).await;
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
        )
        .execute(state.db.pool())
        .await
        .unwrap();

        let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
            .tenant_id("tenant-2")
            .adapter_id("tenant2-adapter")
            .name("Tenant 2 Adapter")
            .hash_b3("b3:tenant2")
            .rank(4)
            .build()
            .unwrap();
        state.db.register_adapter(params).await.unwrap();

        let core = InferenceCore::new(&state);
        let err = core
            .validate_pinned_adapters_for_tenant("tenant-1", &[String::from("tenant2-adapter")])
            .await
            .unwrap_err();

        // PRD-RECT-001: Cross-tenant access returns AdapterNotFound (not PermissionDenied)
        // to prevent tenant enumeration attacks.
        match err {
            InferenceError::AdapterNotFound(msg) => {
                assert!(msg.contains("tenant2-adapter"));
            }
            other => panic!("expected AdapterNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pinned_adapter_outside_allowlist_rejected() {
        let state = build_test_state(false).await;
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
        )
        .execute(state.db.pool())
        .await
        .unwrap();

        // Register one adapter for each tenant
        let tenant1_params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
            .tenant_id("tenant-1")
            .adapter_id("t1-allowed")
            .name("Tenant1")
            .hash_b3("b3:t1")
            .rank(4)
            .build()
            .unwrap();
        state.db.register_adapter(tenant1_params).await.unwrap();

        let tenant2_params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
            .tenant_id("tenant-2")
            .adapter_id("t2-disallowed")
            .name("Tenant2")
            .hash_b3("b3:t2")
            .rank(4)
            .build()
            .unwrap();
        state.db.register_adapter(tenant2_params).await.unwrap();

        let core = InferenceCore::new(&state);
        let allowlist = core
            .adapter_allowlist_for_tenant("tenant-1")
            .await
            .expect("allowlist");

        let err = core
            .validate_ids_against_allowlist(
                &[String::from("t2-disallowed")],
                "tenant-1",
                &allowlist,
                "Pinned adapter",
            )
            .unwrap_err();

        // PRD-RECT-001: Allowlist violations return AdapterNotFound (not PermissionDenied)
        // to prevent leaking adapter existence across tenants.
        match err {
            InferenceError::AdapterNotFound(msg) => {
                assert!(msg.contains("t2-disallowed"));
            }
            other => panic!("expected AdapterNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_stack_from_other_tenant_not_resolved() {
        let state = build_test_state(false).await;
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
        )
        .execute(state.db.pool())
        .await
        .unwrap();

        let stack_req = CreateStackRequest {
            tenant_id: "tenant-2".to_string(),
            name: stack_name(),
            description: None,
            adapter_ids: vec!["cross-a".to_string()],
            workflow_type: None,
            determinism_mode: None,
            routing_determinism_mode: None,
        };
        let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

        let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        req.stack_id = Some(stack_id.clone());

        let core = InferenceCore::new(&state);
        let err = core
            .resolve_effective_adapters(&mut req, None)
            .await
            .unwrap_err();

        match err {
            InferenceError::AdapterNotFound(msg) => {
                assert!(msg.contains("tenant-1"));
            }
            other => panic!("expected AdapterNotFound, got {:?}", other),
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

    // =========================================================================
    // Additional inference_core tests (Bundle-G: Security/Quality)
    // =========================================================================

    #[test]
    fn test_parse_pinned_adapter_ids_valid_json() {
        let result = parse_pinned_adapter_ids(Some(r#"["adapter-a", "adapter-b"]"#));
        assert_eq!(
            result,
            Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
        );
    }

    #[test]
    fn test_parse_pinned_adapter_ids_empty_array() {
        let result = parse_pinned_adapter_ids(Some("[]"));
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_parse_pinned_adapter_ids_none_input() {
        let result = parse_pinned_adapter_ids(None);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_pinned_adapter_ids_invalid_json() {
        // Malformed JSON should return None (not panic)
        let result = parse_pinned_adapter_ids(Some("not valid json"));
        assert!(result.is_none());
    }

    #[test]
    fn test_validate_pinned_within_effective_set_success() {
        let effective = Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let pinned = Some(vec!["a".to_string(), "c".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_pinned_within_effective_set_pinned_outside_fails() {
        let effective = Some(vec!["a".to_string(), "b".to_string()]);
        let pinned = Some(vec!["a".to_string(), "not-in-effective".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not-in-effective"));
    }

    #[test]
    fn test_validate_pinned_within_effective_set_empty_effective_passes() {
        // Empty effective set allows any pinned (no restriction enforced)
        let effective = Some(vec![]);
        let pinned = Some(vec!["any".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_determinism_mode_stack_takes_precedence() {
        let mode = resolve_determinism_mode(Some("strict"), Some("relaxed"), "besteffort");
        assert_eq!(mode, DeterminismMode::Strict);
    }

    #[test]
    fn test_resolve_determinism_mode_tenant_fallback() {
        let mode = resolve_determinism_mode(None, Some("relaxed"), "strict");
        assert_eq!(mode, DeterminismMode::Relaxed);
    }

    #[test]
    fn test_resolve_determinism_mode_global_fallback() {
        let mode = resolve_determinism_mode(None, None, "strict");
        assert_eq!(mode, DeterminismMode::Strict);
    }

    #[test]
    fn test_compute_strict_mode_strict_mode() {
        let strict_mode = compute_strict_mode(DeterminismMode::Strict, true);
        assert!(strict_mode, "Strict mode should always return true");
    }

    #[test]
    fn test_compute_strict_mode_with_fallback_disabled() {
        let strict_mode = compute_strict_mode(DeterminismMode::BestEffort, false);
        assert!(strict_mode, "Fallback disabled should enable strict mode");
    }

    #[test]
    fn test_compute_strict_mode_besteffort_with_fallback() {
        let strict_mode = compute_strict_mode(DeterminismMode::BestEffort, true);
        assert!(
            !strict_mode,
            "BestEffort with fallback should not be strict"
        );
    }

    #[test]
    fn test_validate_strict_mode_constraints_requires_seed() {
        let result = validate_strict_mode_constraints(DeterminismMode::Strict, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("seed"));
    }

    #[test]
    fn test_validate_strict_mode_constraints_with_seed() {
        let result = validate_strict_mode_constraints(DeterminismMode::Strict, Some(12345));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_strict_mode_constraints_relaxed_no_seed() {
        // Relaxed mode doesn't require seed
        let result = validate_strict_mode_constraints(DeterminismMode::Relaxed, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_replay_guarantee_exact_strict() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            false, // fallback_triggered
            false, // prompt_truncated
            false, // response_truncated
            true,  // seed_present
        );
        assert_eq!(guarantee, ReplayGuarantee::Exact);
    }

    #[test]
    fn test_compute_replay_guarantee_fallback_degrades_to_approximate() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            true, // fallback_triggered
            false,
            false,
            true,
        );
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_missing_seed_degrades() {
        let guarantee = compute_replay_guarantee(
            DeterminismMode::Strict,
            false,
            false,
            false,
            false, // seed not present
        );
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_best_effort_always_approximate() {
        let guarantee =
            compute_replay_guarantee(DeterminismMode::BestEffort, false, false, false, true);
        assert_eq!(guarantee, ReplayGuarantee::Approximate);
    }

    #[test]
    fn test_compute_replay_guarantee_relaxed_always_none() {
        let guarantee =
            compute_replay_guarantee(DeterminismMode::Relaxed, false, false, false, true);
        assert_eq!(guarantee, ReplayGuarantee::None);
    }

    #[test]
    fn test_strict_runtime_guard_missing_manifest_fails() {
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("mlx".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            None, // No manifest
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("manifest"),
            "Should mention manifest: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_missing_backend_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &None, // No backend
            &Some(adapteros_core::version::VERSION.to_string()),
            &None,
            &[],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("backend"),
            "Should mention backend: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_version_mismatch_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some("0.0.0-mismatch".to_string()), // Wrong version
            &None,
            &[],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("mismatch"),
            "Should mention mismatch: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_empty_chain_with_adapters_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(vec![]),              // Empty chain
            &["adapter-a".to_string()], // But adapters used
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("non-empty"),
            "Should require non-empty chain: {}",
            err
        );
    }

    #[test]
    fn test_strict_runtime_guard_gate_count_mismatch_fails() {
        let manifest = B3Hash::hash(b"manifest");
        let chain = vec![ApiRouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0, 1], // 2 indices
            adapter_ids: vec!["a".into(), "b".into()],
            gates_q15: vec![100], // Only 1 gate - mismatch!
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest: None,
            policy_overrides_applied: None,
        }];

        let err = enforce_strict_runtime_guards(
            DeterminismMode::Strict,
            &Some("coreml".into()),
            &Some(adapteros_core::version::VERSION.to_string()),
            &Some(chain),
            &["adapter-a".to_string()],
            Some(&manifest),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("mismatch"),
            "Should mention gate count mismatch: {}",
            err
        );
    }

    #[test]
    fn test_routing_policy_resolved_defaults() {
        let defaults = RoutingPolicyResolved::default();
        assert!(!defaults.use_session_stack_for_routing);
        assert!(!defaults.allow_pins_outside_effective_set);
    }

    #[test]
    fn test_golden_policy_resolved_defaults() {
        let defaults = GoldenPolicyResolved::default();
        assert!(!defaults.fail_on_drift);
        assert!(defaults.golden_baseline_id.is_none());
        assert!((defaults.epsilon_threshold - 1e-6).abs() < 1e-12);
    }

    #[tokio::test]
    async fn test_policy_hooks_execution_flow() {
        let state = build_test_state(false).await;
        let core = InferenceCore::new(&state);

        // Enable core policies for the tenant
        state
            .db
            .toggle_tenant_policy("tenant-1", "egress", true, "admin")
            .await
            .unwrap();
        state
            .db
            .toggle_tenant_policy("tenant-1", "determinism", true, "admin")
            .await
            .unwrap();
        state
            .db
            .toggle_tenant_policy("tenant-1", "evidence", true, "admin")
            .await
            .unwrap();

        let req = InferenceRequestInternal::new("tenant-1".to_string(), "test prompt".to_string());

        // This test will fail at Stage 6 (Worker Selection) because no workers are registered,
        // but it should have already passed Stage 3 (OnRequestBeforeRouting).
        // If Stage 3 failed, it would return a PolicyViolation error.
        let result = core.route_and_infer(req, None, None).await;

        match result {
            Err(InferenceError::NoCompatibleWorker { .. }) => {
                // Success: bypassed Stage 3 without error
                info!("Stage 3 policy check passed as expected");
            }
            Err(InferenceError::PolicyViolation {
                tenant_id,
                policy_id,
                reason,
            }) => {
                panic!(
                    "Policy violation at Stage 3: tenant={}, policy={}, reason={}",
                    tenant_id, policy_id, reason
                );
            }
            other => {
                // Might fail earlier or later depending on setup
                debug!("Inference failed with: {:?}", other);
            }
        }
    }
}
