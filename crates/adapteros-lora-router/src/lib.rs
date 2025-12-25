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

use adapteros_core::{determinism::DeterminismContext, B3Hash, Result};
use policy_mask::{PolicyMask, PolicyOverrideFlags};
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;

/// Q15 gate constants for router outputs (deterministic, non-negative gates)
///
/// # Why 32767 and not 32768?
///
/// Q15 fixed-point format represents values in [-1.0, 1.0) using signed 16-bit integers.
/// The maximum positive value is 32767 (0x7FFF), not 32768, because:
///
/// 1. **i16 range**: -32768 to 32767. Using 32768 would overflow.
/// 2. **Precision**: 32767.0 gives exact representation of 1.0 when gate=32767.
///    Using 32768.0 would make max gate = 0.99997, losing the ability to express "full weight".
/// 3. **Determinism**: Consistent denominator ensures identical f32→Q15→f32 round-trips.
///
/// # Usage
/// - Encode: `gate_q15 = (gate_f32 * 32767.0).round() as i16`
/// - Decode: `gate_f32 = gate_q15 as f32 / 32767.0`
///
/// # Critical Invariant
/// **DO NOT CHANGE TO 32768** - This would break determinism proofs and replay verification.
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;

/// Router determinism configuration
///
/// Controls deterministic floating-point behavior and decision hashing
/// to ensure reproducible routing decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDeterminismConfig {
    /// Use IEEE 754 deterministic softmax with f64 intermediate precision and Kahan summation
    pub ieee754_deterministic: bool,
    /// Enable decision hashing with BLAKE3 for audit trail
    pub enable_decision_hashing: bool,
}

impl Default for RouterDeterminismConfig {
    fn default() -> Self {
        Self {
            ieee754_deterministic: true,   // Enabled by default for reproducibility
            enable_decision_hashing: true, // Enabled by default for audit trail
        }
    }
}

/// Decision hash for audit and reproducibility verification
///
/// Contains BLAKE3 hash of routing inputs and outputs, along with metadata
/// to enable determinism proofs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionHash {
    /// BLAKE3 hash of input features and priors
    pub input_hash: String,
    /// BLAKE3 hash of output indices and gates
    pub output_hash: String,
    /// Combined hash of input + output for compact verification
    pub combined_hash: String,
    /// Tau (temperature) used in this decision
    pub tau: f32,
    /// Epsilon (entropy floor) used in this decision
    pub eps: f32,
    /// K (number of selected adapters)
    pub k: usize,
}

// Telemetry imports
use adapteros_telemetry::events::{RouterCandidate as TelemetryCandidate, RouterDecisionEvent};
use adapteros_telemetry::writer::RouterDecisionWriter;

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

// Import policy configuration for entropy floor
use adapteros_policy::packs::router::RouterConfig;

/// Router weights for feature importance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterWeights {
    /// Weight for language detection (0.3 - strong signal)
    pub language_weight: f32,
    /// Weight for framework detection (0.25 - strong signal)
    pub framework_weight: f32,
    /// Weight for symbol hits (0.2 - moderate signal)
    pub symbol_hits_weight: f32,
    /// Weight for path tokens (0.15 - moderate signal)
    pub path_tokens_weight: f32,
    /// Weight for prompt verb (0.1 - weak signal)
    pub prompt_verb_weight: f32,

    // DIR (Deterministic Inference Runtime) additions
    // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    /// Weight for orthogonal constraints (0.05 - weak signal)
    pub orthogonal_weight: f32,
    /// Weight for adapter diversity (0.03 - weak signal)
    pub diversity_weight: f32,
    /// Weight for similarity penalty (0.02 - weak signal)
    pub similarity_penalty: f32,
}

impl Default for RouterWeights {
    fn default() -> Self {
        Self {
            language_weight: 0.27272728,
            framework_weight: 0.22727273,
            symbol_hits_weight: 0.18181819,
            path_tokens_weight: 0.13636364,
            prompt_verb_weight: 0.09090909,
            orthogonal_weight: 0.04545455,
            diversity_weight: 0.02727273,
            similarity_penalty: 0.01818182,
        }
    }
}

impl RouterWeights {
    /// Create custom weights
    pub fn new(language: f32, framework: f32, symbols: f32, paths: f32, verb: f32) -> Self {
        Self {
            language_weight: language,
            framework_weight: framework,
            symbol_hits_weight: symbols,
            path_tokens_weight: paths,
            prompt_verb_weight: verb,
            orthogonal_weight: 0.04545455,
            diversity_weight: 0.02727273,
            similarity_penalty: 0.01818182,
        }
    }

    /// Create custom weights with DIR (Deterministic Inference Runtime) parameters
    pub fn new_with_dir_weights(
        language: f32,
        framework: f32,
        symbols: f32,
        paths: f32,
        verb: f32,
        orthogonal: f32,
        diversity: f32,
        similarity: f32,
    ) -> Self {
        Self {
            language_weight: language,
            framework_weight: framework,
            symbol_hits_weight: symbols,
            path_tokens_weight: paths,
            prompt_verb_weight: verb,
            orthogonal_weight: orthogonal,
            diversity_weight: diversity,
            similarity_penalty: similarity,
        }
    }

    /// Get total weight (for normalization check)
    pub fn total_weight(&self) -> f32 {
        self.language_weight
            + self.framework_weight
            + self.symbol_hits_weight
            + self.path_tokens_weight
            + self.prompt_verb_weight
            + self.orthogonal_weight
            + self.diversity_weight
            + self.similarity_penalty
    }

    /// Load weights from JSON file
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        serde_json::from_str(&content).map_err(|e| adapteros_core::AosError::Io(e.to_string()))
    }

    /// Load weights from TOML file
    pub fn load_toml(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        toml::from_str(&content).map_err(|e| adapteros_core::AosError::Io(e.to_string()))
    }

    /// Save weights to JSON file
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        std::fs::write(path.as_ref(), content)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))
    }

    /// Save weights to TOML file
    pub fn save_toml(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = toml::to_string_pretty(&self)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        std::fs::write(path.as_ref(), content)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))
    }
}

/// Router for selecting K adapters with quantized gates
pub struct Router {
    /// Feature weights for scoring
    feature_weights: RouterWeights,
    /// Number of top adapters to select
    k: usize,
    /// Temperature for softmax
    tau: f32,
    /// Entropy floor to prevent collapse
    eps: f32,
    /// Token counter for sampling (reserved for token-level telemetry)
    _token_count: usize,
    /// Log first N tokens fully (default: 128 per Telemetry Ruleset #9)
    full_log_tokens: usize,

    // DIR (Deterministic Inference Runtime) enhancements
    // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    /// Orthogonal constraints tracker
    orthogonal_constraints: Option<OrthogonalConstraints>,
    /// Whether orthogonal constraints are enabled
    orthogonal_enabled: bool,
    /// Compression ratio for multi-path outputs
    compression_ratio: f32,
    /// Whether shared downsample is enabled
    shared_downsample: bool,

    // Adapter stack support
    /// Currently active stack name (if any)
    active_stack_name: Option<String>,
    /// Adapter IDs that are part of the active stack
    active_stack_adapter_ids: Option<Vec<String>>,
    /// Cached hash of the active stack configuration
    active_stack_hash: Option<B3Hash>,

    // Telemetry
    /// Optional telemetry writer for routing decisions (non-blocking)
    telemetry_writer: Option<RouterDecisionWriter>,
    /// Current step counter for telemetry correlation
    step_counter: usize,

    // Determinism controls
    /// Determinism configuration for reproducible routing
    determinism_config: RouterDeterminismConfig,
    /// Whether adaptive (seeded, reproducible) tie-break routing is enabled
    adaptive_routing: bool,

    // Abstain thresholds
    /// Entropy threshold above which to abstain (high uncertainty)
    abstain_entropy_threshold: Option<f32>,
    /// Confidence threshold below which to abstain (low max gate)
    abstain_confidence_threshold: Option<f32>,
    /// Optional telemetry writer for abstain events
    abstain_telemetry_writer: Option<std::sync::Arc<adapteros_telemetry::TelemetryWriter>>,
}

impl Router {
    /// Create a new router with custom feature weights
    pub fn new_with_weights(feature_weights: RouterWeights, k: usize, tau: f32, eps: f32) -> Self {
        Self {
            feature_weights,
            k,
            tau,
            eps,
            _token_count: 0,
            full_log_tokens: 128, // Per Telemetry Ruleset #9
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
            telemetry_writer: None,
            step_counter: 0,
            determinism_config: RouterDeterminismConfig::default(),
            adaptive_routing: false,
            abstain_entropy_threshold: None,
            abstain_confidence_threshold: None,
            abstain_telemetry_writer: None,
        }
    }

    /// Create a new router from policy configuration
    ///
    /// This constructor reads the entropy floor and other parameters from the policy configuration
    /// instead of hardcoding values, ensuring consistency across the system.
    ///
    /// # Arguments
    /// * `feature_weights` - Custom router weights for feature importance
    /// * `k` - Number of top adapters to select (K-sparse parameter)
    /// * `tau` - Temperature for softmax
    /// * `policy_config` - Router policy configuration containing entropy floor and other settings
    ///
    /// # Returns
    /// New Router instance configured from policy
    pub fn new_with_policy_config(
        feature_weights: RouterWeights,
        k: usize,
        tau: f32,
        policy_config: &RouterConfig,
    ) -> Self {
        // Validate K against policy
        if k > policy_config.k_sparse {
            tracing::warn!(
                "Requested k={} exceeds policy maximum k_sparse={}, clamping to policy maximum",
                k,
                policy_config.k_sparse
            );
        }

        Self {
            feature_weights,
            k: k.min(policy_config.k_sparse),
            tau,
            eps: policy_config.entropy_floor,
            _token_count: 0,
            full_log_tokens: policy_config.sample_tokens_full,
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
            telemetry_writer: None,
            step_counter: 0,
            determinism_config: RouterDeterminismConfig::default(),
            adaptive_routing: false,
            abstain_entropy_threshold: policy_config.abstain_entropy_threshold,
            abstain_confidence_threshold: policy_config.abstain_confidence_threshold,
            abstain_telemetry_writer: None,
        }
    }

    /// Set the telemetry writer for routing decision events
    pub fn set_telemetry_writer(&mut self, writer: RouterDecisionWriter) {
        self.telemetry_writer = Some(writer);
    }

    /// Clear the telemetry writer (useful for testing)
    pub fn clear_telemetry_writer(&mut self) {
        self.telemetry_writer = None;
    }

    /// Set the telemetry writer for abstain events
    pub fn set_abstain_telemetry_writer(
        &mut self,
        writer: std::sync::Arc<adapteros_telemetry::TelemetryWriter>,
    ) {
        self.abstain_telemetry_writer = Some(writer);
    }

    /// Set abstain thresholds
    pub fn set_abstain_thresholds(
        &mut self,
        entropy_threshold: Option<f32>,
        confidence_threshold: Option<f32>,
    ) {
        self.abstain_entropy_threshold = entropy_threshold;
        self.abstain_confidence_threshold = confidence_threshold;
    }

    /// Set determinism configuration
    pub fn set_determinism_config(&mut self, config: RouterDeterminismConfig) {
        self.determinism_config = config;
    }

    /// Configure routing determinism (adaptive uses seeded tie-breaks and stays reproducible
    /// when a determinism context is provided)
    pub fn set_routing_determinism_mode(&mut self, adaptive: bool) {
        self.adaptive_routing = adaptive;
    }

    /// Whether adaptive routing is enabled
    pub fn adaptive_routing(&self) -> bool {
        self.adaptive_routing
    }

    /// Get current determinism configuration
    pub fn determinism_config(&self) -> &RouterDeterminismConfig {
        &self.determinism_config
    }

    /// Get the full log token count (for testing)
    pub fn full_log_tokens(&self) -> usize {
        self.full_log_tokens
    }

    /// Emit a router decision telemetry event (non-blocking)
    ///
    /// This method does NOT fail on errors - it logs and continues to avoid blocking the hot path.
    fn emit_decision_event(&mut self, decision: &Decision, input_token_id: Option<u32>) {
        if let Some(ref writer) = self.telemetry_writer {
            // Convert Decision to RouterDecisionEvent
            let candidates: Vec<TelemetryCandidate> = decision
                .candidates
                .iter()
                .map(|c| TelemetryCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                })
                .collect();

            let event = RouterDecisionEvent {
                step: self.step_counter,
                input_token_id,
                candidate_adapters: candidates,
                entropy: decision.entropy,
                tau: self.tau,
                entropy_floor: self.eps,
                stack_hash: self.active_stack_hash.map(|h| h.to_short_hex()),
                stack_id: self.active_stack_name.clone(),
                stack_version: None, // Will be populated by stack metadata
            };

            // Emit event (non-blocking, logs on error)
            if let Err(e) = writer.emit(event) {
                tracing::debug!(
                    error = %e,
                    step = self.step_counter,
                    "Router decision telemetry dropped (channel full)"
                );
            }

            // Increment step counter
            self.step_counter += 1;
        }
    }

    /// Check abstain conditions and emit AbstainEvent if triggered
    ///
    /// This method checks both entropy and confidence thresholds:
    /// - High entropy: indicates router uncertainty about adapter selection
    /// - Low confidence: max gate value is below threshold, indicating no strong selection
    ///
    /// Events are emitted via telemetry writer if thresholds are configured and exceeded.
    ///
    /// Note: Empty decisions (no adapters selected) are skipped - they represent
    /// policy-based abstention rather than uncertainty-based abstention.
    fn check_abstain_conditions(&self, entropy: f32, gates: &[f32]) {
        use adapteros_telemetry::events::AbstainEvent;

        // Skip abstain checks for empty decisions (already abstained via policy/no adapters)
        if gates.is_empty() {
            return;
        }

        if let Some(ref writer) = self.abstain_telemetry_writer {
            // Check high entropy threshold
            if let Some(entropy_threshold) = self.abstain_entropy_threshold {
                if entropy > entropy_threshold {
                    let event = AbstainEvent::high_entropy(entropy, entropy_threshold);
                    if let Err(e) = writer.log_abstain(event) {
                        tracing::debug!(
                            error = %e,
                            entropy = entropy,
                            threshold = entropy_threshold,
                            "Failed to emit high entropy abstain event"
                        );
                    } else {
                        tracing::info!(
                            entropy = entropy,
                            threshold = entropy_threshold,
                            "Router abstaining due to high entropy (uncertainty)"
                        );
                    }
                }
            }

            // Check low confidence threshold (max gate below threshold)
            if let Some(confidence_threshold) = self.abstain_confidence_threshold {
                let max_gate = gates.iter().fold(0.0f32, |a, &b| a.max(b));
                if max_gate < confidence_threshold {
                    let event = AbstainEvent::low_confidence(max_gate, confidence_threshold);
                    if let Err(e) = writer.log_abstain(event) {
                        tracing::debug!(
                            error = %e,
                            max_gate = max_gate,
                            threshold = confidence_threshold,
                            "Failed to emit low confidence abstain event"
                        );
                    } else {
                        tracing::info!(
                            max_gate = max_gate,
                            threshold = confidence_threshold,
                            "Router abstaining due to low confidence (max gate below threshold)"
                        );
                    }
                }
            }
        }
    }

    /// Create a new router with default weights (for backward compatibility)
    pub fn new(_weights: Vec<f32>, k: usize, tau: f32, eps: f32, _seed: [u8; 32]) -> Result<Self> {
        if k > MAX_K {
            return Err(adapteros_core::AosError::Routing(
                format!(
                    "Adapter routing failed: K={} exceeds maximum allowed value of {}. Reduce the number of top adapters to select (K parameter) to {} or less.",
                    k, MAX_K, MAX_K
                )
            ));
        }
        // Legacy constructor - ignores old weights vector, uses default RouterWeights
        Ok(Self::new_with_weights(
            RouterWeights::default(),
            k,
            tau,
            eps,
        ))
    }

    /// Get temperature (tau) for telemetry
    pub fn tau(&self) -> f32 {
        self.tau
    }

    /// Get entropy floor (eps) for telemetry
    pub fn eps(&self) -> f32 {
        self.eps
    }

    /// Set full log token count
    pub fn set_full_log_tokens(&mut self, count: usize) {
        self.full_log_tokens = count;
    }

    /// Score framework adapters using the configured framework weight.
    pub fn score_frameworks(
        &self,
        query: &str,
        contexts: &[framework_routing::FrameworkRoutingContext],
    ) -> Vec<framework_routing::FrameworkRoutingScore> {
        framework_routing::compute_framework_scores(
            query,
            contexts,
            self.feature_weights.framework_weight,
        )
    }

    /// Score directory adapters using the configured path weight.
    pub fn score_paths(
        &self,
        query: &str,
        contexts: &[path_routing::DirectoryRoutingContext],
    ) -> Vec<path_routing::PathRoutingScore> {
        path_routing::compute_path_scores(query, contexts, self.feature_weights.path_tokens_weight)
    }

    /// Enable orthogonal constraints for DIR (Deterministic Inference Runtime)
    /// Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    pub fn set_orthogonal_constraints(
        &mut self,
        enabled: bool,
        similarity_threshold: f32,
        penalty_weight: f32,
        history_window: usize,
    ) {
        self.orthogonal_enabled = enabled;
        if enabled {
            self.orthogonal_constraints = Some(OrthogonalConstraints::new(
                similarity_threshold,
                penalty_weight,
                history_window,
            ));
        } else {
            self.orthogonal_constraints = None;
        }
    }

    /// Set compression ratio for multi-path outputs
    pub fn set_compression_ratio(&mut self, ratio: f32) {
        self.compression_ratio = ratio.clamp(0.1, 1.0);
    }

    /// Enable shared downsample matrix
    pub fn set_shared_downsample(&mut self, enabled: bool) {
        self.shared_downsample = enabled;
    }

    /// Set the active adapter stack for filtering
    pub fn set_active_stack(
        &mut self,
        stack_name: Option<String>,
        adapter_ids: Option<Vec<String>>,
        stack_hash: Option<B3Hash>,
    ) {
        tracing::debug!(
            "Setting active stack: {:?} with {} adapters, hash: {:?}",
            stack_name,
            adapter_ids.as_ref().map(|ids| ids.len()).unwrap_or(0),
            stack_hash.as_ref().map(|h| h.to_short_hex())
        );
        self.active_stack_name = stack_name;
        self.active_stack_adapter_ids = adapter_ids;
        self.active_stack_hash = stack_hash;
    }

    /// Get the currently active stack name
    pub fn active_stack(&self) -> Option<&String> {
        self.active_stack_name.as_ref()
    }

    /// Get the cached stack hash as hex string
    pub fn stack_hash(&self) -> Option<String> {
        self.active_stack_hash.map(|hash| hash.to_short_hex())
    }

    /// Filter adapter indices based on the active stack
    fn filter_by_stack(&self, adapter_info: &[AdapterInfo]) -> Vec<usize> {
        match &self.active_stack_adapter_ids {
            Some(allowed_ids) => {
                tracing::debug!(
                    "Filtering adapters by stack: {} allowed IDs",
                    allowed_ids.len()
                );
                adapter_info
                    .iter()
                    .enumerate()
                    .filter(|(_, info)| allowed_ids.contains(&info.id))
                    .map(|(idx, _)| idx)
                    .collect()
            }
            None => {
                // No stack active, all adapters are candidates
                (0..adapter_info.len()).collect()
            }
        }
    }

    /// Check if an adapter is in the active stack (reserved for stack-based filtering)
    fn _is_in_active_stack(&self, adapter_id: &str) -> bool {
        match &self.active_stack_adapter_ids {
            Some(allowed_ids) => allowed_ids.contains(&adapter_id.to_string()),
            None => true, // No stack = all adapters allowed
        }
    }

    /// Get current diversity score from orthogonal constraints
    pub fn diversity_score(&self) -> f32 {
        if let Some(ref constraints) = self.orthogonal_constraints {
            constraints.diversity_score()
        } else {
            1.0 // Maximum diversity when constraints are disabled
        }
    }

    /// Get the softmax temperature (tau) used by this router
    pub fn temperature(&self) -> f32 {
        self.tau
    }

    /// Get the entropy floor (epsilon) enforced by this router
    pub fn entropy_floor(&self) -> f32 {
        self.eps
    }

    /// Compute weighted feature score from 22-dimensional feature vector
    ///
    /// Feature vector layout:
    /// - [0..8]: language one-hot (8 dims)
    /// - [8..11]: framework scores (3 dims)
    /// - [11]: symbol hits (1 dim)
    /// - [12]: path tokens (1 dim)
    /// - [13..21]: prompt verb one-hot (8 dims)
    /// - [21]: attention entropy (1 dim)
    ///
    /// DEPRECATED: This method computes a global score that's the same for all adapters.
    /// Use `compute_adapter_feature_score` instead for per-adapter scoring.
    fn compute_weighted_score(&self, features: &[f32]) -> f32 {
        // Support both legacy (21) and new (22) feature vectors
        if features.len() != 21 && features.len() != 22 {
            // Fallback for unexpected feature vectors
            return features.iter().sum::<f32>() * 0.1;
        }

        let mut score = 0.0;

        // Language component (take max of one-hot as language strength)
        let lang_strength = features[0..8].iter().fold(0.0f32, |a, &b| a.max(b));
        score += lang_strength * self.feature_weights.language_weight;

        // Framework component (sum of top 3 framework scores)
        let framework_strength = features[8..11].iter().sum::<f32>();
        score += framework_strength * self.feature_weights.framework_weight;

        // Symbol hits component
        score += features[11] * self.feature_weights.symbol_hits_weight;

        // Path tokens component
        score += features[12] * self.feature_weights.path_tokens_weight;

        // Prompt verb component (max of one-hot)
        let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));
        score += verb_strength * self.feature_weights.prompt_verb_weight;

        score
    }

    /// Compute per-adapter feature score based on adapter metadata
    ///
    /// This is the correct scoring method that produces different scores for each adapter
    /// based on how well the adapter's metadata matches the detected features.
    ///
    /// # Arguments
    /// * `features` - Global 22-dimensional feature vector from prompt/context
    /// * `adapter_info` - Adapter metadata (framework, languages, tier)
    ///
    /// # Returns
    /// Adapter-specific feature relevance score
    fn compute_adapter_feature_score(&self, features: &[f32], adapter_info: &AdapterInfo) -> f32 {
        if features.len() != 21 && features.len() != 22 && features.len() != 25 {
            return 0.0;
        }

        let mut score = 0.0;

        // Language affinity: Check if adapter supports the detected language
        // features[0..8] is language one-hot encoding
        if !features.is_empty() && features.len() >= 8 {
            let detected_lang_idx = features[0..8]
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx);

            if let Some(lang_idx) = detected_lang_idx {
                if adapter_info.supports_language(lang_idx) && features[lang_idx] > 0.0 {
                    // Boost score if adapter supports the detected language
                    score += features[lang_idx] * self.feature_weights.language_weight * 2.0;
                }
            }
        }

        // Framework affinity: Check if adapter's framework matches detected framework
        // features[8..11] are framework scores
        if features.len() >= 11 {
            if let Some(ref _adapter_framework) = adapter_info.framework {
                // Framework scores are already extracted in features[8..11]
                // We give a boost if the adapter has a framework specialization
                let framework_strength = features[8..11].iter().sum::<f32>();
                if framework_strength > 0.0 {
                    // Boost adapters that have framework specialization when frameworks are detected
                    score += framework_strength * self.feature_weights.framework_weight * 1.5;
                }
            }
        }

        // Symbol hits: All adapters benefit equally from symbol density
        if features.len() >= 12 {
            score += features[11] * self.feature_weights.symbol_hits_weight;
        }

        // Path tokens: All adapters benefit equally from path context
        if features.len() >= 13 {
            score += features[12] * self.feature_weights.path_tokens_weight;
        }

        // Prompt verb: All adapters benefit equally from verb classification
        if features.len() >= 21 {
            let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));
            score += verb_strength * self.feature_weights.prompt_verb_weight;
        }

        // Tier-based boost: Higher tiers get slight boost
        let tier_boost = match adapter_info.tier.as_str() {
            "tier_0" => 0.3,
            "tier_1" => 0.2,
            "tier_2" => 0.1,
            _ => 0.0,
        };
        score += tier_boost;

        if let Some(ref lora_tier) = adapter_info.lora_tier {
            let tier_bonus = match lora_tier.as_str() {
                "max" => 0.12,
                "standard" => 0.06,
                "micro" => 0.0,
                _ => 0.0,
            };
            score += tier_bonus;
        }

        score
    }

    /// Score and select top-K adapters
    ///
    /// # DEPRECATED - Use route_with_adapter_info() instead
    ///
    /// This method uses a global feature score that doesn't distinguish between adapters.
    /// It's maintained for backward compatibility only.
    ///
    /// For proper per-adapter scoring:
    /// ```rust
    /// use adapteros_lora_router::{AdapterInfo, Router, RouterWeights, policy_mask::PolicyMask};
    ///
    /// // Old (deprecated):
    /// let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    /// let features = vec![0.0f32; 22];
    /// let priors = vec![0.5f32, 0.3f32, 0.2f32];
    /// #[allow(deprecated)]
    /// let _decision = router.route(&features, &priors);
    ///
    /// // New (recommended):
    /// let adapter_info: Vec<AdapterInfo> = (0..priors.len())
    ///     .map(|i| AdapterInfo {
    ///         id: format!("adapter_{}", i),
    ///         framework: None,
    ///         languages: vec![],
    ///         tier: "default".to_string(),
    ///         scope_path: None,
    ///         lora_tier: None,
    ///         base_model: None,
    ///     })
    ///     .collect();
    /// let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    /// let mask = PolicyMask::allow_all(&adapter_ids, None);
    /// let decision = router
    ///     .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
    ///     .expect("router decision");
    /// assert_eq!(decision.indices.len(), 2);
    /// ```
    ///
    /// The new API enables:
    /// - Per-adapter feature scoring (language affinity, framework specialization)
    /// - Proper orthogonality penalties during selection
    /// - Stack-based filtering
    /// - DIR diversity controls
    ///
    /// See [ROUTER_MIGRATION.md](../../docs/ROUTER_MIGRATION.md) for complete migration steps.
    #[deprecated(
        since = "0.1.1",
        note = "Use route_with_adapter_info() for per-adapter scoring"
    )]
    pub fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        tracing::warn!(
            "Router::route() is deprecated, use route_with_adapter_info() instead. \
             See docs/ROUTER_MIGRATION.md for migration guide"
        );
        // Compute weighted feature score once
        let feature_score = self.compute_weighted_score(features);

        // Compute scores for each adapter combining prior and features
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
            .map(|(i, &prior)| {
                let score = prior + feature_score;
                (i, score)
            })
            .collect();

        // Sort by score descending, then by index for determinism (tie-breaker keeps per-token decisions stable)
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Softmax with temperature using deterministic f64 + Kahan path
        let mut gates: Vec<f32> = Self::deterministic_softmax(&top_k, self.tau);
        let min_gate = self.eps / self.k as f32;
        for g in &mut gates {
            *g = g.max(min_gate);
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        // Quantize to Q15 (denominator 32767.0) so that identical inputs produce the same gates_q15 on every run
        let gates_q15: SmallVec<[i16; 8]> = gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        let entropy = Self::compute_entropy(&gates);

        let candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates_q15.iter())
            .map(|((adapter_idx, raw_score), &gate_q15)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15,
            })
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        // Assert 1:1 mapping
        assert_eq!(
            indices.len(),
            gates_q15.len(),
            "RouterRing must match gate count"
        );
        assert!(
            indices.len() == indices.iter().collect::<HashSet<_>>().len(),
            "Indices must be unique"
        );

        // Apply orthogonal constraints if enabled
        if self.orthogonal_enabled {
            if let Some(ref mut constraints) = self.orthogonal_constraints {
                // Update activation history for diversity tracking
                // Note: Penalty-based rescoring deferred to post-alpha (DIR full implementation)
                // See: https://openreview.net/pdf?id=jqz6Msm3AF
                constraints.update_history(&indices, &gates_q15);
            }
        }

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash: None, // Deprecated method doesn't use decision hashing
            policy_mask_digest: None,
            policy_overrides_applied: None,
        };

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        decision
    }

    /// Compute Shannon entropy of gate distribution
    fn compute_entropy(gates: &[f32]) -> f32 {
        gates
            .iter()
            .filter(|&&g| g > 0.0)
            .map(|&g| -g * g.log2())
            .sum()
    }

    /// Deterministic softmax using f64 intermediate precision and Kahan summation
    ///
    /// This implementation provides IEEE 754 deterministic behavior by:
    /// 1. Using f64 for intermediate computations to reduce rounding errors
    /// 2. Kahan summation for numerically stable sum calculation
    /// 3. Consistent ordering of operations
    ///
    /// # Arguments
    /// * `logits` - Input scores (raw logits)
    /// * `tau` - Temperature parameter for softmax
    ///
    /// # Returns
    /// Softmax probabilities as f32 (converted from f64 intermediate precision)
    pub(crate) fn deterministic_softmax(logits: &[(usize, f32)], tau: f32) -> Vec<f32> {
        if logits.is_empty() {
            return Vec::new();
        }

        // Find max for numerical stability (use f64 for intermediate computation)
        let max = logits
            .iter()
            .map(|(_, score)| *score as f64)
            .fold(f64::NEG_INFINITY, f64::max);

        // Compute exponentials and sum using Kahan summation for numerical stability
        let mut sum = 0.0f64;
        let mut c = 0.0f64; // Kahan summation compensation
        let exps: Vec<f64> = logits
            .iter()
            .map(|(_, score)| {
                let exp = (((*score as f64) - max) / (tau as f64)).exp();

                // Kahan summation: accumulate with compensation for lost low-order bits
                let y = exp - c;
                let t = sum + y;
                c = (t - sum) - y;
                sum = t;

                exp
            })
            .collect();

        // Normalize and convert back to f32
        if sum == 0.0 {
            // All logits were -inf, return uniform distribution
            let uniform = 1.0f32 / logits.len() as f32;
            vec![uniform; logits.len()]
        } else {
            exps.iter().map(|&e| (e / sum) as f32).collect()
        }
    }

    /// Compute decision hash for audit and reproducibility verification
    ///
    /// Hashes both inputs and outputs to create a verifiable audit trail.
    ///
    /// # Arguments
    /// * `features` - Input feature vector
    /// * `priors` - Input prior scores
    /// * `indices` - Output adapter indices
    /// * `gates_q15` - Output quantized gates
    ///
    /// # Returns
    /// DecisionHash containing input, output, and combined hashes
    fn compute_decision_hash(
        &self,
        features: &[f32],
        priors: &[f32],
        indices: &[u16],
        gates_q15: &[i16],
    ) -> DecisionHash {
        // Hash inputs (features + priors)
        let mut input_bytes = Vec::new();
        for &f in features {
            input_bytes.extend_from_slice(&f.to_le_bytes());
        }
        for &p in priors {
            input_bytes.extend_from_slice(&p.to_le_bytes());
        }
        let input_hash = B3Hash::hash(&input_bytes);

        // Hash outputs (indices + gates)
        let mut output_bytes = Vec::new();
        for &idx in indices {
            output_bytes.extend_from_slice(&idx.to_le_bytes());
        }
        for &gate in gates_q15 {
            output_bytes.extend_from_slice(&gate.to_le_bytes());
        }
        let output_hash = B3Hash::hash(&output_bytes);

        // Combine both hashes for compact verification
        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(input_hash.as_bytes());
        combined_bytes.extend_from_slice(output_hash.as_bytes());
        let combined_hash = B3Hash::hash(&combined_bytes);

        DecisionHash {
            input_hash: input_hash.to_short_hex(),
            output_hash: output_hash.to_short_hex(),
            combined_hash: combined_hash.to_short_hex(),
            tau: self.tau,
            eps: self.eps,
            k: self.k,
        }
    }

    /// Compute per-adapter orthogonality penalty
    ///
    /// This computes how much penalty each adapter should receive based on similarity
    /// to recent selections in the activation history.
    fn compute_adapter_orthogonal_penalty(&self, adapter_idx: usize) -> f32 {
        if !self.orthogonal_enabled {
            return 0.0;
        }

        if let Some(ref constraints) = self.orthogonal_constraints {
            let penalty = constraints.compute_adapter_penalty(adapter_idx);
            penalty * self.feature_weights.orthogonal_weight
        } else {
            0.0
        }
    }

    /// Score and select top-K adapters with per-adapter feature scoring and orthogonality penalties
    ///
    /// This is the corrected routing method that:
    /// 1. Computes per-adapter feature scores (different for each adapter)
    /// 2. Applies orthogonality penalties BEFORE selection
    /// 3. Ensures diversity controls actually affect selection
    ///
    /// # Arguments
    /// * `features` - Global feature vector from prompt/context
    /// * `priors` - Prior scores for each adapter
    /// * `adapter_info` - Metadata for each adapter
    ///
    /// # Returns
    /// Result containing a Decision with selected adapter indices and Q15 gates
    pub fn route_with_adapter_info(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
    ) -> Result<Decision> {
        self.route_with_adapter_info_with_ctx(features, priors, adapter_info, policy_mask, None)
    }

    /// Route with an explicit determinism context for deterministic tie-breaking.
    pub fn route_with_adapter_info_with_ctx(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        self.route_with_adapter_info_and_scope_with_ctx(
            features,
            priors,
            adapter_info,
            policy_mask,
            None,
            determinism,
        )
    }

    /// Scope-aware routing that filters adapters by scope_path when provided.
    /// If no adapters match the scope hint, routing falls back to the original priors.
    pub fn route_with_adapter_info_and_scope(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        scope_hint: Option<&str>,
    ) -> Result<Decision> {
        self.route_with_adapter_info_and_scope_with_ctx(
            features,
            priors,
            adapter_info,
            policy_mask,
            scope_hint,
            None,
        )
    }

    /// Scope-aware routing that filters adapters by scope_path when provided with a determinism context.
    pub fn route_with_adapter_info_and_scope_with_ctx(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
        policy_mask: &PolicyMask,
        scope_hint: Option<&str>,
        determinism: Option<&DeterminismContext>,
    ) -> Result<Decision> {
        let mut filtered_priors: Option<Vec<f32>> = None;
        if let Some(hint) = scope_hint {
            let mut priors_copy = priors.to_vec();
            let mut matched = false;
            for (prior, info) in priors_copy.iter_mut().zip(adapter_info.iter()) {
                if info.scope_path.as_deref() == Some(hint) {
                    matched = true;
                } else {
                    *prior = f32::NEG_INFINITY;
                }
            }
            if matched {
                filtered_priors = Some(priors_copy);
            }
        }

        let priors_for_routing: &[f32] = filtered_priors.as_deref().unwrap_or(priors);

        if self.adaptive_routing && determinism.is_none() {
            return Err(adapteros_core::AosError::Config(
                "Adaptive routing configuration error: determinism context is required for seeded tie-breaking when adaptive routing is enabled. Provide a determinism context in the request or disable adaptive routing."
                    .to_string(),
            ));
        }

        if priors_for_routing.len() != adapter_info.len() {
            tracing::error!(
                "Priors length ({}) != adapter_info length ({}); denying routing decision",
                priors_for_routing.len(),
                adapter_info.len()
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }

        if policy_mask.allowed.len() != adapter_info.len() {
            tracing::error!(
                "Policy mask length ({}) != adapter_info length ({}); denying routing decision",
                policy_mask.allowed.len(),
                adapter_info.len()
            );
            return Ok(Self::empty_decision_with_mask(policy_mask));
        }

        // Compute scores for each adapter with per-adapter feature scoring and penalties
        let mut scores: Vec<(usize, f32)> = priors_for_routing
            .iter()
            .enumerate()
            .filter(|(i, _)| *policy_mask.allowed.get(*i).unwrap_or(&false))
            .map(|(i, &prior)| {
                // Compute adapter-specific feature score (DIFFERENT for each adapter)
                let adapter_feature_score =
                    self.compute_adapter_feature_score(features, &adapter_info[i]);

                // Compute orthogonality penalty (if enabled)
                let orthogonal_penalty = self.compute_adapter_orthogonal_penalty(i);

                // Combine: prior + features - penalty
                let score = prior + adapter_feature_score - orthogonal_penalty;

                (i, score)
            })
            .collect();

        // Prepare adaptive tie-breakers when adaptive routing is enabled
        let tie_breakers: Vec<u64> = if self.adaptive_routing {
            let seed = determinism
                .expect("determinism context required for adaptive routing")
                .router_tiebreak_seed();
            let mut rng = ChaCha20Rng::from_seed(seed);
            (0..adapter_info.len()).map(|_| rng.gen()).collect()
        } else {
            Vec::new()
        };

        // Sort by score descending, then by deterministic/adaptive tie-break
        //
        // # Tie-Breaking Strategy (Critical for Determinism)
        //
        // 1. **Primary**: Score descending (highest score first)
        // 2. **Secondary**: Either adaptive (seeded RNG) or index-based (deterministic)
        //
        // When `adaptive_routing` is enabled AND scores are within a relative epsilon,
        // we use seeded ChaCha20 RNG for tie-breaking. This allows controlled randomization
        // while maintaining reproducibility via `router_tiebreak_seed`.
        //
        // When disabled, we use adapter index ascending (a.0.cmp(&b.0)) for pure determinism.
        //
        // # Relative Epsilon for Tie Detection
        // We use a relative epsilon (1e-6 * max(|a|, |b|)) plus a small absolute floor
        // (f32::EPSILON) to handle near-zero scores. This catches practical ties caused
        // by floating-point drift that `partial_cmp` would otherwise order arbitrarily.
        // Note: We check the epsilon BEFORE falling back to partial_cmp, so that
        // near-equal values are treated as ties even if not bit-identical.
        const RELATIVE_EPSILON: f32 = 1e-6;
        scores.sort_by(|a, b| {
            let diff = b.1 - a.1; // Positive when b > a (descending order)
            let max_abs = a.1.abs().max(b.1.abs());
            let tie_threshold = max_abs * RELATIVE_EPSILON + f32::EPSILON;

            if diff.abs() <= tie_threshold {
                // Scores are within tolerance - treat as tie, use tie-breaker
                if self.adaptive_routing {
                    tie_breakers
                        .get(b.0)
                        .unwrap_or(&0)
                        .cmp(tie_breakers.get(a.0).unwrap_or(&0))
                } else {
                    a.0.cmp(&b.0)
                }
            } else if diff.is_nan() {
                // Handle NaN: treat as equal, fall back to index
                a.0.cmp(&b.0)
            } else if diff > 0.0 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        });

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Apply softmax with temperature
        // Softmax with temperature using deterministic f64 + Kahan path
        let mut gates: Vec<f32> = Self::deterministic_softmax(&top_k, self.tau);

        // Apply entropy floor
        let min_gate = self.eps / self.k as f32;
        for g in &mut gates {
            *g = g.max(min_gate);
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        // Quantize to Q15 so identical inputs keep the same per-token gates across runs
        let gates_q15: SmallVec<[i16; 8]> = gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        let entropy = Self::compute_entropy(&gates);

        // Check abstain conditions and emit telemetry if triggered
        self.check_abstain_conditions(entropy, &gates);

        let candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates_q15.iter())
            .map(|((adapter_idx, raw_score), &gate_q15)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15,
            })
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        // Update activation history (orthogonality tracking)
        if self.orthogonal_enabled {
            if let Some(ref mut constraints) = self.orthogonal_constraints {
                constraints.update_history(&indices, &gates_q15);
            }
        }

        // Compute decision hash if enabled
        let decision_hash = if self.determinism_config.enable_decision_hashing {
            let feature_vec: Vec<f32> = features.to_vec();
            Some(self.compute_decision_hash(&feature_vec, priors_for_routing, &indices, &gates_q15))
        } else {
            None
        };

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash,
            policy_mask_digest: Some(policy_mask.digest),
            policy_overrides_applied: Some(policy_mask.overrides_applied.clone()),
        };

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        Ok(decision)
    }

    fn empty_decision_with_mask(policy_mask: &PolicyMask) -> Decision {
        Decision {
            indices: SmallVec::new(),
            gates_q15: SmallVec::new(),
            entropy: 0.0,
            candidates: Vec::new(),
            decision_hash: None,
            policy_mask_digest: Some(policy_mask.digest),
            policy_overrides_applied: Some(policy_mask.overrides_applied.clone()),
        }
    }

    /// Route with k0 detection (no adapters qualify)
    ///
    /// # DEPRECATED - Use route_with_adapter_info() instead
    ///
    /// This method is deprecated in favor of route_with_adapter_info() which provides
    /// better control over adapter selection and k0 detection through proper per-adapter scoring.
    ///
    /// See [ROUTER_MIGRATION.md](../../docs/ROUTER_MIGRATION.md) for migration steps.
    #[deprecated(
        since = "0.1.1",
        note = "Use route_with_adapter_info() for proper k0 detection"
    )]
    pub fn route_with_k0_detection(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        tracing::warn!(
            "Router::route_with_k0_detection() is deprecated, use route_with_adapter_info() instead. \
             See docs/ROUTER_MIGRATION.md for migration guide"
        );
        if priors.is_empty() {
            // Log k0 event
            let _ = self.log_k0_event("no_adapters_available", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest: None,
                policy_overrides_applied: None,
            };
        }

        // Compute weighted feature score once
        let feature_score = self.compute_weighted_score(features);

        // Compute scores for each adapter combining prior and features
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
            .map(|(i, &prior)| {
                let score = prior + feature_score;
                (i, score)
            })
            .collect();

        // Check if any adapter qualifies (score > threshold)
        let qualifying_count = scores.iter().filter(|(_, score)| *score > 0.1).count();

        if qualifying_count == 0 {
            // Log k0 event
            let _ = self.log_k0_event("no_adapters_qualify", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
                decision_hash: None,
                policy_mask_digest: None,
                policy_overrides_applied: None,
            };
        }

        // Sort by score descending, then by index for determinism
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Take top K
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(self.k).collect();

        // Softmax with temperature
        let max_score = top_k
            .iter()
            .map(|(_, s)| s)
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let exp_scores: Vec<f32> = top_k
            .iter()
            .map(|(_, s)| ((s - max_score) / self.tau).exp())
            .collect();
        let sum_exp: f32 = exp_scores.iter().sum();

        // Normalize and apply entropy floor
        let mut gates: Vec<f32> = exp_scores.iter().map(|e| e / sum_exp).collect();
        let min_gate = self.eps / self.k as f32;
        for g in &mut gates {
            *g = g.max(min_gate);
        }

        // Renormalize
        let sum_gates: f32 = gates.iter().sum();
        for g in &mut gates {
            *g /= sum_gates;
        }

        // Quantize to Q15
        let gates_q15: SmallVec<[i16; 8]> = gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        let entropy = Self::compute_entropy(&gates);

        let candidate_entries: Vec<DecisionCandidate> = top_k
            .iter()
            .zip(gates_q15.iter())
            .map(|((adapter_idx, raw_score), &gate_q15)| DecisionCandidate {
                adapter_idx: *adapter_idx as u16,
                raw_score: *raw_score,
                gate_q15,
            })
            .collect();

        let indices: SmallVec<[u16; 8]> = candidate_entries
            .iter()
            .map(|candidate| candidate.adapter_idx)
            .collect();

        let decision = Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
            decision_hash: None, // Deprecated method doesn't use decision hashing
            policy_mask_digest: None,
            policy_overrides_applied: None,
        };

        // Emit telemetry event (non-blocking)
        self.emit_decision_event(&decision, None);

        decision
    }

    /// Log k0 event when no adapters are selected
    fn log_k0_event(&mut self, reason: &str, input: &[f32]) -> Result<()> {
        use adapteros_core::B3Hash;

        // Convert f32 slice to bytes for hashing
        let input_bytes: Vec<u8> = input.iter().flat_map(|&f| f.to_le_bytes()).collect();

        let input_hash = B3Hash::hash(&input_bytes);

        tracing::warn!(
            "Router k0 event: {} (input_hash: {})",
            reason,
            input_hash.to_short_hex()
        );

        Ok(())
    }

    /// Route using code features (convenience method)
    pub fn route_with_code_features(
        &mut self,
        code_features: &CodeFeatures,
        adapter_info: &[AdapterInfo],
    ) -> Result<Decision> {
        // Filter adapters by active stack first
        let allowed_indices = self.filter_by_stack(adapter_info);

        // Generate priors for each adapter based on code features
        let mut priors: Vec<f32> = vec![1.0; adapter_info.len()];

        // Apply framework priors
        for (i, adapter) in adapter_info.iter().enumerate() {
            // Skip if not in active stack
            if !allowed_indices.contains(&i) {
                priors[i] = 0.0; // Zero prior for excluded adapters
                continue;
            }

            if let Some(framework) = &adapter.framework {
                if let Some(&boost) = code_features.framework_prior.get(framework) {
                    priors[i] += boost * 0.5; // Scale framework boost
                }
            }
        }

        // Apply language priors
        let lang_idx = code_features
            .lang_one_hot
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx);

        if let Some(lang_idx) = lang_idx {
            for (i, adapter) in adapter_info.iter().enumerate() {
                // Skip if not in active stack
                if !allowed_indices.contains(&i) {
                    continue;
                }

                if adapter.supports_language(lang_idx) {
                    priors[i] += 0.3;
                }
            }
        }

        // Log stack filtering info
        if self.active_stack_name.is_some() {
            tracing::debug!(
                "Stack filtering: {} of {} adapters eligible",
                allowed_indices.len(),
                adapter_info.len()
            );
        }

        // Convert code features to vector
        let feature_vec = code_features.to_vector();

        // Route using the computed priors with adapter info
        let mask = PolicyMask::allow_all(
            &adapter_info
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<_>>(),
            None,
        );
        self.route_with_adapter_info(&feature_vec, &priors, adapter_info, &mask)
    }

    /// Get scoring explanation for debugging/audit
    pub fn explain_score(&self, features: &[f32]) -> ScoringExplanation {
        // Accept both 21 (without entropy) and 22 (with entropy) dimensions
        if features.len() != 21 && features.len() != 22 {
            return ScoringExplanation {
                language_score: 0.0,
                framework_score: 0.0,
                symbol_hits_score: 0.0,
                path_tokens_score: 0.0,
                prompt_verb_score: 0.0,
                total_score: features.iter().sum::<f32>() * 0.1,
            };
        }

        let lang_strength = features[0..8].iter().fold(0.0f32, |a, &b| a.max(b));
        let framework_strength = features[8..11].iter().sum::<f32>();
        let symbol_hits = features[11];
        let path_tokens = features[12];
        let verb_strength = features[13..21].iter().fold(0.0f32, |a, &b| a.max(b));

        let language_score = lang_strength * self.feature_weights.language_weight;
        let framework_score = framework_strength * self.feature_weights.framework_weight;
        let symbol_hits_score = symbol_hits * self.feature_weights.symbol_hits_weight;
        let path_tokens_score = path_tokens * self.feature_weights.path_tokens_weight;
        let prompt_verb_score = verb_strength * self.feature_weights.prompt_verb_weight;

        ScoringExplanation {
            language_score,
            framework_score,
            symbol_hits_score,
            path_tokens_score,
            prompt_verb_score,
            total_score: language_score
                + framework_score
                + symbol_hits_score
                + path_tokens_score
                + prompt_verb_score,
        }
    }
}

/// Scoring explanation for debugging and audit
#[derive(Debug, Clone)]
pub struct ScoringExplanation {
    pub language_score: f32,
    pub framework_score: f32,
    pub symbol_hits_score: f32,
    pub path_tokens_score: f32,
    pub prompt_verb_score: f32,
    pub total_score: f32,
}

impl ScoringExplanation {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Scoring Breakdown:\n\
             - Language:     {:.3} (weight: 0.30)\n\
             - Framework:    {:.3} (weight: 0.25)\n\
             - Symbol Hits:  {:.3} (weight: 0.20)\n\
             - Path Tokens:  {:.3} (weight: 0.15)\n\
             - Prompt Verb:  {:.3} (weight: 0.10)\n\
             = Total Score:  {:.3}",
            self.language_score,
            self.framework_score,
            self.symbol_hits_score,
            self.path_tokens_score,
            self.prompt_verb_score,
            self.total_score,
        )
    }
}

/// Adapter information for routing
#[derive(Debug, Clone, Default)]
pub struct AdapterInfo {
    pub id: String,
    pub framework: Option<String>,
    pub languages: Vec<usize>, // Language indices
    pub tier: String,
    pub scope_path: Option<String>,
    pub lora_tier: Option<String>,
    pub base_model: Option<String>,
}

impl AdapterInfo {
    /// Check if adapter supports a language
    pub fn supports_language(&self, lang_idx: usize) -> bool {
        self.languages.contains(&lang_idx)
    }
}

/// Candidate adapter selected by the router with raw score and gate
#[derive(Debug, Clone)]
pub struct DecisionCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Router decision with indices and quantized gates
#[derive(Debug, Clone)]
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,
    pub gates_q15: SmallVec<[i16; 8]>,
    pub entropy: f32,
    pub candidates: Vec<DecisionCandidate>,
    /// Optional decision hash for audit and reproducibility verification
    pub decision_hash: Option<DecisionHash>,
    /// Digest binding routing policy context to the applied mask (if any).
    pub policy_mask_digest: Option<B3Hash>,
    /// Flags indicating which policy overrides were applied for this decision.
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
}

impl Decision {
    /// Convert Q15 gates back to float
    pub fn gates_f32(&self) -> Vec<f32> {
        self.gates_q15
            .iter()
            .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
            .collect()
    }

    /// Convert to canonical RouterRing for kernel execution
    pub fn to_router_ring(&self) -> adapteros_lora_kernel_api::RouterRing {
        let k = self.indices.len();
        assert!(k <= 8, "Decision has too many adapters (k={}), max is 8", k);

        let mut ring = adapteros_lora_kernel_api::RouterRing::new(k);
        ring.set(&self.indices[..], &self.gates_q15[..]);
        ring
    }
}

/// Convert Decision to canonical RouterRing for kernel interface
impl From<Decision> for adapteros_lora_kernel_api::RouterRing {
    fn from(decision: Decision) -> Self {
        decision.to_router_ring()
    }
}

/// Convert Decision reference to canonical RouterRing
impl From<&Decision> for adapteros_lora_kernel_api::RouterRing {
    fn from(decision: &Decision) -> Self {
        decision.to_router_ring()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::determinism::DeterminismSource;
    use adapteros_core::seed::{derive_seed_full, hash_adapter_dir};
    use std::path::Path;

    fn mask_all(adapters: &[AdapterInfo]) -> PolicyMask {
        let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
        PolicyMask::allow_all(&ids, None)
    }

    fn pivot_from_seed(seed: [u8; 32], len: usize) -> usize {
        let prefix = u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]]);
        (prefix as usize) % len.max(1)
    }

    fn seeded_priors(seed: [u8; 32], len: usize) -> Vec<f32> {
        let pivot = pivot_from_seed(seed, len);
        let mut priors = Vec::with_capacity(len);

        for i in 0..len {
            let byte = seed[(i * 3) % seed.len()] as f32;
            priors.push(0.05 + byte / 255.0);
        }

        if len > 0 {
            priors[pivot] += 1.0; // Strong, deterministic bias driven by seed
        }

        priors
    }

    #[test]
    fn test_router_topk() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        let features = vec![0.5; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.6, 0.0];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        assert_eq!(decision.indices.len(), 3);
        assert_eq!(decision.gates_q15.len(), 3);

        // Gates should sum to approximately 1.0
        let sum: f32 = decision.gates_f32().iter().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_entropy_floor() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.1);

        let features = vec![0.0; 5];
        let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let gates = decision.gates_f32();

        // All gates should be >= entropy floor / k
        let min_gate = 0.1 / 3.0;
        for &g in &gates {
            assert!(g >= min_gate - 0.001);
        }
    }

    #[test]
    fn test_route_with_code_features() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        let code_features = CodeFeatures::from_context("Fix this python bug in django app");

        let adapters = vec![
            AdapterInfo {
                id: "python-general".to_string(),
                framework: None,
                languages: vec![0], // Python index
                tier: "persistent".to_string(),
                ..Default::default()
            },
            AdapterInfo {
                id: "django-specific".to_string(),
                framework: Some("django".to_string()),
                languages: vec![0], // Python index
                tier: "persistent".to_string(),
                ..Default::default()
            },
            AdapterInfo {
                id: "rust-general".to_string(),
                framework: None,
                languages: vec![1], // Rust index
                tier: "persistent".to_string(),
                ..Default::default()
            },
        ];

        let decision = router
            .route_with_code_features(&code_features, &adapters)
            .expect("router decision");

        assert_eq!(decision.indices.len(), 3);

        // Django adapter should likely be selected due to framework prior
        // (though exact ordering depends on weights)
        tracing::debug!("Selected indices: {:?}", decision.indices);
        tracing::debug!("Gates: {:?}", decision.gates_f32());
    }

    #[test]
    fn test_weighted_scoring_influences_selection() {
        // Create two different weight configurations
        let heavy_language_weights = RouterWeights::new(
            0.8,  // language - very high
            0.05, // framework
            0.05, // symbols
            0.05, // paths
            0.05, // verb
        );

        let heavy_framework_weights = RouterWeights::new(
            0.05, // language
            0.8,  // framework - very high
            0.05, // symbols
            0.05, // paths
            0.05, // verb
        );

        let router1 = Router::new_with_weights(heavy_language_weights, 3, 1.0, 0.02);
        let router2 = Router::new_with_weights(heavy_framework_weights, 3, 1.0, 0.02);

        // Create code features with strong Python + Django signal
        let features = CodeFeatures::from_context("def main(): # Python Django application");
        let feature_vec = features.to_vector();

        // Get scoring explanations
        let explanation1 = router1.explain_score(&feature_vec);
        let explanation2 = router2.explain_score(&feature_vec);

        // With heavy language weights, language score should dominate
        assert!(
            explanation1.language_score > explanation1.framework_score,
            "Language-weighted router should prioritize language score"
        );

        tracing::debug!("Heavy language weights: {}", explanation1.format());
        tracing::debug!("Heavy framework weights: {}", explanation2.format());

        // Language contribution should be much higher in router1
        assert!(
            explanation1.language_score > explanation2.language_score * 2.0,
            "Language score should be much higher with language-heavy weights: {} vs {}",
            explanation1.language_score,
            explanation2.language_score
        );

        // Framework contribution should be much higher in router2
        assert!(
            explanation2.framework_score > explanation1.framework_score * 2.0,
            "Framework score should be much higher with framework-heavy weights: {} vs {}",
            explanation2.framework_score,
            explanation1.framework_score
        );
    }

    #[test]
    fn test_feature_score_components() {
        let weights = RouterWeights::default();
        let router = Router::new_with_weights(weights, 3, 1.0, 0.02);

        // Create features with known components
        let features = CodeFeatures::from_context("Fix the bug in this Python function");
        let feature_vec = features.to_vector();

        let explanation = router.explain_score(&feature_vec);

        // All scores should be non-negative
        assert!(explanation.language_score >= 0.0);
        assert!(explanation.framework_score >= 0.0);
        assert!(explanation.symbol_hits_score >= 0.0);
        assert!(explanation.path_tokens_score >= 0.0);
        assert!(explanation.prompt_verb_score >= 0.0);

        // Total should equal sum of components
        let sum = explanation.language_score
            + explanation.framework_score
            + explanation.symbol_hits_score
            + explanation.path_tokens_score
            + explanation.prompt_verb_score;

        assert!(
            (sum - explanation.total_score).abs() < 0.001,
            "Total score should equal sum of components"
        );
    }

    #[test]
    fn test_default_weights_sum_to_one() {
        let weights = RouterWeights::default();
        let total = weights.total_weight();

        assert!(
            (total - 1.0).abs() < 0.001,
            "Default weights should sum to 1.0, got {}",
            total
        );
    }

    #[test]
    fn test_router_with_policy_config_entropy_floor() {
        // Create a policy config with custom entropy floor
        let policy_config = adapteros_policy::packs::router::RouterConfig {
            entropy_floor: 0.05, // Custom entropy floor
            ..Default::default()
        };

        let router =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

        // Verify that the entropy floor is read from policy config
        assert_eq!(
            router.entropy_floor(),
            0.05,
            "Entropy floor should match policy config"
        );
    }

    #[test]
    fn test_router_with_policy_config_sample_tokens() {
        // Create a policy config with custom sample tokens
        let policy_config = adapteros_policy::packs::router::RouterConfig {
            sample_tokens_full: 256, // Custom sample tokens
            ..Default::default()
        };

        let router =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

        // Verify that sample tokens is read from policy config
        assert_eq!(
            router.full_log_tokens, 256,
            "Full log tokens should match policy config"
        );
    }

    #[test]
    fn test_router_with_policy_config_k_sparse_clamping() {
        // Create a policy config with k_sparse limit
        let policy_config = adapteros_policy::packs::router::RouterConfig {
            k_sparse: 4, // Limit K to 4
            ..Default::default()
        };

        // Try to create router with k=6 (exceeds policy limit)
        let router =
            Router::new_with_policy_config(RouterWeights::default(), 6, 1.0, &policy_config);

        // Verify that k is clamped to policy maximum
        assert_eq!(router.k, 4, "K should be clamped to policy maximum");
    }

    #[test]
    fn test_entropy_floor_enforcement_with_policy_config() {
        let policy_config = adapteros_policy::packs::router::RouterConfig {
            entropy_floor: 0.15, // Higher entropy floor
            ..Default::default()
        };

        let mut router =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

        let features = vec![0.0; 5];
        let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let gates = decision.gates_f32();

        // All gates should be >= entropy floor / k
        let min_gate = 0.15 / 3.0;
        for &g in &gates {
            assert!(
                g >= min_gate - 0.001,
                "Gate {} should be >= minimum {} required by policy",
                g,
                min_gate
            );
        }
    }

    #[test]
    fn test_policy_config_different_entropy_floors() {
        // Create two routers with different policy configs
        let mut policy_low = adapteros_policy::packs::router::RouterConfig::default();
        policy_low.entropy_floor = 0.01;

        let mut policy_high = adapteros_policy::packs::router::RouterConfig::default();
        policy_high.entropy_floor = 0.20;

        let mut router_low =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_low);

        let mut router_high =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_high);

        // Same input
        let features = vec![0.1; 5];
        let priors = vec![0.9, 0.05, 0.03, 0.02, 0.0];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision_low = router_low
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let decision_high = router_high
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        let gates_low = decision_low.gates_f32();
        let gates_high = decision_high.gates_f32();

        // Minimum gate in high entropy floor config should be larger
        let actual_min_low = gates_low.iter().fold(f32::MAX, |a, &b| a.min(b));
        let actual_min_high = gates_high.iter().fold(f32::MAX, |a, &b| a.min(b));

        assert!(
            actual_min_high >= actual_min_low - 0.001,
            "Higher entropy floor should result in higher minimum gate: {} vs {}",
            actual_min_high,
            actual_min_low
        );
    }

    #[test]
    fn test_deterministic_softmax_reproducibility() {
        // Test that deterministic softmax produces identical results across multiple runs
        let scores = vec![(0, 0.9f32), (1, 0.5f32), (2, 0.3f32), (3, 0.1f32)];
        let tau = 1.0f32;

        // Run deterministic softmax multiple times
        let result1 = Router::deterministic_softmax(&scores, tau);
        let result2 = Router::deterministic_softmax(&scores, tau);
        let result3 = Router::deterministic_softmax(&scores, tau);

        // All results should be identical
        assert_eq!(result1.len(), result2.len());
        assert_eq!(result2.len(), result3.len());

        for i in 0..result1.len() {
            assert_eq!(
                result1[i], result2[i],
                "Deterministic softmax should produce identical results (run 1 vs 2)"
            );
            assert_eq!(
                result2[i], result3[i],
                "Deterministic softmax should produce identical results (run 2 vs 3)"
            );
        }

        // Results should sum to approximately 1.0
        let sum: f32 = result1.iter().sum();
        assert!((sum - 1.0).abs() < 0.0001, "Softmax should sum to 1.0");
    }

    #[test]
    fn test_route_gates_follow_deterministic_softmax_path() {
        let priors = vec![0.3f32, 0.2f32, 0.1f32];
        let features = vec![0.0f32; 22];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mut router =
            Router::new_with_weights(RouterWeights::default(), priors.len(), 1.0, 0.01);
        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        // Recreate the router's top-k ordering
        let mut scores: Vec<(usize, f32)> =
            priors.iter().enumerate().map(|(i, &p)| (i, p)).collect();
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        let top_k: Vec<(usize, f32)> = scores.into_iter().take(priors.len()).collect();

        // Compute expected gates using deterministic softmax + entropy floor + renorm
        let mut expected_gates = Router::deterministic_softmax(&top_k, 1.0);
        let eps = 0.01;
        let min_gate = eps / priors.len() as f32;
        for g in expected_gates.iter_mut() {
            *g = g.max(min_gate);
        }
        let sum_expected: f32 = expected_gates.iter().sum();
        for g in expected_gates.iter_mut() {
            *g /= sum_expected;
        }
        let expected_q15: Vec<i16> = expected_gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        assert_eq!(
            decision.gates_q15.as_slice(),
            expected_q15.as_slice(),
            "Route should use deterministic f64 softmax prior to Q15 quantization"
        );
    }

    #[test]
    fn test_seeded_routing_reproducible_for_same_inputs() {
        let global = B3Hash::hash(b"global-router-seed");
        let manifest = B3Hash::hash(b"manifest-router-a");
        let adapter_dir_hash = hash_adapter_dir(Path::new("/adapters/seeded/a"));

        // Same seed context should yield identical priors and routing outcomes
        let seed = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 0);
        let priors = seeded_priors(seed, 4);

        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let features = vec![0.0f32; 3];

        let mut router_run1 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
        let mut router_run2 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);

        let mask = mask_all(&adapter_info);
        let decision1 = router_run1
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let decision2 = router_run2
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        assert_eq!(
            decision1.indices, decision2.indices,
            "Same seed-derived priors must yield identical adapter choices"
        );
        assert_eq!(
            decision1.gates_q15, decision2.gates_q15,
            "Same seed-derived priors must yield identical quantized gates"
        );
    }

    #[test]
    fn test_seed_changes_can_shift_routing() {
        let global = B3Hash::hash(b"global-router-seed");
        let manifest = B3Hash::hash(b"manifest-router-a");
        let adapter_dir_hash = hash_adapter_dir(Path::new("/adapters/seeded/a"));
        let adapter_count = 4usize;

        // Different nonces give us different seeds; fall back to a third if the pivot collides
        let seed_a = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 1);
        let mut seed_b = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 2);
        let pivot_a = pivot_from_seed(seed_a, adapter_count);
        let mut pivot_b = pivot_from_seed(seed_b, adapter_count);
        if pivot_a == pivot_b {
            seed_b = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 3);
            pivot_b = pivot_from_seed(seed_b, adapter_count);
        }
        assert_ne!(
            pivot_a, pivot_b,
            "Different seed contexts should map to different adapter pivots"
        );

        let priors_a = seeded_priors(seed_a, adapter_count);
        let priors_b = seeded_priors(seed_b, adapter_count);
        assert_ne!(
            priors_a, priors_b,
            "Different seeds must produce different priors for routing"
        );

        let adapter_info: Vec<AdapterInfo> = (0..adapter_count)
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();
        let features = vec![0.0f32; 3];

        let mut router_a = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
        let mut router_b = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);

        let mask = mask_all(&adapter_info);
        let decision_a = router_a
            .route_with_adapter_info(&features, &priors_a, &adapter_info, &mask)
            .expect("router decision");
        let decision_b = router_b
            .route_with_adapter_info(&features, &priors_b, &adapter_info, &mask)
            .expect("router decision");

        assert!(
            decision_a.indices != decision_b.indices
                || decision_a.gates_q15 != decision_b.gates_q15,
            "Different seed contexts should change routing choices or gates when priors differ"
        );
    }

    #[test]
    fn adaptive_routing_uses_context_for_tie_breakers() {
        let mut router_a = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
        let mut router_b = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
        router_a.set_routing_determinism_mode(true);
        router_b.set_routing_determinism_mode(true);

        let determinism_ctx = DeterminismContext::new(
            [1u8; 32],
            None,
            adapteros_core::SeedMode::BestEffort,
            adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
            DeterminismSource::DerivedFromRequest,
        );

        let features = vec![0.0f32; 4];
        let priors = vec![0.5f32; 4];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let policy_mask = mask_all(&adapter_info);

        let decision_a = router_a
            .route_with_adapter_info_with_ctx(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
                Some(&determinism_ctx),
            )
            .expect("router decision");
        let decision_b = router_b
            .route_with_adapter_info_with_ctx(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
                Some(&determinism_ctx),
            )
            .expect("router decision");

        assert_eq!(
            decision_a.indices, decision_b.indices,
            "Adaptive routing should be deterministic when provided a determinism context"
        );
        assert_eq!(
            decision_a.gates_q15, decision_b.gates_q15,
            "Gates should also remain deterministic under the same tie-break seed"
        );
    }

    #[test]
    fn test_decision_hash_computation() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        // Enable decision hashing
        let mut config = RouterDeterminismConfig::default();
        config.enable_decision_hashing = true;
        router.set_determinism_config(config);

        let features = vec![0.5f32; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        // Decision should have a hash
        assert!(
            decision.decision_hash.is_some(),
            "Decision should have hash when hashing is enabled"
        );

        let hash = decision.decision_hash.unwrap();

        // Hash should have all fields populated
        assert!(
            !hash.input_hash.is_empty(),
            "Input hash should be populated"
        );
        assert!(
            !hash.output_hash.is_empty(),
            "Output hash should be populated"
        );
        assert!(
            !hash.combined_hash.is_empty(),
            "Combined hash should be populated"
        );
        assert_eq!(hash.tau, 1.0, "Tau should match router config");
        assert_eq!(hash.eps, 0.02, "Eps should match router config");
        assert_eq!(hash.k, 3, "K should match router config");
    }

    #[test]
    fn test_decision_hash_reproducibility() {
        // Test that identical inputs produce identical hashes
        let mut router1 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
        let mut router2 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        // Enable decision hashing on both
        let config = RouterDeterminismConfig::default();
        router1.set_determinism_config(config.clone());
        router2.set_determinism_config(config);

        let features = vec![0.5f32; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision1 = router1
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let decision2 = router2
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        // Both decisions should have hashes
        assert!(decision1.decision_hash.is_some());
        assert!(decision2.decision_hash.is_some());

        let hash1 = decision1.decision_hash.unwrap();
        let hash2 = decision2.decision_hash.unwrap();

        // Hashes should be identical for identical inputs
        assert_eq!(
            hash1.input_hash, hash2.input_hash,
            "Input hashes should match for identical inputs"
        );
        assert_eq!(
            hash1.output_hash, hash2.output_hash,
            "Output hashes should match for deterministic routing"
        );
        assert_eq!(
            hash1.combined_hash, hash2.combined_hash,
            "Combined hashes should match"
        );
    }

    #[test]
    fn test_ieee754_deterministic_flag() {
        // Test that the IEEE 754 deterministic flag is respected
        let mut router_deterministic =
            Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
        let mut router_standard = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        // Enable deterministic mode for first router
        let mut config_det = RouterDeterminismConfig::default();
        config_det.ieee754_deterministic = true;
        router_deterministic.set_determinism_config(config_det);

        // Disable deterministic mode for second router
        let mut config_std = RouterDeterminismConfig::default();
        config_std.ieee754_deterministic = false;
        router_standard.set_determinism_config(config_std);

        let features = vec![0.5f32; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision_det = router_deterministic
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");
        let decision_std = router_standard
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        // Both should produce valid decisions
        assert_eq!(decision_det.indices.len(), 3);
        assert_eq!(decision_std.indices.len(), 3);

        // Gates should sum to approximately 1.0 in both cases
        let sum_det: f32 = decision_det.gates_f32().iter().sum();
        let sum_std: f32 = decision_std.gates_f32().iter().sum();
        assert!((sum_det - 1.0).abs() < 0.01);
        assert!((sum_std - 1.0).abs() < 0.01);

        // For these simple inputs, results should be very close (may differ in last bits)
        // We don't assert exact equality since f32 vs f64 paths may differ slightly
    }

    #[test]
    fn test_decision_hash_disabled() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        // Disable decision hashing
        let mut config = RouterDeterminismConfig::default();
        config.enable_decision_hashing = false;
        router.set_determinism_config(config);

        let features = vec![0.5f32; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = mask_all(&adapter_info);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("router decision");

        // Decision should NOT have a hash when disabled
        assert!(
            decision.decision_hash.is_none(),
            "Decision should not have hash when hashing is disabled"
        );
    }

    #[test]
    fn test_abstain_thresholds_from_policy_config() {
        // Create a policy config with abstain thresholds
        let mut policy_config = adapteros_policy::packs::router::RouterConfig::default();
        policy_config.abstain_entropy_threshold = Some(0.9);
        policy_config.abstain_confidence_threshold = Some(0.3);

        let router =
            Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

        // Verify that thresholds are set from policy config
        assert_eq!(
            router.abstain_entropy_threshold,
            Some(0.9),
            "Entropy threshold should match policy config"
        );
        assert_eq!(
            router.abstain_confidence_threshold,
            Some(0.3),
            "Confidence threshold should match policy config"
        );
    }

    #[test]
    fn test_set_abstain_thresholds() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        // Initially no thresholds
        assert!(router.abstain_entropy_threshold.is_none());
        assert!(router.abstain_confidence_threshold.is_none());

        // Set thresholds
        router.set_abstain_thresholds(Some(0.85), Some(0.25));

        assert_eq!(router.abstain_entropy_threshold, Some(0.85));
        assert_eq!(router.abstain_confidence_threshold, Some(0.25));
    }

    #[test]
    fn test_scope_hint_filters_non_matching_adapters() {
        let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
        let features = vec![0.0; 4];
        let priors = vec![0.8, 0.8];
        let scope_hint = "domain/group/scope/op";

        let adapter_info = vec![
            AdapterInfo {
                id: "scoped".to_string(),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                scope_path: Some(scope_hint.to_string()),
                ..Default::default()
            },
            AdapterInfo {
                id: "other".to_string(),
                framework: None,
                languages: vec![],
                tier: "default".to_string(),
                scope_path: Some("other/scope".to_string()),
                ..Default::default()
            },
        ];

        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
        let decision = router
            .route_with_adapter_info_and_scope(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
                Some(scope_hint),
            )
            .expect("router decision");

        assert_eq!(decision.indices.len(), 1);
        assert_eq!(decision.indices[0], 0);
    }

    // ============================================================================
    // Q15 QUANTIZATION EDGE CASE TESTS
    // ============================================================================

    #[test]
    fn test_q15_constants_validation() {
        // Verify Q15 constants are correct
        assert_eq!(
            ROUTER_GATE_Q15_DENOM, 32767.0,
            "Q15 denominator MUST be 32767.0, not 32768.0"
        );
        assert_eq!(
            ROUTER_GATE_Q15_MAX, 32767,
            "Q15 max value MUST be 32767 (i16::MAX)"
        );
    }

    #[test]
    fn test_q15_zero_gate_edge_case() {
        // Edge case 1: Gate = 0 → Q15 = 0
        let gate_f32 = 0.0f32;
        let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;
        let gate_q15_clamped = gate_q15.max(0);

        assert_eq!(gate_q15, 0, "0.0 gate should convert to Q15 = 0");
        assert_eq!(gate_q15_clamped, 0, "Clamped 0 should remain 0");

        // Verify round-trip
        let recovered = gate_q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;
        assert_eq!(recovered, 0.0, "Q15 = 0 should decode to 0.0");
    }

    #[test]
    fn test_q15_max_gate_edge_case() {
        // Edge case 2: Gate = 1.0 → Q15 = 32767
        let gate_f32 = 1.0f32;
        let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;

        assert_eq!(gate_q15, 32767, "1.0 gate should convert to Q15 = 32767");

        // Verify round-trip
        let recovered = gate_q15 as f32 / ROUTER_GATE_Q15_DENOM;
        assert_eq!(recovered, 1.0, "Q15 = 32767 should decode to exactly 1.0");
    }

    #[test]
    fn test_q15_negative_gate_clamping() {
        // Edge case 3: Negative gates should be clamped to 0
        let negative_gate = -0.5f32;
        let gate_q15_raw = (negative_gate * ROUTER_GATE_Q15_DENOM).round() as i16;
        let gate_q15_clamped = gate_q15_raw.max(0);

        assert!(gate_q15_raw < 0, "Negative gate produces negative Q15");
        assert_eq!(gate_q15_clamped, 0, "Negative Q15 should be clamped to 0");
    }

    #[test]
    fn test_q15_very_small_gates_underflow() {
        // Edge case 4: Very small gates should round to 0 or 1
        let tiny_gates = vec![1e-8, 1e-7, 1e-6, 1e-5, 1e-4];

        for gate in tiny_gates {
            let q = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
            let q_clamped = q.max(0);

            if gate * ROUTER_GATE_Q15_DENOM < 0.5 {
                assert_eq!(
                    q_clamped, 0,
                    "Gate {} should round to Q15 = 0",
                    gate
                );
            } else {
                assert!(
                    q_clamped >= 1,
                    "Gate {} should round to Q15 >= 1",
                    gate
                );
            }
        }
    }

    #[test]
    fn test_q15_sum_normalization() {
        // Edge case 5: Sum of Q15 gates should be ~32767 after normalization
        let normalized_gates = vec![0.25, 0.25, 0.25, 0.25];

        let gates_q15: Vec<i16> = normalized_gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        let sum_q15: i32 = gates_q15.iter().map(|&g| g as i32).sum();

        // Sum should be close to 32767 (within rounding error)
        assert!(
            (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= gates_q15.len() as i32,
            "Sum of Q15 gates ({}) should be within {} of max ({})",
            sum_q15,
            gates_q15.len(),
            ROUTER_GATE_Q15_MAX
        );
    }

    #[test]
    fn test_q15_to_f32_conversion_formula() {
        // Edge case 6: Verify Q15→f32 conversion: gate_q15 / 32767.0
        let test_values = vec![
            (0i16, 0.0f32),
            (1i16, 1.0 / 32767.0),
            (16383i16, 16383.0 / 32767.0),
            (32767i16, 1.0),
        ];

        for (q15, expected_f32) in test_values {
            let converted = q15 as f32 / ROUTER_GATE_Q15_DENOM;
            assert!(
                (converted - expected_f32).abs() < 1e-6,
                "Q15 {} should convert to {}, got {}",
                q15,
                expected_f32,
                converted
            );
        }
    }

    #[test]
    fn test_q15_conversion_determinism() {
        // Edge case 7: Same gates → same Q15 values (determinism)
        let gates = vec![0.2, 0.3, 0.5];

        // Convert 5 times and verify consistency
        let mut results = Vec::new();
        for _ in 0..5 {
            let gates_q15: Vec<i16> = gates
                .iter()
                .map(|&g| {
                    let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                    q.max(0)
                })
                .collect();
            results.push(gates_q15);
        }

        // All results should be identical
        for i in 1..results.len() {
            assert_eq!(
                results[0], results[i],
                "Q15 conversion should be deterministic"
            );
        }
    }

    #[test]
    fn test_q15_round_trip_precision() {
        // Test f32 → Q15 → f32 round-trip precision
        let test_gates = vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

        for original in test_gates {
            let q15 = (original * ROUTER_GATE_Q15_DENOM).round() as i16;
            let q15_clamped = q15.max(0);
            let recovered = q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;

            let max_error = 1.0 / ROUTER_GATE_Q15_DENOM;
            let actual_error = (recovered - original).abs();

            assert!(
                actual_error <= max_error,
                "Round-trip error ({}) exceeds max ({}) for gate {}",
                actual_error,
                max_error,
                original
            );
        }
    }

    #[test]
    fn test_q15_not_using_legacy_32768() {
        // Verify we're NOT using incorrect 32768 denominator
        let gate_max = 1.0f32;

        let q15_correct = (gate_max * 32767.0).round() as i16;
        let q15_incorrect = (gate_max * 32768.0).round() as i16;

        assert_eq!(q15_correct, 32767);
        assert_ne!(q15_correct, q15_incorrect, "32767 and 32768 must differ");

        let recovered_correct = q15_correct as f32 / 32767.0;
        assert_eq!(recovered_correct, 1.0, "32767 denom gives exact 1.0");
    }

    #[test]
    fn test_q15_decision_gates_f32_method() {
        // Test Decision::gates_f32() conversion
        let decision = Decision {
            indices: SmallVec::from_vec(vec![0, 1, 2]),
            gates_q15: SmallVec::from_vec(vec![32767, 16383, 0]),
            entropy: 0.5,
            candidates: vec![],
            decision_hash: None,
            policy_mask_digest: None,
            policy_overrides_applied: None,
        };

        let gates_f32 = decision.gates_f32();

        assert_eq!(gates_f32.len(), 3);
        assert_eq!(gates_f32[0], 1.0);
        assert!((gates_f32[1] - 0.5).abs() < 0.001);
        assert_eq!(gates_f32[2], 0.0);
    }

    #[test]
    fn test_router_q15_gates_sum_correctly() {
        // Integration test: router Q15 gates should sum to ~32767
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
        router.set_routing_determinism_mode(true);

        let features = vec![0.5; 22];
        let priors = vec![1.0, 1.0, 1.0];

        let adapter_info: Vec<AdapterInfo> = (0..3)
            .map(|i| AdapterInfo {
                adapter_id: format!("adapter-{}", i),
                adapter_hash: Some(format!("hash{}", i)),
                framework_tags: vec![],
                language_tags: vec![],
                scope: None,
            })
            .collect();

        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.adapter_id.clone()).collect();
        let mask = PolicyMask::allow_all(&adapter_ids, None);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("routing decision");

        let sum_q15: i32 = decision.gates_q15.iter().map(|&g| g as i32).sum();
        let sum_f32: f32 = decision.gates_f32().iter().sum();

        assert!((sum_f32 - 1.0).abs() < 0.01, "Float gates sum to ~1.0");
        assert!(
            (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= decision.gates_q15.len() as i32,
            "Q15 gates sum to ~32767"
        );
    }

    #[test]
    fn test_router_single_adapter_gets_max_q15() {
        // Single adapter should get gate = 1.0 → Q15 = 32767
        let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
        router.set_routing_determinism_mode(true);

        let features = vec![0.5; 22];
        let priors = vec![1.0];

        let adapter_info = vec![AdapterInfo {
            adapter_id: "adapter-1".to_string(),
            adapter_hash: Some("hash1".to_string()),
            framework_tags: vec![],
            language_tags: vec![],
            scope: None,
        }];

        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.adapter_id.clone()).collect();
        let mask = PolicyMask::allow_all(&adapter_ids, None);
        let decision = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("routing decision");

        assert_eq!(decision.indices.len(), 1);
        assert_eq!(decision.gates_q15[0], 32767);
        assert_eq!(decision.gates_f32()[0], 1.0);
    }

    #[test]
    fn test_router_q15_determinism_identical_inputs() {
        // Multiple routing calls with identical inputs produce identical Q15
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
        router.set_routing_determinism_mode(true);

        let features = vec![0.5; 22];
        let priors = vec![0.6, 0.3, 0.1];

        let adapter_info: Vec<AdapterInfo> = (0..3)
            .map(|i| AdapterInfo {
                adapter_id: format!("adapter-{}", i),
                adapter_hash: Some(format!("hash{}", i)),
                framework_tags: vec![],
                language_tags: vec![],
                scope: None,
            })
            .collect();

        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.adapter_id.clone()).collect();
        let mask = PolicyMask::allow_all(&adapter_ids, None);

        // Make 3 identical routing decisions
        let mut decisions = Vec::new();
        for _ in 0..3 {
            let decision = router
                .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
                .expect("routing decision");
            decisions.push(decision);
        }

        // All decisions should have identical Q15 gates
        for i in 1..decisions.len() {
            assert_eq!(
                decisions[0].gates_q15, decisions[i].gates_q15,
                "Identical inputs should produce identical Q15 gates"
            );
        }
    }
}
