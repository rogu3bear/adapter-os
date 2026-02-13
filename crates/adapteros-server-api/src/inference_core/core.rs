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
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 2: Adapter Resolution                                                │
//! │  - Load adapters from DB by adapter_ids or stack_id                         │
//! │  - Apply pinned adapter overrides (CHAT-PIN-02)                             │
//! │  - Validate all adapters belong to tenant                                   │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 3: Policy Hooks (OnRequestBeforeRouting)                             │
//! │  - Execute policy packs: egress, determinism, isolation, evidence           │
//! │  - Generate policy mask for router                                          │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 4: RAG Context Retrieval (if enabled)                                │
//! │  - Query collection for relevant chunks                                     │
//! │  - Compute rag_snapshot_hash for replay                                     │
//! │  - Inject context into prompt                                               │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 5: Router Decision                                                   │
//! │  - K-sparse top-K selection with Q15 gates                                  │
//! │  - Deterministic tie-breaking (score DESC, stable_id ASC)                   │
//! │  - Entropy floor enforcement                                                │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 6: Worker Selection                                                  │
//! │  - Find worker with required adapters loaded                                │
//! │  - Placement constraints (memory, backend compatibility)                    │
//! │  - Hot-swap triggers if adapters not loaded                                 │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 7: Policy Hooks (OnBeforeInference)                                  │
//! │  - Final policy checks before worker call                                   │
//! │  - Rate limiting, quota enforcement                                         │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 8: Worker Inference (UDS)                                            │
//! │  - Send request over Unix Domain Socket                                     │
//! │  - Execute on CoreML/Metal/MLX backend                                      │
//! │  - Collect router decisions per token                                       │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 9: Policy Hooks (OnAfterInference)                                   │
//! │  - Post-inference validation                                                │
//! │  - Output filtering (if configured)                                         │
//! └────────────────────────────────────────────────────────────────────────────┘
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Stage 10: Evidence & Telemetry                                             │
//! │  - Store replay metadata (manifest_hash, router_seed, etc.)                 │
//! │  - Emit routing telemetry event                                             │
//! │  - Store RAG evidence (if applicable)                                       │
//! └────────────────────────────────────────────────────────────────────────────┘
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
//! by design (sorted by score DESC, then by stable_id ASC for ties). The `router_seed`
//! field is stored for **audit purposes only** - it does not affect routing decisions.
//!
//! For replay, pass a `ReplayContext` to enforce manifest/backend compatibility
//! and skip metadata capture for the replay itself.

use super::adapters::{map_router_decision_chain, map_router_decisions, parse_routing_mode};
use super::determinism::validate_strict_mode_constraints;
use super::diag::{extract_error_code, suggest_recovery, DiagRunContext};
use super::policy::{resolve_tenant_execution_policy, GoldenPolicyResolved};
use super::replay::{compute_replay_guarantee, enforce_strict_runtime_guards};
use super::validation::{parse_pinned_adapter_ids, validate_pinned_within_effective_set};
use crate::chat_session_config::ChatSessionConfig;
use crate::citations::collect_citations_for_adapters;
use crate::handlers::rag_common::{
    retrieve_rag_context, store_rag_evidence, EvidenceModelContext, RagContextResult,
};
use crate::middleware::policy_enforcement::{compute_policy_mask_digest, enforce_at_hook};
use crate::state::AppState;
use crate::types::{
    new_run_envelope, new_run_envelope_no_tick, set_policy_mask, set_router_seed,
    set_worker_context, ChunkReference, InferenceError, InferenceRequestInternal, InferenceResult,
    PlacementReplay, PlacementTraceEntry, RagEvidence, ReplayContext, RouterCandidateRecord,
    RouterDecisionRecord, SamplingParams, TokenUsage, WorkerInferRequest, MAX_REPLAY_TEXT_SIZE,
    SAMPLING_ALGORITHM_VERSION,
};
use crate::uds_client::{UdsClient, WorkerStreamEvent, WorkerStreamPaused, WorkerStreamToken};
use crate::uds_metrics::record_uds_timings;
use crate::worker_capabilities::{
    capability_reasons, parse_worker_capabilities, RequiredModes, WorkerCapabilityExclusion,
};
use crate::worker_selector::{RequiredCapabilities, WorkerRequirements, WorkerSelector};
use adapteros_api_types::inference::ReplayGuarantee;
use adapteros_api_types::{RunActor, RunEnvelope};
use adapteros_config::PlacementConfig;
use adapteros_core::{
    compute_key_id, identity::IdentityEnvelope, pinned_degradation_telemetry_ref_ids, B3Hash,
    BackendKind, BundleMetadataRef, EvidenceEnvelope, EvidenceScope, GuardLogLevel,
    PinnedDegradationEvidence, SeedScopeGuard,
};
use adapteros_db::workers::WorkerWithBinding;
use adapteros_db::{chat_sessions::ChatSession, CreateReplayMetadataParams};
use adapteros_deterministic_exec::{ExecutorEvent, TaskId};
use adapteros_policy::hooks::{HookContext, PolicyHook};
#[allow(unused_imports)]
use adapteros_telemetry::diagnostics::DiagStage;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use adapteros_telemetry::{
    build_inference_metrics_event, build_routing_event, InferenceMetricsEvent,
    RoutingTelemetryEvent,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use futures_util::StreamExt;
use hex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, info_span, warn};

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
        stream_tx: Option<mpsc::Sender<WorkerStreamToken>>,
        pause_tx: Option<mpsc::Sender<WorkerStreamPaused>>,
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

        // Create trace context for diagnostics
        let trace_context = adapteros_telemetry::tracing::TraceContext::new_root();

        // Initialize diagnostic run context (if diagnostics enabled)
        let diag_ctx = DiagRunContext::try_new(
            self.state,
            &request.request_id,
            &request.cpid,
            &trace_context,
        );

        // Emit RunStarted
        if let Some(ref ctx) = diag_ctx {
            ctx.emit_run_started(is_replay);
        }

        // Register inference in state tracker as Running (with idempotency check)
        if let Some(ref tracker) = self.state.inference_state_tracker {
            if !tracker.register_inference(
                request.request_id.clone(),
                request.cpid.clone(),
                is_replay,
            ) {
                // Duplicate request detected - another request with same ID is already in-flight
                return Err(InferenceError::DuplicateRequest {
                    request_id: request.request_id.clone(),
                });
            }
        }

        let result = async {
        let mut all_policy_decisions = Vec::new();
        if request.run_envelope.is_none() {
            if let Some(claims) = request.claims.as_ref() {
                let envelope = if should_capture {
                    new_run_envelope(
                        self.state,
                        claims,
                        request.request_id.clone(),
                        request.reasoning_mode,
                    )
                } else {
                    new_run_envelope_no_tick(
                        self.state,
                        claims,
                        request.request_id.clone(),
                        request.reasoning_mode,
                    )
                };
                request.run_envelope = Some(envelope);
            } else {
                let tick = if should_capture {
                    self.state
                        .tick_ledger
                        .as_ref()
                        .map(|ledger| ledger.increment_tick())
                } else {
                    None
                };
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
                    tick,
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
        if should_capture {
            if let Some(ledger) = self.state.tick_ledger.as_ref() {
                let (run_id, tick) = match request.run_envelope.as_mut() {
                    Some(envelope) => {
                        let tick = match envelope.tick {
                            Some(tick) => tick,
                            None => {
                                let tick = ledger.increment_tick();
                                envelope.tick = Some(tick);
                                tick
                            }
                        };
                        (Some(envelope.run_id.clone()), Some(tick))
                    }
                    None => (None, None),
                };

                if let (Some(run_id), Some(tick)) = (run_id, tick) {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(b"inference_run");
                    hasher.update(run_id.as_bytes());
                    let task_id = TaskId::from_bytes(*hasher.finalize().as_bytes());
                    let event = ExecutorEvent::inference_started(run_id.clone(), tick);
                    if let Err(e) = ledger.record_tick_at(tick, task_id, &event).await {
                        warn!(
                            run_id = %run_id,
                            tick,
                            error = %e,
                            "Failed to record inference tick ledger entry"
                        );
                    }
                }
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
            )
            .await?;
        }

        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            inference_span.record("adapters", tracing::field::display(effective.join(",")));
        } else if let Some(adapters) = request.adapters.as_ref() {
            inference_span.record("adapters", tracing::field::display(adapters.join(",")));
        }

        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            let correlation_ids = self
                .resolve_correlation_ids_for_adapters(&request.cpid, effective)
                .await;
            if !correlation_ids.is_empty() {
                info!(
                    request_id = %request.request_id,
                    correlation_id = %correlation_ids.join(","),
                    adapter_ids = %effective.join(","),
                    "Inference correlation resolved"
                );
            }
        }

        // Stage 3: Policy Hooks (OnRequestBeforeRouting)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │  - Execute policy packs: egress, determinism, isolation, evidence│
        // │  - Generate policy mask for router                               │
        // └─────────────────────────────────────────────────────────────────┘
        let hook_ctx = HookContext::new(
            request.cpid.clone(),
            request.request_id.clone(),
            PolicyHook::OnRequestBeforeRouting,
            "inference",
        )
        .with_input(request.prompt.clone())
        .with_metadata(
            "adapter_ids",
            serde_json::json!(request.effective_adapter_ids),
        );

        let decisions = enforce_at_hook(self.state, &hook_ctx)
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
        all_policy_decisions.extend(decisions);

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
        let coreml_mode = request.coreml_mode.unwrap_or(adapteros_types::coreml::CoreMLMode::CoremlPreferred);
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

        let is_prod_mode = self
            .state
            .runtime_mode
            .map(|mode| mode.is_prod())
            .unwrap_or(config.server.production_mode);
        if is_prod_mode {
            request.require_step = true;
            if execution_profile.profile.backend_profile == BackendKind::MlxBridge {
                return Err(InferenceError::ValidationError(
                    "Bulk MLX bridge backend is disabled in production; step loop is required"
                        .to_string(),
                ));
            }
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
        request.require_determinism = matches!(
            resolved_policy.effective_determinism_mode,
            adapteros_core::determinism_mode::DeterminismMode::Strict
        );

        // Extract values for use later in the function
        let resolved_mode = resolved_policy.effective_determinism_mode;
        let strict_mode = resolved_policy.strict_mode;
        let execution_policy = resolved_policy.policy.clone();

        // 0. Validate adapters are loadable (not archived/purged)
        // This is a defense-in-depth check - handlers should also validate
        self.validate_adapters_loadable(&request).await?;

        // 0.1 Gate on base model readiness (strictly per-tenant)
        //
        // If the request doesn't pin a specific model, prefer the tenant's active base model
        // (workspace_active_state) rather than the most recently updated status record.
        let effective_model_id = match request.model.as_deref() {
            Some(model_id) => Some(model_id.to_string()),
            None => self
                .state
                .db
                .get_workspace_active_state(&request.cpid)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!(
                        "Failed to fetch workspace active state: {}",
                        e
                    ))
                })?
                .and_then(|ws| ws.active_base_model_id),
        };

        let base_status = match effective_model_id.as_deref() {
            Some(model_id) => self
                .state
                .db
                .get_base_model_status_for_model(&request.cpid, model_id)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!("Failed to fetch base model status: {}", e))
                })?,
            None => self
                .state
                .db
                .get_base_model_status(&request.cpid)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!("Failed to fetch base model status: {}", e))
                })?,
        };

        let mut records: Vec<adapteros_db::models::BaseModelStatus> = Vec::new();
        if let Some(status) = base_status {
            records.push(status);
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
        if let Some(ref expected_base_model_id) = base_model_id {
            let adapter_ids = self.collect_adapter_ids_for_request(&request);
            if !adapter_ids.is_empty() {
                self.validate_adapter_base_models(
                    &request.cpid,
                    &adapter_ids,
                    expected_base_model_id,
                )
                .await?;
            }
        }

        // 1. Resolve worker UDS path and capture worker identifier for telemetry
        // For replay: enforce manifest/backend constraints first, then reuse standard selection.
        let (uds_path, selected_worker) = if let Some(ref ctx) = replay_context {
            let path = self
                .resolve_worker_path_for_replay(
                    &request,
                    &ctx.required_manifest_hash,
                    &ctx.required_backend,
                )
                .await?;
            let worker = self.select_worker_for_request(&request).await.ok();
            (path, worker)
        } else {
            let worker = self.select_worker_for_request(&request).await?;
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
        // │ - Evidence IDs returned for message binding            │
        // └─────────────────────────────────────────────────────────────────┘
        let (augmented_prompt, rag_evidence, pending_evidence_ids) = if request.rag_enabled {
            self.retrieve_and_augment_rag(&request).await?
        } else {
            (request.prompt.clone(), None, Vec::new())
        };

        // Stage 7: Policy Hooks (OnBeforeInference)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │  - Final policy checks before worker call                       │
        // │  - Rate limiting, quota enforcement                             │
        // └─────────────────────────────────────────────────────────────────┘
        let hook_ctx = HookContext::new(
            request.cpid.clone(),
            request.request_id.clone(),
            PolicyHook::OnBeforeInference,
            "inference",
        )
        .with_input(augmented_prompt.clone())
        .with_metadata(
            "adapter_ids",
            serde_json::json!(request.effective_adapter_ids),
        )
        .with_metadata("model_id", serde_json::json!(request.model))
        .with_metadata("worker_id", serde_json::json!(selected_worker_id));

        let decisions = enforce_at_hook(self.state, &hook_ctx)
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
        all_policy_decisions.extend(decisions);

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
            )
            .await?;
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

        let routing_policy = Some(execution_policy.routing.clone().unwrap_or_default());

        // Compute policy mask digest for the worker call
        let policy_mask_digest = compute_policy_mask_digest(&all_policy_decisions);
        request.policy_mask_digest_b3 = Some(policy_mask_digest);
        if let Some(ref mut envelope) = request.run_envelope {
            set_policy_mask(envelope, Some(&policy_mask_digest));
            set_worker_context(
                envelope,
                selected_worker.as_ref(),
                self.state.manifest_hash.clone(),
            );
        }

        // MLX main driver; CoreML for reasoning (inference) and optional preferred training backend.
        // 3. Create worker request with full sampling parameters
        let routing_mode = request
            .routing_determinism_mode
            .unwrap_or(RoutingDeterminismMode::Deterministic);
        let adapter_stable_ids = if routing_mode == RoutingDeterminismMode::Deterministic {
            match request.effective_adapter_ids.as_ref() {
                Some(effective_ids) if effective_ids.is_empty() => None,
                Some(effective_ids) => Some(
                    self.resolve_stable_ids_for_adapters(&request.cpid, effective_ids)
                        .await?,
                ),
                None => Some(self.resolve_stable_ids_for_tenant(&request.cpid).await?),
            }
        } else {
            None
        };

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
            policy_id: Some(execution_policy.id.clone()),
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
            adapter_stable_ids,
            routing_policy,
            placement: None,
            adapter_strength_overrides: request.adapter_strength_overrides.clone(),
            stop_policy: request.stop_policy.clone(),
            policy_mask_digest_b3: Some(policy_mask_digest),
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
        // Capture pre-UDS latency: time from request receipt to UDS call start
        let pre_uds_latency = start_time.elapsed();
        let pre_uds_latency_us = pre_uds_latency.as_micros() as u64;

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

        let map_uds_error = |e| match e {
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
        };

        let worker_response = if let Some(stream_tx) = stream_tx.clone() {
            let pause_tx = pause_tx.clone();
            // Wrap the UDS call in routing context to ensure task-local guard is set
            let worker_call = crate::uds_client::run_with_routing_context(async {
                uds_client
                    .infer_stream(
                        &uds_path,
                        worker_request,
                        worker_auth_token.as_deref(),
                        cancellation_token.clone(),
                    )
                    .await
            });

            let mut stream = worker_call.await.map_err(map_uds_error)?;

            let mut response: Option<crate::types::WorkerInferResponse> = None;
            while let Some(event) = stream.next().await {
                let event = event.map_err(map_uds_error)?;
                match event {
                    WorkerStreamEvent::Token(token) => {
                        if stream_tx.send(token).await.is_err() {
                            return Err(InferenceError::ClientClosed(
                                "Client disconnected during streaming".to_string(),
                            ));
                        }
                    }
                    WorkerStreamEvent::Complete(resp) => {
                        response = Some(*resp);
                        break;
                    }
                    WorkerStreamEvent::Error(message) => {
                        return Err(InferenceError::WorkerError(message));
                    }
                    WorkerStreamEvent::Paused(paused) => {
                        // Best-effort: forward pause info to the streaming client so it can
                        // navigate to the review UI. If the client is gone, ignore.
                        if let Some(ref pause_tx) = pause_tx {
                            let _ = pause_tx.send(paused.clone()).await;
                        }

                        // Register pause with server's pause tracker for human review
                        if let Some(ref tracker) = self.state.pause_tracker {
                            tracker.register_pause(request.cpid.clone(), paused.clone(), uds_path.clone());
                            info!(
                                pause_id = %paused.pause_id,
                                inference_id = %paused.inference_id,
                                "Registered paused inference for review"
                            );
                        } else {
                            warn!(
                                pause_id = %paused.pause_id,
                                "Received Paused event but pause tracker not configured"
                            );
                        }

                        // Update state tracker to Paused
                        if let Some(ref state_tracker) = self.state.inference_state_tracker {
                            state_tracker.mark_paused(
                                &paused.inference_id,
                                paused.pause_id.clone(),
                                paused.trigger_kind.clone(),
                                Some(paused.token_count as u32),
                            );
                        }
                        // Worker is blocked waiting for review - continue waiting for more events
                        // (Resume will be sent via UDS when review is submitted)
                    }
                }
            }

            response.ok_or_else(|| {
                InferenceError::WorkerError("No response received from worker".to_string())
            })?
        } else {
            // Wrap the UDS call in routing context to ensure task-local guard is set
            // Use infer_with_phase_timings to capture UDS latency metrics (PRD-11)
            let worker_call = crate::uds_client::run_with_routing_context(async {
                uds_client
                    .infer_with_phase_timings(
                        &uds_path,
                        worker_request,
                        worker_auth_token.as_deref(),
                        cancellation_token.clone(),
                    )
                    .await
            });

            let (response, uds_timings) = worker_call.await.map_err(map_uds_error)?;

            // Record UDS phase timings to metrics registry (PRD-11)
            record_uds_timings(
                &self.state.metrics_registry,
                &uds_timings,
                Some("/inference"),
                selected_worker_id.as_deref(),
                true, // success
            )
            .await;

            response
        };

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

        // 5a. AARA Lifecycle: Compute max gate score for abstention check
        let max_gate_score = router_decisions
            .iter()
            .flat_map(|d| d.candidates.iter())
            .map(|c| c.raw_score)
            .fold(0.0_f32, f32::max);

        // Check if we should abstain due to low confidence
        let abstention_info = if request.should_abstain(max_gate_score) {
            let threshold = request.effective_abstention_threshold();
            tracing::info!(
                request_id = %request.request_id,
                max_gate_score = max_gate_score,
                threshold = threshold,
                "Abstaining due to low confidence"
            );
            Some(crate::types::AbstentionInfo::low_confidence(max_gate_score, threshold))
        } else if worker_response.trace.router_summary.adapters_used.is_empty() {
            tracing::info!(
                request_id = %request.request_id,
                "Abstaining due to no adapters available"
            );
            Some(crate::types::AbstentionInfo::no_adapters())
        } else {
            None
        };

        // 5b. Persist per-token routing chain for audit
        if let Some(chain) = router_decision_chain.as_ref() {
            let records_result: Result<Vec<adapteros_db::RoutingDecisionChainRecord>, _> = chain
                .iter()
                .map(|entry| {
                    let hash_json = entry.decision_hash.as_ref().and_then(|h| {
                        match serde_json::to_string(h) {
                            Ok(json) => Some(json),
                            Err(e) => {
                                tracing::warn!(
                                    request_id = %request.request_id,
                                    error = %e,
                                    "Failed to serialize decision_hash for routing chain audit"
                                );
                                None
                            }
                        }
                    });
                    adapteros_db::make_chain_record_from_api(
                        &request.cpid,
                        &request.request_id,
                        Some(&request.request_id),
                        entry,
                        hash_json,
                    )
                })
                .collect();

            match records_result {
                Ok(records) => {
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
                Err(e) => {
                    warn!(
                        error = %e,
                        request_id = %request.request_id,
                        "Failed to serialize routing decision chain records - audit trail corrupted"
                    );
                }
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
        let token_usage = Self::resolve_token_usage(&worker_response);
        let tokens_generated = token_usage
            .as_ref()
            .map(|usage| usage.completion_tokens as usize)
            .or(if worker_response.trace.token_count > 0 {
                Some(worker_response.trace.token_count)
            } else {
                None
            })
            .unwrap_or(0);
        let tokens_generated_event = if token_usage.is_some() || worker_response.trace.token_count > 0
        {
            Some(tokens_generated as u64)
        } else {
            None
        };
        let tokens_generated_meta = if token_usage.is_some() || worker_response.trace.token_count > 0
        {
            Some(tokens_generated)
        } else {
            None
        };
        let run_receipt = worker_response.run_receipt.clone();
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
                tokens_generated_meta,
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

        // Evidence-bound pinned degradation (counts + digest, no raw IDs) is computed on the worker.
        let pinned_degradation_evidence = worker_response.pinned_degradation_evidence.clone();

        // Log and persist evidence only when there is actual degradation (some pins unavailable).
        if let Some(ref ev) = pinned_degradation_evidence {
            let fallback = ev
                .pinned_fallback_mode
                .as_deref()
                .or(pinned_routing_fallback.as_deref())
                .unwrap_or("none");

            // Persist as a signed, chain-linked evidence envelope (Telemetry scope).
            if let Err(e) = persist_pinned_degradation_evidence_envelope(
                self.state,
                &request.cpid,
                &request.request_id,
                ev.clone(),
            )
            .await
            {
                warn!(
                    request_id = %request.request_id,
                    cpid = %request.cpid,
                    error = %e,
                    "Failed to persist pinned degradation evidence envelope"
                );
            }

            // Observability warning without raw adapter IDs.
            warn!(
                request_id = %request.request_id,
                cpid = %request.cpid,
                pinned_total_count = ev.pinned_total_count,
                unavailable_pinned_count = ev.unavailable_pinned_count,
                unavailable_pinned_set_digest_b3 = ?ev.unavailable_pinned_set_digest_b3,
                fallback = %fallback,
                "Pinned adapters unavailable - using fallback routing"
            );

            // Emit structured telemetry event for missing pinned adapters (no raw IDs).
            let identity = IdentityEnvelope::new(
                request.cpid.clone(),
                "inference_core".to_string(),
                "pinned_adapters_unavailable".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            );
            let event_result = TelemetryEventBuilder::new(
                EventType::Custom("inference.pinned_adapters_unavailable".to_string()),
                LogLevel::Warn,
                format!(
                    "{} of {} pinned adapters unavailable - fallback: {}",
                    ev.unavailable_pinned_count,
                    ev.pinned_total_count,
                    fallback
                ),
                identity,
            )
            .component("inference_core".to_string())
            .metadata(serde_json::json!({
                "request_id": request.request_id,
                "cpid": request.cpid,
                "session_id": request.session_id,
                "pinned_total_count": ev.pinned_total_count,
                "unavailable_pinned_count": ev.unavailable_pinned_count,
                "unavailable_pinned_set_digest_b3": ev.unavailable_pinned_set_digest_b3.as_ref().map(|h| h.to_hex()),
                "fallback_mode": fallback,
                "latency_ms": latency_ms,
            }))
            .build();

            // Push to telemetry buffer (fire-and-forget, don't block inference).
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
            server_handler_latency_us: Some(pre_uds_latency_us),
        };

        if let Ok(event) = build_inference_metrics_event(identity.clone(), metrics_payload) {
            let telemetry_buffer = self.state.telemetry_buffer.clone();
            tokio::spawn(async move {
                let _ = telemetry_buffer.push(event).await;
            });
        }

        if let Some(router_events) = worker_response.trace.router_decisions.clone() {
            let mapped_decisions =
                map_router_decisions(&router_events, request.policy_mask_digest_b3);
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
                tokens_generated: tokens_generated_event,
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
            tokens_generated,
            run_receipt,
            token_usage,
            finish_reason: worker_response.status,
            adapters_used: worker_response.trace.router_summary.adapters_used,
            router_decisions,
            router_decision_chain,
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
            // AARA Lifecycle: Abstention
            abstention: abstention_info,
            // RAG evidence IDs for message binding
            pending_evidence_ids,
        })
        }
        .await;

        // Emit diagnostic run completion
        if let Some(ref ctx) = diag_ctx {
            match &result {
                Ok(_) => ctx.emit_run_finished(),
                Err(err) => {
                    let error_code = extract_error_code(err);
                    let recovery = suggest_recovery(err);
                    ctx.emit_run_failed(error_code, recovery);
                }
            }
        }

        // Update inference state tracker with completion status
        if let Some(ref tracker) = self.state.inference_state_tracker {
            match &result {
                Ok(res) => {
                    let token_count = res.token_usage.as_ref().map(|u| u.completion_tokens);
                    tracker.mark_complete(&request.request_id, token_count);
                }
                Err(err) => {
                    let error_code = extract_error_code(err).to_string();
                    tracker.mark_failed(&request.request_id, error_code);
                }
            }
        }

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

        // Record Prometheus metrics for inference requests
        let duration_secs = start_time.elapsed().as_secs_f64();
        let tenant_id = &request.cpid;
        let model_id = request.model.as_deref().unwrap_or("unknown");

        match &result {
            Ok(res) => {
                self.state.metrics_exporter.record_inference_request(
                    tenant_id,
                    model_id,
                    "success",
                    duration_secs,
                    res.tokens_generated as u64,
                );

                // Record routing decision metrics from first decision
                if let Some(decision) = res.router_decisions.first() {
                    let gate_max = decision
                        .candidates
                        .iter()
                        .map(|c| c.raw_score)
                        .fold(0.0_f32, f32::max) as f64;

                    self.state.metrics_exporter.record_routing_decision(
                        tenant_id,
                        decision.candidates.len(),
                        decision.entropy,
                        decision.selected_adapters.len(),
                        gate_max,
                    );
                }

                // Record receipt generation
                if res.run_receipt.is_some() {
                    self.state
                        .metrics_exporter
                        .record_receipt_generated(tenant_id, "inference");
                }
            }
            Err(_) => {
                self.state.metrics_exporter.record_inference_request(
                    tenant_id,
                    model_id,
                    "error",
                    duration_secs,
                    0,
                );
            }
        }

        result
    }

    pub(crate) fn resolve_token_usage(
        worker_response: &crate::types::WorkerInferResponse,
    ) -> Option<TokenUsage> {
        worker_response
            .run_receipt
            .as_ref()
            .map(|receipt| TokenUsage {
                prompt_tokens: receipt.logical_prompt_tokens,
                completion_tokens: receipt.logical_output_tokens,
                billed_input_tokens: receipt.billed_input_tokens,
                billed_output_tokens: receipt.billed_output_tokens,
            })
            .or_else(|| worker_response.token_usage.clone())
    }

    /// Execute inference through the unified pipeline with token streaming.
    pub async fn route_and_infer_stream(
        &self,
        request: InferenceRequestInternal,
        replay_context: Option<ReplayContext>,
        cancellation_token: Option<CancellationToken>,
        stream_tx: mpsc::Sender<WorkerStreamToken>,
        pause_tx: Option<mpsc::Sender<WorkerStreamPaused>>,
    ) -> Result<InferenceResult, InferenceError> {
        self.route_and_infer(
            request,
            replay_context,
            cancellation_token,
            Some(stream_tx),
            pause_tx,
        )
        .await
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
        self.route_and_infer(
            request,
            Some(replay_context),
            cancellation_token,
            None,
            None,
        )
        .await
    }

    /// Validate that all specified adapters are loadable (not archived/purged)
    ///
    /// This is a defense-in-depth check to prevent inference on archived adapters.
    /// Returns Ok(()) if all adapters are loadable, or an error if any are archived/purged.
    pub(crate) async fn validate_adapters_loadable(
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

    async fn resolve_correlation_ids_for_adapters(
        &self,
        tenant_id: &str,
        adapter_ids: &[String],
    ) -> Vec<String> {
        let mut correlation_ids = Vec::new();
        for adapter_id in adapter_ids {
            match self
                .state
                .db
                .get_training_job_by_adapter(adapter_id, tenant_id)
                .await
            {
                Ok(Some(job)) => {
                    if let Some(corr) = job.correlation_id {
                        correlation_ids.push(corr);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        error = %e,
                        "Failed to resolve correlation_id for adapter"
                    );
                }
            }
        }
        correlation_ids.sort();
        correlation_ids.dedup();
        correlation_ids
    }

    async fn resolve_stable_ids_for_adapters(
        &self,
        tenant_id: &str,
        adapter_ids: &[String],
    ) -> Result<std::collections::HashMap<String, u64>, InferenceError> {
        let mut stable_ids =
            std::collections::HashMap::with_capacity(adapter_ids.len().saturating_mul(2));
        let mut seen = std::collections::HashSet::with_capacity(adapter_ids.len());

        for requested in adapter_ids {
            // Avoid duplicate DB lookups when effective_adapter_ids contains repeats.
            if !seen.insert(requested.as_str()) {
                continue;
            }

            let adapter = self
                .state
                .db
                .get_adapter_for_tenant(tenant_id, requested)
                .await
                .map_err(|e| {
                    InferenceError::DatabaseError(format!(
                        "Failed to resolve stable_id for adapter '{}': {}",
                        requested, e
                    ))
                })?
                .ok_or_else(|| {
                    InferenceError::AdapterNotFound(format!(
                        "Adapter '{}' not found for tenant {}",
                        requested, tenant_id
                    ))
                })?;

            let stable_id = adapter
                .stable_id
                .and_then(|v| (v > 0).then_some(v as u64))
                .unwrap_or(0);

            // Insert for the requested key and all canonical forms so the worker can
            // look up by either internal UUID (`id`) or external adapter ID (`adapter_id`).
            stable_ids.insert(requested.clone(), stable_id);
            stable_ids.insert(adapter.id.clone(), stable_id);
            if let Some(adapter_id) = adapter.adapter_id.clone() {
                stable_ids.insert(adapter_id, stable_id);
            }
        }

        Ok(stable_ids)
    }

    async fn resolve_stable_ids_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<std::collections::HashMap<String, u64>, InferenceError> {
        let adapters = self
            .state
            .db
            .list_adapters_for_tenant_unthrottled(tenant_id)
            .await
            .map_err(|e| {
                InferenceError::DatabaseError(format!(
                    "Failed to list adapters for tenant '{}' stable_id resolution: {}",
                    tenant_id, e
                ))
            })?;

        let mut stable_ids =
            std::collections::HashMap::with_capacity(adapters.len().saturating_mul(2));
        for adapter in adapters {
            let stable_id = adapter
                .stable_id
                .and_then(|v| (v > 0).then_some(v as u64))
                .unwrap_or(0);
            stable_ids.insert(adapter.id.clone(), stable_id);
            if let Some(adapter_id) = adapter.adapter_id.clone() {
                stable_ids.insert(adapter_id, stable_id);
            }
        }

        Ok(stable_ids)
    }

    /// Validate pinned adapters belong to the requesting tenant.
    ///
    pub(crate) async fn validate_pinned_adapters_for_tenant(
        &self,
        tenant_id: &str,
        pins: &[String],
    ) -> Result<(), InferenceError> {
        for pin in pins {
            let adapter = self
                .state
                .db
                .get_adapter_for_tenant(tenant_id, pin)
                .await
                .map_err(|e| {
                    InferenceError::AdapterNotFound(format!(
                        "Failed to load pinned adapter '{}': {}",
                        pin, e
                    ))
                })?;
            if adapter.is_none() {
                let adapter = self.state.db.get_adapter(pin).await.map_err(|e| {
                    InferenceError::DatabaseError(format!(
                        "Failed to validate pinned adapter '{}': {}",
                        pin, e
                    ))
                })?;
                return match adapter {
                    Some(adapter) if adapter.tenant_id != tenant_id => {
                        Err(InferenceError::AdapterTenantMismatch {
                            adapter_id: pin.clone(),
                            tenant_id: tenant_id.to_string(),
                            adapter_tenant_id: adapter.tenant_id,
                        })
                    }
                    _ => Err(InferenceError::AdapterNotFound(format!(
                        "Pinned adapter '{}' not found",
                        pin
                    ))),
                };
            }
        }

        Ok(())
    }

    /// Build allowlist of adapter IDs for a tenant for membership checks
    ///
    /// Includes both internal UUIDs (`id`) and human-readable adapter IDs (`adapter_id`)
    /// to support lookups by either identifier.
    pub(crate) async fn adapter_allowlist_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<HashSet<String>, InferenceError> {
        let adapters = self
            .state
            .db
            .list_adapters_for_tenant_unthrottled(tenant_id)
            .await
            .map_err(|e| {
                InferenceError::WorkerError(format!(
                    "Failed to list adapters for tenant {}: {}",
                    tenant_id, e
                ))
            })?;

        // Include both internal UUID and human-readable adapter_id in allowlist
        let mut allowlist = HashSet::new();
        for adapter in adapters {
            allowlist.insert(adapter.id);
            if let Some(adapter_id) = adapter.adapter_id {
                allowlist.insert(adapter_id);
            }
        }
        Ok(allowlist)
    }

    /// Ensure every adapter in `ids` is permitted for the tenant.
    ///
    /// Validates adapter IDs against tenant allowlist.
    pub(crate) async fn validate_ids_against_allowlist(
        &self,
        ids: &[String],
        tenant_id: &str,
        allowlist: &HashSet<String>,
        context: &str,
    ) -> Result<(), InferenceError> {
        for id in ids {
            if !allowlist.contains(id) {
                let adapter = self.state.db.get_adapter(id).await.map_err(|e| {
                    InferenceError::DatabaseError(format!(
                        "Failed to validate {} '{}': {}",
                        context, id, e
                    ))
                })?;
                return match adapter {
                    Some(adapter) if adapter.tenant_id != tenant_id => {
                        Err(InferenceError::AdapterTenantMismatch {
                            adapter_id: id.clone(),
                            tenant_id: tenant_id.to_string(),
                            adapter_tenant_id: adapter.tenant_id,
                        })
                    }
                    Some(_) => Err(InferenceError::AdapterNotFound(format!(
                        "{} '{}' not found for tenant {}",
                        context, id, tenant_id
                    ))),
                    None => Err(InferenceError::AdapterNotFound(format!(
                        "{} '{}' not found",
                        context, id
                    ))),
                };
            }
        }
        Ok(())
    }

    fn collect_adapter_ids_for_request(&self, request: &InferenceRequestInternal) -> Vec<String> {
        if let Some(effective) = request.effective_adapter_ids.as_ref() {
            return effective.clone();
        }
        if let Some(adapters) = request.adapters.as_ref() {
            return adapters.clone();
        }
        if let Some(stack_list) = request.adapter_stack.as_ref() {
            return stack_list.clone();
        }
        Vec::new()
    }

    pub(crate) async fn validate_adapter_base_models(
        &self,
        tenant_id: &str,
        adapter_ids: &[String],
        expected_base_model_id: &str,
    ) -> Result<(), InferenceError> {
        for adapter_id in adapter_ids {
            let adapter = self
                .state
                .db
                .get_adapter_for_tenant(tenant_id, adapter_id)
                .await
                .map_err(|e| {
                    InferenceError::DatabaseError(format!(
                        "Failed to load adapter '{}': {}",
                        adapter_id, e
                    ))
                })?
                .ok_or_else(|| {
                    InferenceError::AdapterNotFound(format!(
                        "Adapter '{}' not found for tenant {}",
                        adapter_id, tenant_id
                    ))
                })?;

            if let Some(ref adapter_base_model_id) = adapter.base_model_id {
                if adapter_base_model_id != expected_base_model_id {
                    return Err(InferenceError::AdapterBaseModelMismatch {
                        adapter_id: adapter
                            .adapter_id
                            .clone()
                            .unwrap_or_else(|| adapter.id.clone()),
                        expected_base_model_id: expected_base_model_id.to_string(),
                        adapter_base_model_id: Some(adapter_base_model_id.clone()),
                    });
                }
            }
        }

        Ok(())
    }

    /// Validate a list of adapters are loadable and not archived/purged.
    pub(crate) async fn validate_adapter_ids_loadable(
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
    pub(crate) async fn resolve_effective_adapters(
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
                request.stack_version = Some(stack.version_number());
                request.stack_determinism_mode = stack.determinism_mode.clone();
                request.stack_routing_determinism_mode =
                    parse_routing_mode(&stack.routing_determinism_mode);
                request.effective_adapter_ids = Some(Vec::new());
                return Ok(());
            }

            request.stack_id = Some(stack_id);
            request.stack_version = Some(stack.version_number());
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
                    request.stack_version = Some(stack.version_number());
                    request.stack_determinism_mode = stack.determinism_mode.clone();
                    request.stack_routing_determinism_mode =
                        parse_routing_mode(&stack.routing_determinism_mode);
                    request.effective_adapter_ids = Some(Vec::new());
                    return Ok(());
                }

                request.stack_id = Some(default_stack_id.clone());
                request.stack_version = Some(stack.version_number());
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
    /// Uses the unified WorkerSelector for single-pass selection that combines:
    /// - Database query for compatible workers
    /// - Capability filtering with pre-indexed capabilities
    /// - Health-aware selection integrated into scoring
    ///
    /// # Retry Behavior
    ///
    /// - 3 attempts with 2s, 4s delays between retries
    /// - Logs attempt number, delay, and remaining budget on each retry
    /// - In dev mode: returns WorkerDegraded after retries (allows graceful degradation)
    /// - In prod/staging mode: returns NoCompatibleWorker (hard failure)
    pub(crate) async fn select_worker_for_request(
        &self,
        request: &InferenceRequestInternal,
    ) -> Result<WorkerWithBinding, InferenceError> {
        let required_manifest = self.state.manifest_hash.as_deref().ok_or_else(|| {
            InferenceError::NoCompatibleWorker {
                required_hash: "unset".to_string(),
                tenant_id: request.cpid.clone(),
                available_count: 0,
                reason: "Manifest hash not configured on control plane".to_string(),
                details: None,
            }
        })?;

        // Build requirements from the request
        let requirements = WorkerRequirements::from_request(request, required_manifest);

        // Use the unified selector with retry
        self.select_worker_with_unified_selector(requirements).await
    }

    pub(crate) async fn select_worker_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<WorkerWithBinding, InferenceError> {
        let required_manifest = self.state.manifest_hash.as_deref().ok_or_else(|| {
            InferenceError::NoCompatibleWorker {
                required_hash: "unset".to_string(),
                tenant_id: tenant_id.to_string(),
                available_count: 0,
                reason: "Manifest hash not configured on control plane".to_string(),
                details: None,
            }
        })?;

        // Build minimal requirements for tenant
        let requirements = WorkerRequirements::for_tenant(tenant_id, required_manifest);

        // Use the unified selector with retry
        self.select_worker_with_unified_selector(requirements).await
    }

    /// Internal method that uses the unified WorkerSelector with retry logic.
    ///
    /// This replaces the previous multi-step flow with a single-pass selection:
    /// 1. Creates a WorkerSelector with DB and health monitor
    /// 2. Uses select_with_retry for automatic retry handling
    /// 3. Converts SelectionError to InferenceError with dev mode handling
    async fn select_worker_with_unified_selector(
        &self,
        requirements: WorkerRequirements,
    ) -> Result<WorkerWithBinding, InferenceError> {
        use std::time::Duration;

        const MAX_ATTEMPTS: u32 = 3;
        const BASE_DELAY: Duration = Duration::from_secs(2);
        const MAX_ELAPSED: Duration = Duration::from_secs(30);

        // Create unified selector (borrows db from state)
        let selector = WorkerSelector::new(self.state.db.raw(), self.state.health_monitor.clone());

        // Use selector with retry
        match selector
            .select_with_retry(&requirements, MAX_ATTEMPTS, BASE_DELAY, MAX_ELAPSED)
            .await
        {
            Ok(worker) => Ok(worker),
            Err(e) => {
                // Convert SelectionError to InferenceError with dev mode handling
                let is_dev_mode = self.state.runtime_mode.map(|m| m.is_dev()).unwrap_or(false);

                match e {
                    crate::worker_selector::SelectionError::DatabaseError(msg) => {
                        Err(InferenceError::WorkerError(format!(
                            "Failed to list compatible workers: {}",
                            msg
                        )))
                    }
                    crate::worker_selector::SelectionError::NoCompatibleWorker {
                        tenant_id,
                        manifest_hash,
                        candidates_considered,
                        candidates_after_filter,
                        exclusions,
                    } => {
                        // Diagnose the specific reason
                        let (reason, available_count) = if candidates_considered == 0 {
                            let (count, reason) = self
                                .diagnose_worker_unavailability(&manifest_hash, &tenant_id)
                                .await;
                            (reason, count)
                        } else if !exclusions.is_empty() {
                            (
                                "Workers excluded by capability requirements".to_string(),
                                candidates_considered,
                            )
                        } else {
                            (
                                format!(
                                    "No workers passed filtering ({} considered, {} after filter)",
                                    candidates_considered, candidates_after_filter
                                ),
                                candidates_considered,
                            )
                        };

                        if is_dev_mode {
                            warn!(
                                tenant_id = %tenant_id,
                                reason = %reason,
                                "Worker discovery failed in dev mode - system degraded"
                            );
                            Err(InferenceError::WorkerDegraded {
                                tenant_id,
                                reason: format!("No compatible worker (dev mode): {}", reason),
                            })
                        } else {
                            error!(
                                tenant_id = %tenant_id,
                                reason = %reason,
                                "Worker discovery failed after all retries"
                            );
                            let exclusion_details = if !exclusions.is_empty() {
                                Some(serde_json::json!({
                                    "excluded_workers": exclusions,
                                }))
                            } else {
                                None
                            };

                            Err(InferenceError::NoCompatibleWorker {
                                required_hash: manifest_hash,
                                tenant_id,
                                available_count,
                                reason,
                                details: exclusion_details,
                            })
                        }
                    }
                }
            }
        }
    }

    /// Legacy method for backward compatibility.
    ///
    /// This method is kept for callers that need the old API. Internally it delegates
    /// to the unified selector.
    #[allow(dead_code)]
    async fn select_worker_for_tenant_with_requirements(
        &self,
        tenant_id: &str,
        required_modes: Option<RequiredModes>,
        require_determinism: bool,
        require_backend: Option<BackendKind>,
    ) -> Result<WorkerWithBinding, InferenceError> {
        let required_manifest = self.state.manifest_hash.as_deref().ok_or_else(|| {
            InferenceError::NoCompatibleWorker {
                required_hash: "unset".to_string(),
                tenant_id: tenant_id.to_string(),
                available_count: 0,
                reason: "Manifest hash not configured on control plane".to_string(),
                details: None,
            }
        })?;

        // Build requirements from the legacy parameters
        let capabilities = if let Some(modes) = required_modes {
            RequiredCapabilities {
                modes,
                backend: require_backend,
                require_determinism,
            }
        } else {
            RequiredCapabilities::any()
        };

        let requirements = WorkerRequirements {
            tenant_id: tenant_id.to_string(),
            manifest_hash: required_manifest.to_string(),
            capabilities,
            prefer_cache_hit: false,
        };

        self.select_worker_with_unified_selector(requirements).await
    }

    /// Legacy filter method for backward compatibility.
    ///
    /// This is kept for any callers that might use it directly.
    /// The unified WorkerSelector now handles this inline.
    #[allow(dead_code)]
    fn filter_workers_by_capabilities(
        &self,
        workers: Vec<WorkerWithBinding>,
        required_modes: &RequiredModes,
        require_backend: Option<BackendKind>,
        require_determinism: bool,
    ) -> (Vec<WorkerWithBinding>, Vec<WorkerCapabilityExclusion>) {
        let mut compatible = Vec::new();
        let mut exclusions = Vec::new();

        for worker in workers {
            let caps = parse_worker_capabilities(
                worker.capabilities_json.as_deref(),
                worker.backend.as_deref(),
                &[],
            );
            let reasons = capability_reasons(
                caps.as_ref(),
                required_modes,
                require_backend,
                require_determinism,
            );

            if reasons.is_empty() {
                compatible.push(worker);
            } else {
                let worker_id = worker.id.clone();
                info!(
                    worker_id = %worker_id,
                    tenant_id = %worker.tenant_id,
                    backend = %worker.backend.as_deref().unwrap_or("unknown"),
                    reasons = ?reasons,
                    "Worker excluded by capability requirements"
                );
                exclusions.push(WorkerCapabilityExclusion {
                    worker_id,
                    backend: worker.backend.clone(),
                    reasons,
                    capabilities: caps,
                });
            }
        }

        (compatible, exclusions)
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
    #[allow(dead_code)]
    pub(crate) async fn resolve_worker_path(
        &self,
        tenant_id: &str,
    ) -> Result<PathBuf, InferenceError> {
        let worker = self.select_worker_for_tenant(tenant_id).await?;
        Ok(PathBuf::from(&worker.uds_path))
    }

    /// Resolve worker UDS path for replay with manifest/backend constraints
    ///
    /// Unlike `resolve_worker_path()`, this method enforces strict compatibility:
    /// - Current loaded manifest_hash must match required
    /// - Current backend must match required
    ///
    /// If the current AppState matches the requirements, we resolve a worker
    /// using the request's capability requirements. If not, we return
    /// `InferenceError::NoCompatibleWorker`.
    ///
    /// # Design Note
    ///
    /// We check against AppState rather than querying worker metadata because:
    /// 1. AppState reflects what's actually loaded right now
    /// 2. Worker DB records may not have manifest_hash populated
    /// 3. If AppState matches, all workers in this process can serve the replay
    async fn resolve_worker_path_for_replay(
        &self,
        request: &InferenceRequestInternal,
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
                tenant_id: request.cpid.clone(),
                available_count: 0,
                reason,
                details: None,
            });
        }

        debug!(
            tenant_id = %request.cpid,
            manifest_hash = %required_manifest_hash,
            backend = %required_backend,
            "Current system state matches replay requirements, using standard worker resolution"
        );

        // Current state matches - use standard worker resolution with request requirements
        let worker = self.select_worker_for_request(request).await?;
        Ok(PathBuf::from(&worker.uds_path))
    }

    /// Retrieve RAG context and augment the prompt
    ///
    /// Uses the shared rag_common module for deterministic retrieval.
    ///
    /// Returns: (augmented_prompt, rag_evidence, pending_evidence_ids)
    /// The pending_evidence_ids are evidence records stored without a message_id.
    /// After message creation, call `db.bind_evidence_to_message()` with these IDs.
    async fn retrieve_and_augment_rag(
        &self,
        request: &InferenceRequestInternal,
    ) -> Result<(String, Option<RagEvidence>, Vec<String>), InferenceError> {
        let collection_id = match &request.rag_collection_id {
            Some(id) => id,
            None => {
                // RAG enabled but no collection - just return original prompt
                debug!(
                    request_id = %request.request_id,
                    "RAG enabled but no collection_id provided, skipping RAG retrieval"
                );
                return Ok((request.prompt.clone(), None, Vec::new()));
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
                return Ok((request.prompt.clone(), None, Vec::new()));
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
                    Ok((request.prompt.clone(), None, Vec::new()))
                } else {
                    // Capture model context at inference time for evidence audit trail
                    // This ensures evidence remains accurate even if workspace state changes later
                    let model_context = match self
                        .state
                        .db
                        .get_workspace_active_state(&request.cpid)
                        .await
                    {
                        Ok(Some(ws)) => Some(EvidenceModelContext {
                            base_model_id: ws.active_base_model_id,
                            adapter_ids: ws.active_adapter_ids.and_then(|s| {
                                match serde_json::from_str::<Vec<String>>(&s) {
                                    Ok(ids) => Some(ids),
                                    Err(e) => {
                                        tracing::warn!(
                                            tenant_id = %request.cpid,
                                            request_id = %request.request_id,
                                            error = %e,
                                            raw_value = %s,
                                            "Failed to parse active_adapter_ids from workspace state"
                                        );
                                        None
                                    }
                                }
                            }),
                            manifest_hash: ws.manifest_hash_b3,
                        }),
                        _ => None,
                    };

                    // Store evidence (best effort, Phase 1 of two-phase binding).
                    // NOTE: message_id is None because the message is created after inference.
                    // After message creation, call db.bind_evidence_to_message()
                    // with the returned evidence_ids to complete the audit trail.
                    let evidence_ids = store_rag_evidence(
                        self.state,
                        &rag_result,
                        &request.request_id,
                        request.session_id.as_deref(),
                        None, // Phase 2: bind_evidence_to_message(evidence_ids, message_id)
                        model_context.as_ref(),
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

                    Ok((augmented, Some(evidence), evidence_ids))
                }
            }
            Err(e) => {
                error!(
                    request_id = %request.request_id,
                    error = %e,
                    "RAG context retrieval failed, proceeding without RAG"
                );
                // Don't fail the whole request, just proceed without RAG
                Ok((request.prompt.clone(), None, Vec::new()))
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
        let golden_runs_dir = adapteros_core::rebase_var_path("var/golden_runs/baselines");
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

        let stop_policy_json =
            request
                .stop_policy
                .as_ref()
                .and_then(|sp| match serde_json::to_string(sp) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        tracing::warn!(
                            request_id = %request.request_id,
                            error = %e,
                            "Failed to serialize stop_policy for replay metadata"
                        );
                        None
                    }
                });

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
            policy_mask_digest_b3: request.policy_mask_digest_b3.as_ref().map(hex::encode),
            utf8_healing: request.utf8_healing,
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
    /// The router uses a deterministic algorithm (sorted by score, then by stable_id
    /// for tie-breaking). The `router_seed` is stored for audit purposes but
    /// does not currently affect routing decisions. This means replays will
    /// produce identical routing given identical inputs and model state.
    #[allow(clippy::too_many_arguments)]
    async fn capture_replay_metadata(
        &self,
        request: &InferenceRequestInternal,
        prompt_text: &str,
        response_text: &str,
        tokens_generated: Option<usize>,
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

        let tokens_generated = tokens_generated.map(|value| value as i32);

        // Serialize stop_policy if present
        let stop_policy_json =
            request
                .stop_policy
                .as_ref()
                .and_then(|sp| match serde_json::to_string(sp) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        tracing::warn!(
                            request_id = %request.request_id,
                            error = %e,
                            "Failed to serialize stop_policy for replay metadata"
                        );
                        None
                    }
                });

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
            policy_mask_digest_b3: request.policy_mask_digest_b3.as_ref().map(hex::encode),
            utf8_healing: request.utf8_healing,
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

async fn persist_pinned_degradation_evidence_envelope(
    state: &AppState,
    tenant_id: &str,
    inference_id: &str,
    evidence: PinnedDegradationEvidence,
) -> adapteros_core::Result<()> {
    let (bundle_hash, merkle_root) = pinned_degradation_telemetry_ref_ids(tenant_id, inference_id);
    let root = B3Hash::hash_multi(&[bundle_hash.as_bytes(), merkle_root.as_bytes()]);

    // Idempotency: if this evidence envelope is already present, don't insert again.
    if state
        .db
        .get_evidence_envelope_by_root(tenant_id, EvidenceScope::Telemetry, &root)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let bundle_ref = BundleMetadataRef {
        bundle_hash,
        merkle_root,
        event_count: 1,
        cpid: Some(tenant_id.to_string()),
        sequence_no: None,
        pinned_degradation_evidence: Some(evidence),
    };

    // Retry once if the telemetry evidence chain tail moves concurrently.
    for attempt in 0..=1 {
        let previous_root = state
            .db
            .get_evidence_chain_tail(tenant_id, EvidenceScope::Telemetry)
            .await?
            .map(|(root, _seq)| root);

        let mut envelope = EvidenceEnvelope::new_telemetry(
            tenant_id.to_string(),
            bundle_ref.clone(),
            previous_root,
        );

        // Sign the envelope under the existing server signing key.
        // NOTE: signed_at_us is part of canonical bytes; set it before signing.
        envelope.signed_at_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        let canonical_bytes = envelope.to_canonical_bytes();
        let signature = state.crypto.signing_keypair.sign(&canonical_bytes);
        let pubkey_bytes = state.crypto.signing_keypair.public_key().to_bytes();
        envelope.signature = hex::encode(signature.to_bytes());
        envelope.public_key = hex::encode(pubkey_bytes);
        envelope.key_id = compute_key_id(&pubkey_bytes);

        match state.db.store_evidence_envelope(&envelope).await {
            Ok(_id) => return Ok(()),
            Err(e) if attempt == 0 && adapteros_core::is_evidence_chain_divergence(&e) => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
