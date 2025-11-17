//! Top-K sparse router with Q15 gate quantization

pub mod calibration;
pub mod code_features;
pub mod features;
pub mod framework_routing;
pub mod metrics;
pub mod orthogonal;
pub mod path_routing;
pub mod scoring;

use adapteros_core::{B3Hash, Result};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;

pub use calibration::{
    CalibrationDataset, CalibrationSample, Calibrator, OptimizationMethod, ValidationMetrics,
};
pub use code_features::{CodeFeatureExtractor, CodeFeatures as CodeFeaturesExt};
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

    // MPLoRA additions
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

    /// Create custom weights with MPLoRA parameters
    pub fn new_with_mplora(
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

    /// Save weights to JSON file
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
        std::fs::write(path.as_ref(), content)
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))
    }
}

pub const MAX_K: usize = 8;

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
    /// Token counter for sampling
    #[allow(dead_code)]
    token_count: usize,
    /// Log first N tokens fully (default: 128 per Telemetry Ruleset #9)
    full_log_tokens: usize,

    // MPLoRA enhancements
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
}

impl Router {
    /// Create a new router with custom feature weights
    pub fn new_with_weights(feature_weights: RouterWeights, k: usize, tau: f32, eps: f32) -> Self {
        Self {
            feature_weights,
            k,
            tau,
            eps,
            token_count: 0,
            full_log_tokens: 128, // Per Telemetry Ruleset #9
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
        }
    }

    /// Create a new router with default weights (for backward compatibility)
    pub fn new(_weights: Vec<f32>, k: usize, tau: f32, eps: f32, _seed: [u8; 32]) -> Result<Self> {
        if k > MAX_K {
            return Err(adapteros_core::AosError::Config(
                "K cannot exceed MAX_K=8".to_string(),
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

    /// Enable orthogonal constraints for MPLoRA
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

    /// Check if an adapter is in the active stack
    #[allow(dead_code)]
    fn is_in_active_stack(&self, adapter_id: &str) -> bool {
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

        score
    }

    /// Score and select top-K adapters
    pub fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision {
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
                let q = (g * 32767.0).round() as i16;
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
                // Note: Penalty-based rescoring deferred to post-alpha (MPLoRA full implementation)
                // See: https://openreview.net/pdf?id=jqz6Msm3AF
                constraints.update_history(&indices, &gates_q15);
            }
        }

        Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
        }
    }

    /// Compute Shannon entropy of gate distribution
    fn compute_entropy(gates: &[f32]) -> f32 {
        gates
            .iter()
            .filter(|&&g| g > 0.0)
            .map(|&g| -g * g.log2())
            .sum()
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
    /// Decision with selected adapter indices and Q15 gates
    pub fn route_with_adapter_info(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[AdapterInfo],
    ) -> Decision {
        if priors.len() != adapter_info.len() {
            tracing::warn!(
                "Priors length ({}) != adapter_info length ({}), falling back to basic route",
                priors.len(),
                adapter_info.len()
            );
            return self.route(features, priors);
        }

        // Compute scores for each adapter with per-adapter feature scoring and penalties
        let mut scores: Vec<(usize, f32)> = priors
            .iter()
            .enumerate()
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
                let q = (g * 32767.0).round() as i16;
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

        // Update activation history (orthogonality tracking)
        if self.orthogonal_enabled {
            if let Some(ref mut constraints) = self.orthogonal_constraints {
                constraints.update_history(&indices, &gates_q15);
            }
        }

        Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
        }
    }

    /// Route with k0 detection (no adapters qualify)
    pub fn route_with_k0_detection(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        if priors.is_empty() {
            // Log k0 event
            let _ = self.log_k0_event("no_adapters_available", features);
            return Decision {
                indices: SmallVec::new(),
                gates_q15: SmallVec::new(),
                entropy: 0.0,
                candidates: Vec::new(),
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
                let q = (g * 32767.0).round() as i16;
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

        Decision {
            indices,
            gates_q15,
            entropy,
            candidates: candidate_entries,
        }
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
    ) -> Decision {
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

        // Route using the computed priors
        self.route(&feature_vec, &priors)
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
#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub id: String,
    pub framework: Option<String>,
    pub languages: Vec<usize>, // Language indices
    pub tier: String,
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
}

impl Decision {
    /// Convert Q15 gates back to float
    pub fn gates_f32(&self) -> Vec<f32> {
        self.gates_q15.iter().map(|&q| q as f32 / 32767.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_topk() {
        let weights = vec![1.0; 10];
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        let features = vec![0.5; 10];
        let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.6, 0.0];

        let decision = router.route(&features, &priors);

        assert_eq!(decision.indices.len(), 3);
        assert_eq!(decision.gates_q15.len(), 3);

        // Gates should sum to approximately 1.0
        let sum: f32 = decision.gates_f32().iter().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_entropy_floor() {
        let weights = vec![1.0; 5];
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.1);

        let features = vec![0.0; 5];
        let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior

        let decision = router.route(&features, &priors);
        let gates = decision.gates_f32();

        // All gates should be >= entropy floor / k
        let min_gate = 0.1 / 3.0;
        for &g in &gates {
            assert!(g >= min_gate - 0.001);
        }
    }

    #[test]
    fn test_route_with_code_features() {
        let weights = vec![1.0; 21]; // 21-dim feature vector
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

        let code_features = CodeFeatures::from_context("Fix this python bug in django app");

        let adapters = vec![
            AdapterInfo {
                id: "python-general".to_string(),
                framework: None,
                languages: vec![0], // Python index
                tier: "persistent".to_string(),
            },
            AdapterInfo {
                id: "django-specific".to_string(),
                framework: Some("django".to_string()),
                languages: vec![0], // Python index
                tier: "persistent".to_string(),
            },
            AdapterInfo {
                id: "rust-general".to_string(),
                framework: None,
                languages: vec![1], // Rust index
                tier: "persistent".to_string(),
            },
        ];

        let decision = router.route_with_code_features(&code_features, &adapters);

        assert_eq!(decision.indices.len(), 3);

        // Django adapter should likely be selected due to framework prior
        // (though exact ordering depends on weights)
        println!("Selected indices: {:?}", decision.indices);
        println!("Gates: {:?}", decision.gates_f32());
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

        println!("Heavy language weights: {}", explanation1.format());
        println!("\nHeavy framework weights: {}", explanation2.format());

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
}
