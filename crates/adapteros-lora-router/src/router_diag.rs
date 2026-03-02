//! Router diagnostics emitter for fine-grained routing decision events.
//!
//! This module provides a RouterDiagEmitter that emits deterministic diagnostic
//! events during the K-sparse routing process. All events use:
//! - Q15 quantization for scores (i32, denominator 32767.0)
//! - stable_id for adapter identification (not array indices)
//! - No timing in canonical hash bytes
//!
//! # Event Sequence
//!
//! ```text
//! RoutingStart → [GateComputed]* → KsparseSelected → [TieBreakApplied]* → RoutingEnd
//! ```
//!
//! Events are only emitted when `diag.level >= router`.

use crate::quantization::ROUTER_GATE_Q15_DENOM;
use crate::AdapterInfo;
use adapteros_core::B3Hash;
use adapteros_telemetry::diagnostics::{
    DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, SharedDiagEmitter,
};
use adapteros_telemetry::tracing::TraceContext;
use std::time::Instant;

/// Q15 denominator for score quantization (must match ROUTER_GATE_Q15_DENOM)
const SCORE_Q15_DENOM: f32 = ROUTER_GATE_Q15_DENOM;

/// Convert a floating-point score to Q15 format for deterministic storage.
///
/// Uses denominator 32767.0 and stores as i32 to preserve the sign for
/// potentially negative intermediate scores.
#[inline]
fn score_to_q15(score: f32) -> i32 {
    (score * SCORE_Q15_DENOM).round() as i32
}

/// Router diagnostics emitter for K-sparse routing decisions.
///
/// Wraps a SharedDiagEmitter and provides convenience methods for emitting
/// router-specific diagnostic events with deterministic payloads.
///
/// # Usage
///
/// ```ignore
/// let diag = RouterDiagEmitter::new(diag_emitter, trace_ctx, run_id, tenant_id);
/// diag.emit_routing_start(step_idx, candidates, k, &features);
/// // ... routing logic ...
/// diag.emit_routing_end(step_idx, selected_count, decision_hash, policy_digest);
/// ```
#[derive(Clone)]
pub struct RouterDiagEmitter {
    /// The underlying diagnostic emitter
    emitter: SharedDiagEmitter,
    /// Trace context for correlation
    trace_ctx: TraceContext,
    /// Run ID for grouping events
    run_id: DiagRunId,
    /// Tenant ID for isolation
    tenant_id: String,
    /// Monotonic start time for duration calculation
    start_time: Instant,
}

impl RouterDiagEmitter {
    /// Create a new RouterDiagEmitter.
    ///
    /// # Arguments
    ///
    /// * `emitter` - The shared diagnostic emitter
    /// * `trace_ctx` - W3C trace context for correlation
    /// * `run_id` - Run ID for grouping related events
    /// * `tenant_id` - Tenant ID for multi-tenant isolation
    pub fn new(
        emitter: SharedDiagEmitter,
        trace_ctx: TraceContext,
        run_id: DiagRunId,
        tenant_id: impl Into<String>,
    ) -> Self {
        Self {
            emitter,
            trace_ctx,
            run_id,
            tenant_id: tenant_id.into(),
            start_time: Instant::now(),
        }
    }

    /// Get monotonic timestamp in microseconds since router start.
    fn mono_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Emit a diagnostic envelope with the given event.
    fn emit(&self, severity: DiagSeverity, event: DiagEvent) {
        let envelope = DiagEnvelope::new(
            &self.trace_ctx,
            &self.tenant_id,
            self.run_id.clone(),
            severity,
            self.mono_us(),
            event,
        );
        // Non-blocking emit, ignore errors
        let _ = self.emitter.emit(envelope);
    }

    /// Emit RoutingStart event at entry to route().
    ///
    /// # Arguments
    ///
    /// * `step_idx` - Step index within the inference (0-based)
    /// * `candidate_count` - Number of candidate adapters before filtering
    /// * `k` - Maximum adapters to select
    /// * `features` - Input features (hashed for storage)
    pub fn emit_routing_start(
        &self,
        step_idx: u32,
        candidate_count: u32,
        k: u32,
        features: &[f32],
    ) {
        // Hash features for deterministic storage (avoid storing floats)
        let features_bytes: Vec<u8> = features.iter().flat_map(|f| f.to_le_bytes()).collect();
        let features_hash = B3Hash::hash(&features_bytes);

        self.emit(
            DiagSeverity::Debug,
            DiagEvent::RoutingStart {
                step_idx,
                candidate_count,
                k,
                features_hash,
            },
        );
    }

    /// Emit GateComputed event for a single adapter after scoring.
    ///
    /// # Arguments
    ///
    /// * `step_idx` - Step index within the inference
    /// * `adapter_info` - Adapter metadata (for stable_id and id)
    /// * `score` - Combined score (prior + features - penalty)
    pub fn emit_gate_computed(&self, step_idx: u32, adapter_info: &AdapterInfo, score: f32) {
        self.emit(
            DiagSeverity::Trace,
            DiagEvent::GateComputed {
                step_idx,
                stable_id: adapter_info.stable_id,
                adapter_id: adapter_info.id.clone(),
                score_q15: score_to_q15(score),
            },
        );
    }

    /// Emit KsparseSelected event after top-K selection and softmax.
    ///
    /// # Arguments
    ///
    /// * `step_idx` - Step index within the inference
    /// * `selected_adapters` - Selected adapter infos in gate-descending order
    /// * `gates_q15` - Q15 gates for selected adapters (parallel to selected_adapters)
    /// * `decision_hash` - BLAKE3 hash of the complete decision
    pub fn emit_ksparse_selected(
        &self,
        step_idx: u32,
        selected_adapters: &[&AdapterInfo],
        gates_q15: &[i16],
        decision_hash: B3Hash,
    ) {
        let selected_stable_ids: Vec<u64> = selected_adapters.iter().map(|a| a.stable_id).collect();

        self.emit(
            DiagSeverity::Debug,
            DiagEvent::KsparseSelected {
                step_idx,
                selected_count: selected_adapters.len() as u32,
                selected_stable_ids,
                gates_q15: gates_q15.to_vec(),
                decision_hash,
            },
        );
    }

    /// Emit TieBreakApplied event when two adapters have identical scores.
    ///
    /// # Arguments
    ///
    /// * `step_idx` - Step index within the inference
    /// * `winner_info` - Adapter info for the winner (lower stable_id)
    /// * `loser_info` - Adapter info for the loser (higher stable_id)
    /// * `tied_score` - The tied score value
    pub fn emit_tie_break_applied(
        &self,
        step_idx: u32,
        winner_info: &AdapterInfo,
        loser_info: &AdapterInfo,
        tied_score: f32,
    ) {
        self.emit(
            DiagSeverity::Debug,
            DiagEvent::TieBreakApplied {
                step_idx,
                winner_stable_id: winner_info.stable_id,
                loser_stable_id: loser_info.stable_id,
                tied_score_q15: score_to_q15(tied_score),
            },
        );
    }

    /// Emit RoutingEnd event at exit from route().
    ///
    /// # Arguments
    ///
    /// * `step_idx` - Step index within the inference
    /// * `selected_count` - Number of adapters selected
    /// * `decision_hash` - Final BLAKE3 decision hash
    /// * `policy_mask_digest` - Policy mask digest that was applied
    pub fn emit_routing_end(
        &self,
        step_idx: u32,
        selected_count: u32,
        decision_hash: B3Hash,
        policy_mask_digest: Option<B3Hash>,
    ) {
        // Duration is included but NOT in canonical bytes (skip_serializing_if)
        let duration_us = Some(self.mono_us());

        self.emit(
            DiagSeverity::Debug,
            DiagEvent::RoutingEnd {
                step_idx,
                selected_count,
                decision_hash,
                policy_mask_digest,
                duration_us,
            },
        );
    }
}

/// No-op router diagnostics emitter for when diagnostics are disabled.
#[derive(Clone, Default)]
pub struct NoopRouterDiagEmitter;

impl NoopRouterDiagEmitter {
    pub fn emit_routing_start(&self, _: u32, _: u32, _: u32, _: &[f32]) {}
    pub fn emit_gate_computed(&self, _: u32, _: &AdapterInfo, _: f32) {}
    pub fn emit_ksparse_selected(&self, _: u32, _: &[&AdapterInfo], _: &[i16], _: B3Hash) {}
    pub fn emit_tie_break_applied(&self, _: u32, _: &AdapterInfo, _: &AdapterInfo, _: f32) {}
    pub fn emit_routing_end(&self, _: u32, _: u32, _: B3Hash, _: Option<B3Hash>) {}
}

/// Enum wrapper for router diagnostics (either enabled or no-op).
#[derive(Clone)]
pub enum RouterDiag {
    Enabled(RouterDiagEmitter),
    Disabled(NoopRouterDiagEmitter),
}

impl RouterDiag {
    /// Create a new enabled RouterDiag.
    pub fn enabled(
        emitter: SharedDiagEmitter,
        trace_ctx: TraceContext,
        run_id: DiagRunId,
        tenant_id: impl Into<String>,
    ) -> Self {
        Self::Enabled(RouterDiagEmitter::new(
            emitter, trace_ctx, run_id, tenant_id,
        ))
    }

    /// Create a disabled (no-op) RouterDiag.
    pub fn disabled() -> Self {
        Self::Disabled(NoopRouterDiagEmitter)
    }

    /// Emit RoutingStart event.
    pub fn emit_routing_start(
        &self,
        step_idx: u32,
        candidate_count: u32,
        k: u32,
        features: &[f32],
    ) {
        match self {
            Self::Enabled(e) => e.emit_routing_start(step_idx, candidate_count, k, features),
            Self::Disabled(e) => e.emit_routing_start(step_idx, candidate_count, k, features),
        }
    }

    /// Emit GateComputed event.
    pub fn emit_gate_computed(&self, step_idx: u32, adapter_info: &AdapterInfo, score: f32) {
        match self {
            Self::Enabled(e) => e.emit_gate_computed(step_idx, adapter_info, score),
            Self::Disabled(e) => e.emit_gate_computed(step_idx, adapter_info, score),
        }
    }

    /// Emit KsparseSelected event.
    pub fn emit_ksparse_selected(
        &self,
        step_idx: u32,
        selected_adapters: &[&AdapterInfo],
        gates_q15: &[i16],
        decision_hash: B3Hash,
    ) {
        match self {
            Self::Enabled(e) => {
                e.emit_ksparse_selected(step_idx, selected_adapters, gates_q15, decision_hash)
            }
            Self::Disabled(e) => {
                e.emit_ksparse_selected(step_idx, selected_adapters, gates_q15, decision_hash)
            }
        }
    }

    /// Emit TieBreakApplied event.
    pub fn emit_tie_break_applied(
        &self,
        step_idx: u32,
        winner_info: &AdapterInfo,
        loser_info: &AdapterInfo,
        tied_score: f32,
    ) {
        match self {
            Self::Enabled(e) => {
                e.emit_tie_break_applied(step_idx, winner_info, loser_info, tied_score)
            }
            Self::Disabled(e) => {
                e.emit_tie_break_applied(step_idx, winner_info, loser_info, tied_score)
            }
        }
    }

    /// Emit RoutingEnd event.
    pub fn emit_routing_end(
        &self,
        step_idx: u32,
        selected_count: u32,
        decision_hash: B3Hash,
        policy_mask_digest: Option<B3Hash>,
    ) {
        match self {
            Self::Enabled(e) => {
                e.emit_routing_end(step_idx, selected_count, decision_hash, policy_mask_digest)
            }
            Self::Disabled(e) => {
                e.emit_routing_end(step_idx, selected_count, decision_hash, policy_mask_digest)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod tests {
    use super::*;
    use adapteros_telemetry::diagnostics::NoopDiagEmitter;
    use std::sync::Arc;

    fn make_adapter_info(stable_id: u64, id: &str) -> AdapterInfo {
        AdapterInfo {
            id: id.to_string(),
            stable_id,
            framework: None,
            languages: vec![],
            tier: "tier_1".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            recommended_for_moe: false,
            reasoning_specialties: vec![],
            adapter_type: None,
            stream_session_id: None,
            base_adapter_id: None,
        }
    }

    #[test]
    fn test_score_to_q15_positive() {
        assert_eq!(score_to_q15(0.0), 0);
        assert_eq!(score_to_q15(0.5), 16384); // 0.5 * 32767 = 16383.5 → 16384
        assert_eq!(score_to_q15(1.0), 32767);
    }

    #[test]
    fn test_score_to_q15_negative() {
        // Negative scores should be preserved as negative i32
        assert_eq!(score_to_q15(-0.5), -16384);
        assert_eq!(score_to_q15(-1.0), -32767);
    }

    #[test]
    fn test_noop_emitter_does_not_panic() {
        let diag = RouterDiag::disabled();
        let adapter = make_adapter_info(123, "test-adapter");

        diag.emit_routing_start(0, 10, 3, &[0.1, 0.2, 0.3]);
        diag.emit_gate_computed(0, &adapter, 0.75);
        diag.emit_ksparse_selected(0, &[&adapter], &[16384], B3Hash::hash(b"test"));
        diag.emit_tie_break_applied(0, &adapter, &adapter, 0.5);
        diag.emit_routing_end(0, 1, B3Hash::hash(b"test"), None);
    }

    #[test]
    fn test_enabled_emitter_with_noop_underlying() {
        let emitter: SharedDiagEmitter = Arc::new(NoopDiagEmitter);
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::from_trace_context(&trace_ctx);

        let diag = RouterDiag::enabled(emitter, trace_ctx, run_id, "tenant-123");
        let adapter = make_adapter_info(456, "adapter-456");

        // Should not panic even with noop underlying emitter
        diag.emit_routing_start(0, 5, 2, &[0.0; 22]);
        diag.emit_gate_computed(0, &adapter, 1.5);
        diag.emit_ksparse_selected(0, &[&adapter], &[32767], B3Hash::hash(b"decision"));
        diag.emit_routing_end(0, 1, B3Hash::hash(b"final"), Some(B3Hash::hash(b"policy")));
    }

    #[test]
    fn test_q15_determinism() {
        // Same score should always produce same Q15 value
        let score = 0.123456789f32;
        let q15_1 = score_to_q15(score);
        let q15_2 = score_to_q15(score);
        assert_eq!(q15_1, q15_2);
    }
}
