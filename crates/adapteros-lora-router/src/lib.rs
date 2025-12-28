//! Top-K sparse router with Q15 gate quantization

#![allow(clippy::manual_clamp)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::useless_vec)]

pub mod calibration;
pub mod code_features;
pub mod constants;
pub mod features;
pub mod framework_routing;
pub mod metrics;
pub mod orthogonal;
pub mod path_routing;
pub mod policy_mask;
pub mod scoring;
pub mod types;

mod quantization;
mod router;

#[cfg(test)]
mod tests;

pub use calibration::{
    CalibrationDataset, CalibrationSample, Calibrator, OptimizationMethod, ValidationMetrics,
};
pub use code_features::{CodeFeatureExtractor, CodeFeatures as CodeFeaturesExt};
pub use constants::*;
pub use features::{extract_attn_entropy, CodeFeatures, PromptVerb};
pub use framework_routing::{
    compute_framework_scores, FrameworkRoutingContext, FrameworkRoutingScore,
};
pub use metrics::{
    AdapterMetrics, MemoryMetrics, MemoryPressure, RouterMonitoringMetrics, RouterOverheadMetrics,
    ThroughputMetrics,
};
pub use orthogonal::OrthogonalConstraints;
pub use path_routing::{compute_path_scores, DirectoryRoutingContext, PathRoutingScore};
pub use scoring::{create_scorer, EntropyFloorScorer, ScoringFunction, WeightedScorer};

pub(crate) use quantization::quantize_gate;
pub use quantization::{ROUTER_GATE_Q15_DENOM, ROUTER_GATE_Q15_MAX};
pub use router::{AbstainContext, Router};
pub use types::{
    AdapterInfo, Decision, DecisionCandidate, DecisionHash, RouterAbstainReason,
    RouterDeterminismConfig, RouterWeights, RoutingDecision, ScoringExplanation,
};
