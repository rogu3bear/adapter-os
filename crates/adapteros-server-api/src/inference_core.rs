//! Unified inference execution core (PRD-05)
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
//! # Deterministic Replay (PRD-02)
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
    ChunkReference, InferenceError, InferenceRequestInternal, InferenceResult, RagEvidence,
    ReplayContext, RouterDecisionRecord, SamplingParams, WorkerInferRequest, MAX_REPLAY_TEXT_SIZE,
    SAMPLING_ALGORITHM_VERSION,
};
use crate::uds_client::UdsClient;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_db::CreateReplayMetadataParams;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, warn};

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
    /// * `replay_context` - Optional replay context for deterministic replay (PRD-02)
    ///
    /// # Returns
    /// * `Ok(InferenceResult)` - Successful inference result
    /// * `Err(InferenceError)` - Error during inference
    pub async fn route_and_infer(
        &self,
        request: InferenceRequestInternal,
        replay_context: Option<ReplayContext>,
    ) -> Result<InferenceResult, InferenceError> {
        let start_time = std::time::Instant::now();
        let is_replay = replay_context.is_some();

        if let Some(ref ctx) = replay_context {
            info!(
                request_id = %request.request_id,
                original_inference_id = %ctx.original_inference_id,
                required_manifest = %ctx.required_manifest_hash,
                required_backend = %ctx.required_backend,
                "Starting replay inference"
            );
        }

        // 0. Validate adapters are loadable (not archived/purged)
        // This is a defense-in-depth check - handlers should also validate
        self.validate_adapters_loadable(&request).await?;

        // Set the routing guard - this allows the UDS client to proceed
        crate::uds_client::enter_routed_context();

        // Use scopeguard to ensure we always exit the routed context
        let _guard = scopeguard::guard((), |_| {
            crate::uds_client::exit_routed_context();
        });

        // 1. Resolve worker UDS path
        // For replay: enforce manifest/backend constraints (PRD-02)
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

        // 2.5. Resolve pinned adapters from session (CHAT-PIN-02)
        // ┌─────────────────────────────────────────────────────────────────┐
        // │ Pinned Adapter Pipeline: Session-level preferences              │
        // │ - Pinned adapters receive PINNED_BOOST to their priors          │
        // │ - Non-pinned adapters can still be selected with high scores    │
        // │ - Unavailable pinned adapters are tracked for UI warning        │
        // └─────────────────────────────────────────────────────────────────┘
        let (pinned_adapter_ids, unavailable_pinned_adapters) = if let Some(ref session_id) =
            request.session_id
        {
            // Use request.pinned_adapter_ids if already set, otherwise fetch from session
            let pinned_ids = if request.pinned_adapter_ids.is_some() {
                request.pinned_adapter_ids.clone()
            } else {
                // Fetch session to get pinned adapter IDs
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

            // Compute unavailable pinned adapters
            // Note: We use adapters_used from the response to check availability.
            // For now, we track pinned IDs and compute unavailable after inference.
            // This is a simplification - full availability check would require
            // querying the manifest before inference.
            (pinned_ids, None::<Vec<String>>)
        } else {
            (None, None)
        };

        // 3. Create worker request with full sampling parameters (PRD-02: determinism)
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
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            seed: request.seed,
            router_seed: request.router_seed.clone(),
            pinned_adapter_ids: pinned_adapter_ids.clone(),
        };

        // 4. Call worker via UDS
        // Longer timeout for replay to account for cold worker startup
        let timeout_secs = if is_replay { 120 } else { 60 };
        let uds_client = UdsClient::new(Duration::from_secs(timeout_secs));
        let worker_response =
            uds_client
                .infer(&uds_path, worker_request)
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

        // 6. Update session activity if session_id provided
        if let Some(session_id) = &request.session_id {
            self.update_session_activity(session_id, &worker_response)
                .await;
        }

        // 7. Log inference completion
        let latency_ms = start_time.elapsed().as_millis() as u64;
        let response_text = worker_response.text.clone().unwrap_or_default();

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

        // 8. Capture replay metadata (PRD-02: deterministic replay)
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
                &rag_evidence,
                latency_ms,
            )
            .await;
        } else {
            debug!(
                request_id = %request.request_id,
                "Skipping replay metadata capture (skip_metadata_capture=true)"
            );
        }

        // 9. Get unavailable pinned adapters from worker response (CHAT-PIN-02, PRD-6A)
        // The worker computes this by checking pinned IDs against loaded adapters
        let unavailable_pinned = worker_response
            .unavailable_pinned_adapters
            .clone()
            .or(unavailable_pinned_adapters);

        // Compute pinned_routing_fallback based on unavailability (PRD-6A)
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

        // Log warning (not debug) when pinned adapters are missing for observability (PRD-6A)
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

            // Emit structured telemetry event for missing pinned adapters (PRD-6A)
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
            rag_evidence,
            latency_ms,
            request_id: request.request_id,
            unavailable_pinned_adapters: unavailable_pinned,
            pinned_routing_fallback,
        })
    }

    /// Execute inference replay through the unified pipeline (PRD-02)
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
        // Collect all adapter IDs from the request
        let adapter_ids: Vec<&str> = request
            .adapters
            .as_ref()
            .map(|a| a.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        // Also check adapter_stack if present
        let stack_ids: Vec<&str> = request
            .adapter_stack
            .as_ref()
            .map(|a| a.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        // Check each adapter
        for adapter_id in adapter_ids.into_iter().chain(stack_ids.into_iter()) {
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
                    // Log but don't fail - adapter might not exist yet or DB error
                    // The worker will handle the actual adapter resolution
                    debug!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "Could not verify adapter loadability (may be resolved by worker)"
                    );
                }
            }
        }

        Ok(())
    }

    /// Resolve the worker UDS path from database or environment (PRD-01)
    ///
    /// Uses manifest-based routing to select a compatible worker:
    /// 1. If AppState has a manifest_hash, filters workers by that hash + tenant
    /// 2. Otherwise, gets all serving workers and filters by tenant
    /// 3. Falls back to env override or default socket for dev mode
    ///
    /// This ensures workers only serve requests they're compatible with.
    async fn resolve_worker_path(&self, tenant_id: &str) -> Result<PathBuf, InferenceError> {
        // Get the current manifest hash from AppState (if available)
        let required_manifest = self.state.manifest_hash.as_deref();

        // Get workers - filter by manifest if we have one (PRD-01: manifest binding)
        let workers = if let Some(manifest_hash) = required_manifest {
            // Use manifest-aware worker selection
            self.state
                .db
                .list_compatible_workers_for_tenant(manifest_hash, tenant_id)
                .await
                .map_err(|e| {
                    InferenceError::WorkerError(format!("Failed to list compatible workers: {}", e))
                })?
        } else {
            // No manifest specified - get all serving workers and filter by tenant
            let all_workers = self.state.db.list_serving_workers().await.map_err(|e| {
                InferenceError::WorkerError(format!("Failed to list workers: {}", e))
            })?;

            // Filter by tenant (PRD-01: tenant isolation)
            all_workers
                .into_iter()
                .filter(|w| w.tenant_id == tenant_id)
                .collect()
        };

        // Select the best compatible worker (already sorted by latency)
        if let Some(worker) = workers.first() {
            debug!(
                tenant_id = %tenant_id,
                worker_id = %worker.id,
                manifest_hash = %worker.manifest_hash_b3.as_deref().unwrap_or("none"),
                required_manifest = %required_manifest.unwrap_or("any"),
                "Selected compatible worker for inference"
            );
            return Ok(PathBuf::from(&worker.uds_path));
        }

        // If no compatible tenant workers, try any serving worker (dev mode fallback)
        if required_manifest.is_none() {
            let any_workers = self.state.db.list_serving_workers().await.map_err(|e| {
                InferenceError::WorkerError(format!("Failed to list workers: {}", e))
            })?;

            if let Some(worker) = any_workers.first() {
                debug!(
                    tenant_id = %tenant_id,
                    worker_id = %worker.id,
                    worker_tenant = %worker.tenant_id,
                    "No tenant-specific worker, using available serving worker (dev mode)"
                );
                return Ok(PathBuf::from(&worker.uds_path));
            }
        }

        // Fallback: honor env override or default socket (dev mode)
        if let Ok(socket_path) = std::env::var("AOS_WORKER_SOCKET") {
            debug!(
                tenant_id = %tenant_id,
                socket_path = %socket_path,
                "No serving workers found, using AOS_WORKER_SOCKET fallback"
            );
            return Ok(PathBuf::from(socket_path));
        }

        // No compatible worker available - build detailed error (PRD-01)
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
            } else if required_manifest.is_some() {
                // Workers exist but none match the required manifest
                let manifest_matched = all_serving
                    .iter()
                    .filter(|w| w.manifest_hash_b3.as_deref() == required_manifest)
                    .count();

                if manifest_matched == 0 {
                    (all_serving.len(), format!(
                        "No workers match required manifest hash. {} serving workers exist with different manifests",
                        all_serving.len()
                    ))
                } else {
                    // Manifest matched but tenant didn't
                    (
                        all_serving.len(),
                        format!(
                            "{} workers match manifest but none belong to tenant '{}'",
                            manifest_matched, tenant_id
                        ),
                    )
                }
            } else {
                // No manifest required, but still no tenant match
                let tenant_matched = all_serving
                    .iter()
                    .filter(|w| w.tenant_id == tenant_id)
                    .count();

                if tenant_matched == 0 {
                    (
                        all_serving.len(),
                        format!(
                            "{} serving workers exist but none belong to tenant '{}'",
                            all_serving.len(),
                            tenant_id
                        ),
                    )
                } else {
                    (
                        all_serving.len(),
                        "Workers filtered out by schema version incompatibility".to_string(),
                    )
                }
            }
        };

        Err(InferenceError::NoCompatibleWorker {
            required_hash: required_manifest.unwrap_or("any").to_string(),
            tenant_id: tenant_id.to_string(),
            available_count: all_count,
            reason,
        })
    }

    /// Resolve worker UDS path for replay with manifest/backend constraints (PRD-02)
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
        let chunks_used: Vec<ChunkReference> = result
            .doc_ids
            .iter()
            .zip(result.scores.iter())
            .enumerate()
            .map(|(rank, (doc_id, score))| ChunkReference {
                document_id: doc_id.clone(),
                chunk_id: String::new(), // Not available in RagContextResult
                page_number: None,       // Not available in RagContextResult
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
    /// # Current Limitation
    /// The worker protocol currently only returns `adapters_used` in `RouterSummary`.
    /// Full routing decisions (entropy, candidates, scores, latency) are computed in
    /// `adapteros-lora-worker/src/inference_pipeline.rs` during token generation but
    /// are NOT included in the worker response.
    ///
    /// To populate the `routing_decisions` table (migration 0070), the worker would need
    /// to either:
    /// 1. Include detailed routing decisions in `WorkerInferResponse`, or
    /// 2. Write routing decisions directly to DB from the worker
    ///
    /// For PRD-05, routing is still _enforced_ (happens in worker's inference_pipeline),
    /// but detailed decision records are not captured in the control plane.
    fn extract_router_decisions(
        &self,
        _response: &crate::types::WorkerInferResponse,
    ) -> Vec<RouterDecisionRecord> {
        // Worker only returns adapters_used, not full routing decision details.
        // Routing enforcement happens, but detailed records require worker protocol changes.
        vec![]
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

    /// Capture replay metadata for deterministic replay (PRD-02)
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
        rag_evidence: &Option<RagEvidence>,
        latency_ms: u64,
    ) {
        // Get manifest hash from state (current loaded manifest)
        let manifest_hash = self
            .state
            .manifest_hash
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        // Get backend from state
        let backend = self
            .state
            .backend_name
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        // Build sampling params JSON
        let sampling_params = SamplingParams {
            temperature: request.temperature,
            top_k: request.top_k,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            seed: request.seed,
        };
        let sampling_params_json = serde_json::to_string(&sampling_params).unwrap_or_default();

        // Compute RAG snapshot hash and extract doc IDs
        let (rag_snapshot_hash, rag_doc_ids) = if let Some(evidence) = rag_evidence {
            let doc_ids: Vec<String> = evidence
                .chunks_used
                .iter()
                .map(|c| c.document_id.clone())
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
        let prompt_truncated = prompt_text.len() > MAX_REPLAY_TEXT_SIZE;
        let response_truncated = response_text.len() > MAX_REPLAY_TEXT_SIZE;
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

        // Get adapter IDs from request or default to empty
        let adapter_ids = request.adapters.clone();

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
            router_seed: request.router_seed.clone(),
            sampling_params_json,
            backend,
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
        };

        // Store to database (best effort - don't fail inference on capture error)
        if let Err(e) = self.state.db.create_replay_metadata(params).await {
            warn!(
                request_id = %request.request_id,
                error = %e,
                "Failed to capture replay metadata (PRD-02)"
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

    #[test]
    fn test_replay_context_structure() {
        // Verify ReplayContext has all required fields for PRD-02
        let ctx = ReplayContext {
            original_inference_id: "test-123".to_string(),
            required_manifest_hash: "abc123".to_string(),
            required_backend: "mlx".to_string(),
            skip_metadata_capture: true,
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
        };
        assert!(!ctx.skip_metadata_capture);
    }

    #[test]
    fn test_sampling_params_serialization() {
        let params = SamplingParams {
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.9),
            max_tokens: 100,
            seed: Some(42),
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
}
