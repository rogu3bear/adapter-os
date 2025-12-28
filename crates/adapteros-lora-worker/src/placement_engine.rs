//! Placement engine types for backend selection and memory pressure management
//!
//! This module provides:
//! - PlacementState for tracking placement decisions
//! - AvailableBackends for representing configured backends
//! - ensure_preload_allowed for memory pressure guardrails

use crate::device_placement::{
    LaneDescriptor, PlacementDecision, PlacementEngine, TelemetryCollector,
};
use crate::kernel_wrapper::BackendLane;
use crate::memory::MemoryPressureLevel;
use adapteros_core::{AosError, BackendKind, Result};

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
}
