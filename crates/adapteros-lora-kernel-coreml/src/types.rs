//! Type definitions for CoreML adapter artifacts and status tracking
//!
//! This module contains core types used by the CoreML backend for adapter
//! management, memory tracking, and ANE status monitoring.

use adapteros_core::B3Hash;
use std::path::PathBuf;

/// ANE availability status
#[derive(Debug, Clone)]
pub struct AneStatus {
    pub available: bool,
    pub generation: Option<u8>,
    pub max_batch_size: usize,
    pub deterministic: bool,
}

/// Memory baseline statistics for anomaly detection (Welford's algorithm)
#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryBaseline {
    mean: f64,
    m2: f64,
    count: usize,
}

impl MemoryBaseline {
    pub(crate) fn update(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    pub(crate) fn stddev(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            (self.m2 / (self.count - 1) as f64).sqrt()
        }
    }

    pub(crate) fn z_score(&self, value: f64) -> f64 {
        let std = self.stddev();
        if std == 0.0 {
            0.0
        } else {
            (value - self.mean) / std
        }
    }
}

/// CoreML adapter artifact semantics for hot-swap (PRD 3).
///
/// - `SidecarDelta`: base CoreML package stays resident; LoRA deltas are
///   attached/detached at runtime without recompiling.
/// - `FusedPackage`: pre-fused `.mlmodelc` produced by the export pipeline; the
///   backend can switch to this compiled bundle without restarting the process.
#[derive(Debug, Clone)]
pub enum CoreMLAdapterArtifact {
    SidecarDelta {
        /// Number of floats held in-memory for this adapter.
        len: usize,
        /// Provenance for observability and routing decisions.
        source: CoreMLAdapterSource,
    },
    FusedPackage {
        /// Path to the compiled CoreML bundle that already contains the adapter.
        model_path: PathBuf,
        /// Optional hash of the compiled bundle for identity tracking.
        model_hash: Option<B3Hash>,
    },
}

/// Source of CoreML adapter payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMLAdapterSource {
    /// Safetensors sidecar from the canonical segment (default hot-swap path).
    CanonicalSidecar,
    /// CoreML-specific segment (fused/sidecar emitted by PRD 3 pipeline).
    CoremlSegment,
}
