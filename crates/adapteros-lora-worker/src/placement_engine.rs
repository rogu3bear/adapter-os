//! Placement engine types for backend selection and memory pressure management
//!
//! This module provides:
//! - PlacementState for tracking placement decisions
//! - AvailableBackends for representing configured backends
//! - ensure_preload_allowed for memory pressure guardrails

use crate::adapter_hotswap::FallbackCandidate;
use crate::device_placement::{
    LaneDescriptor, PlacementDecision, PlacementEngine, TelemetryCollector,
};
use crate::kernel_wrapper::BackendLane;
use crate::memory::MemoryPressureLevel;
use adapteros_core::{AosError, B3Hash, BackendKind, Result};

/// CoreML runtime telemetry captured for replay/logging
#[derive(Debug, Clone, Default)]
pub struct CoremlRuntimeTelemetry {
    pub compute_preference: Option<String>,
    pub compute_units: Option<String>,
    pub gpu_available: Option<bool>,
    pub ane_available: Option<bool>,
    pub gpu_used: Option<bool>,
    pub ane_used: Option<bool>,
    pub production_mode: Option<bool>,
}

/// Backends available in this worker (primary + optional fallback).
#[derive(Debug, Clone)]
pub struct AvailableBackends {
    pub primary: BackendKind,
    pub fallback: Option<BackendKind>,
    pub coreml_primary: Option<CoremlRuntimeTelemetry>,
    pub coreml_fallback: Option<CoremlRuntimeTelemetry>,
}

impl AvailableBackends {
    pub fn contains(&self, backend: BackendKind) -> bool {
        self.primary == backend || self.fallback == Some(backend)
    }

    pub fn lane_for(&self, backend: BackendKind) -> BackendLane {
        if self.fallback == Some(backend) {
            BackendLane::Fallback
        } else {
            BackendLane::Primary
        }
    }
}

#[allow(dead_code)]
pub(crate) struct PlacementState {
    engine: PlacementEngine,
    telemetry: TelemetryCollector,
    lanes: Vec<LaneDescriptor>,
}

#[allow(dead_code)]
impl PlacementState {
    pub fn new(
        engine: PlacementEngine,
        telemetry: TelemetryCollector,
        lanes: Vec<LaneDescriptor>,
    ) -> Self {
        Self {
            engine,
            telemetry,
            lanes,
        }
    }

    pub fn decide(&mut self) -> Option<PlacementDecision> {
        let snapshot = self.telemetry.snapshot();
        self.engine.choose_lane(&self.lanes, &snapshot)
    }
}

/// Build an ordered list of fallback candidates from adapter IDs and their hashes.
///
/// The input pairs should be ordered by routing score (highest first). This is
/// typically constructed by mapping `Decision.candidates` through the adapter
/// registry to resolve string IDs and weight hashes.
///
/// # Example
///
/// ```ignore
/// let candidates = build_fallback_candidates(&[
///     ("top-adapter", hash_a),
///     ("second-adapter", hash_b),
/// ]);
/// let outcome = hotswap.try_load_with_fallback(&candidates, timeout).await?;
/// ```
pub fn build_fallback_candidates(
    adapter_id_hash_pairs: &[(&str, B3Hash)],
) -> Vec<FallbackCandidate> {
    adapter_id_hash_pairs
        .iter()
        .map(|(id, hash)| FallbackCandidate {
            adapter_id: id.to_string(),
            hash: *hash,
        })
        .collect()
}

/// Enforce guardrails for adapter preload under memory pressure.
pub fn ensure_preload_allowed(
    pressure_before: MemoryPressureLevel,
    pressure_after: MemoryPressureLevel,
) -> Result<()> {
    if matches!(pressure_after, MemoryPressureLevel::Critical) {
        return Err(AosError::MemoryPressure(
            "Memory pressure critical, cannot load more adapters".to_string(),
        ));
    }

    // If we started at critical but recovered, allow the load to proceed.
    tracing::debug!(
        pressure_before = ?pressure_before,
        pressure_after = ?pressure_after,
        "Preload allowed after memory pressure check"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_guard_blocks_on_critical_pressure() {
        let err =
            ensure_preload_allowed(MemoryPressureLevel::Critical, MemoryPressureLevel::Critical)
                .unwrap_err();

        match err {
            AosError::MemoryPressure(msg) => {
                assert!(msg.contains("Memory pressure critical, cannot load more adapters"))
            }
            other => panic!("expected memory pressure error, got {:?}", other),
        }
    }

    #[test]
    fn preload_guard_allows_after_recovery() {
        let res = ensure_preload_allowed(MemoryPressureLevel::Critical, MemoryPressureLevel::Low);
        assert!(res.is_ok());
    }

    #[test]
    fn build_fallback_candidates_preserves_order() {
        let hash_a = B3Hash::hash(b"adapter-a");
        let hash_b = B3Hash::hash(b"adapter-b");
        let hash_c = B3Hash::hash(b"adapter-c");

        let candidates = build_fallback_candidates(&[
            ("adapter-a", hash_a),
            ("adapter-b", hash_b),
            ("adapter-c", hash_c),
        ]);

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].adapter_id, "adapter-a");
        assert_eq!(candidates[0].hash, hash_a);
        assert_eq!(candidates[1].adapter_id, "adapter-b");
        assert_eq!(candidates[2].adapter_id, "adapter-c");
    }

    #[test]
    fn build_fallback_candidates_empty_input() {
        let candidates = build_fallback_candidates(&[]);
        assert!(candidates.is_empty());
    }
}
