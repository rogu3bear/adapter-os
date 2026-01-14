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
pub mod layer_routing;
pub mod metrics;
pub mod orthogonal;
pub mod path_routing;
pub mod policy_mask;
pub mod scoring;
pub mod types;

mod quantization;
mod router;
pub mod router_diag;

#[cfg(test)]
mod tests;

pub use calibration::{
    CalibrationDataset, CalibrationSample, Calibrator, OptimizationMethod, ValidationMetrics,
};
pub use code_features::{CodeFeatureExtractor, CodeFeatures as CodeFeaturesExt};
pub use constants::*;
pub use features::{
    extract_attn_entropy, CodeFeatures, PromptVerb, FEATURE_SCHEMA_VERSION, MIN_INPUT_LENGTH,
};
pub use framework_routing::{
    compute_framework_scores, FrameworkRoutingContext, FrameworkRoutingScore,
};
pub use metrics::{
    AdapterMetrics, MemoryMetrics, MemoryPressure, RouterMonitoringMetrics, RouterOverheadMetrics,
    ThroughputMetrics,
};
pub use orthogonal::OrthogonalConstraints;
pub use path_routing::{compute_path_scores, DirectoryRoutingContext, PathRoutingScore};
pub use policy_mask::filter_decision_by_policy;
pub use scoring::{create_scorer, EntropyFloorScorer, ScoringFunction, WeightedScorer};

pub(crate) use quantization::quantize_gate;
pub use quantization::{
    GateQuantFormat, Q15_FORMAT_NAME, ROUTER_GATE_Q15_DENOM, ROUTER_GATE_Q15_MAX,
};
pub use router::{sort_scores_deterministic, AbstainContext, Router, TieEvent};
pub use router_diag::{NoopRouterDiagEmitter, RouterDiag, RouterDiagEmitter};
pub use types::{
    AdapterInfo, Decision, DecisionCandidate, DecisionHash, RouterAbstainReason,
    RouterDeterminismConfig, RouterWeights, RoutingDecision, ScoringExplanation,
};

// Layer routing exports (Patent 3535886.0002 Claim 7)
pub use layer_routing::{
    compute_layer_adapter_scores, LayerDecision, LayerFeatures, LayerRouterConfig,
    LayerRoutingChain, LayerRoutingChainSummary, LayerRoutingDecision, LayerScoringContext,
    LayerType,
};
