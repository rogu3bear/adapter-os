//! Per-layer adapter MoE routing (Patent 3535886.0002 Claim 7)
//!
//! This module implements adapter-layer mixture-of-experts routing where
//! adapters are routed per transformer layer, not globally. Each layer can
//! activate a different subset of adapters with different gate values.

use crate::quantization::ROUTER_GATE_Q15_DENOM;
use crate::types::{AdapterInfo, Decision};
use crate::CodeFeatures;
use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

// =============================================================================
// Layer Types
// =============================================================================

/// Type of transformer layer for routing context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerType {
    /// Self-attention layer
    Attention,
    /// Feed-forward network layer
    Ffn,
    /// Combined attention + FFN (fused layer)
    Combined,
    /// Layer normalization (typically not routed)
    LayerNorm,
    /// Embedding layer
    Embedding,
    /// Output projection layer
    Output,
}

impl LayerType {
    /// Check if this layer type typically uses adapter routing
    pub fn is_routable(&self) -> bool {
        matches!(self, Self::Attention | Self::Ffn | Self::Combined)
    }

    /// Get the default routing weight multiplier for this layer type
    pub fn default_weight_multiplier(&self) -> f32 {
        match self {
            Self::Attention => 1.0,
            Self::Ffn => 1.2, // FFN layers often benefit more from adapters
            Self::Combined => 1.1,
            Self::LayerNorm => 0.0,
            Self::Embedding => 0.5,
            Self::Output => 0.8,
        }
    }
}

/// Features extracted at a specific layer for routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerFeatures {
    /// Layer index (0-based)
    pub layer_idx: u32,
    /// Type of layer (attention, FFN, etc.)
    pub layer_type: LayerType,
    /// Attention entropy at this layer (for attention layers)
    pub attention_entropy: Option<f32>,
    /// L2 norm of hidden states at this layer
    pub hidden_state_norm: f32,
    /// Mean activation magnitude
    pub activation_mean: f32,
    /// Variance of activations
    pub activation_variance: f32,
    /// Optional: residual magnitude for skip connection analysis
    pub residual_magnitude: Option<f32>,
}

impl LayerFeatures {
    /// Create new layer features
    pub fn new(layer_idx: u32, layer_type: LayerType, hidden_state_norm: f32) -> Self {
        Self {
            layer_idx,
            layer_type,
            attention_entropy: None,
            hidden_state_norm,
            activation_mean: 0.0,
            activation_variance: 0.0,
            residual_magnitude: None,
        }
    }

    /// Create layer features with attention entropy
    pub fn with_attention_entropy(mut self, entropy: f32) -> Self {
        self.attention_entropy = Some(entropy);
        self
    }

    /// Create layer features with activation statistics
    pub fn with_activation_stats(mut self, mean: f32, variance: f32) -> Self {
        self.activation_mean = mean;
        self.activation_variance = variance;
        self
    }

    /// Compute a normalized difficulty score for this layer.
    /// Higher values indicate the layer may need more adapter capacity.
    pub fn difficulty_score(&self) -> f32 {
        let entropy_factor = self.attention_entropy.unwrap_or(0.5);
        let norm_factor = (self.hidden_state_norm / 100.0).clamp(0.0, 1.0);
        let variance_factor = self.activation_variance.sqrt().clamp(0.0, 1.0);

        // Weighted combination
        0.4 * entropy_factor + 0.3 * norm_factor + 0.3 * variance_factor
    }
}

// =============================================================================
// Layer Routing Decision
// =============================================================================

/// Per-layer routing decision for adapter-layer MoE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRoutingDecision {
    /// Layer index this decision applies to
    pub layer_idx: u32,
    /// Layer type for context
    pub layer_type: LayerType,
    /// The routing decision (selected adapters and gates)
    pub decision: LayerDecision,
    /// Layer features used for this decision
    pub layer_features: Option<LayerFeatures>,
    /// BLAKE3 hash of this layer decision for audit
    pub decision_hash_b3: Option<B3Hash>,
}

impl LayerRoutingDecision {
    /// Create a new layer routing decision
    pub fn new(layer_idx: u32, layer_type: LayerType, decision: LayerDecision) -> Self {
        Self {
            layer_idx,
            layer_type,
            decision,
            layer_features: None,
            decision_hash_b3: None,
        }
    }

    /// Attach layer features
    pub fn with_features(mut self, features: LayerFeatures) -> Self {
        self.layer_features = Some(features);
        self
    }

    /// Compute and attach decision hash
    pub fn with_hash(mut self) -> Self {
        self.decision_hash_b3 = Some(self.compute_hash());
        self
    }

    /// Compute BLAKE3 hash of this layer decision
    pub fn compute_hash(&self) -> B3Hash {
        // Collect all bytes to hash
        let mut data = Vec::with_capacity(128);

        // Hash layer info
        data.extend_from_slice(&self.layer_idx.to_le_bytes());
        data.push(self.layer_type as u8);

        // Hash adapter indices
        data.extend_from_slice(&(self.decision.indices.len() as u32).to_le_bytes());
        for idx in &self.decision.indices {
            data.extend_from_slice(&idx.to_le_bytes());
        }

        // Hash gates
        for gate in &self.decision.gates_q15 {
            data.extend_from_slice(&gate.to_le_bytes());
        }

        B3Hash::hash(&data)
    }
}

/// Simplified decision struct for per-layer routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDecision {
    /// Selected adapter indices for this layer
    pub indices: SmallVec<[u16; 8]>,
    /// Q15 quantized gate values
    pub gates_q15: SmallVec<[i16; 8]>,
    /// Entropy of the gate distribution
    pub entropy: f32,
    /// Raw scores before softmax (for debugging)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub raw_scores: Vec<f32>,
}

impl LayerDecision {
    /// Create a new layer decision
    pub fn new(indices: SmallVec<[u16; 8]>, gates_q15: SmallVec<[i16; 8]>, entropy: f32) -> Self {
        Self {
            indices,
            gates_q15,
            entropy,
            raw_scores: Vec::new(),
        }
    }

    /// Create from a global Decision
    pub fn from_decision(decision: &Decision) -> Self {
        Self {
            indices: decision.indices.clone(),
            gates_q15: decision.gates_q15.clone(),
            entropy: decision.entropy,
            raw_scores: decision.candidates.iter().map(|c| c.raw_score).collect(),
        }
    }

    /// Convert Q15 gates to float
    pub fn gates_f32(&self) -> Vec<f32> {
        self.gates_q15
            .iter()
            .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
            .collect()
    }

    /// Check if this layer is using adapters
    pub fn is_active(&self) -> bool {
        !self.indices.is_empty()
    }

    /// Get the dominant adapter (highest gate value)
    pub fn dominant_adapter(&self) -> Option<(u16, f32)> {
        self.indices
            .iter()
            .zip(self.gates_q15.iter())
            .max_by_key(|(_, &g)| g)
            .map(|(&idx, &g)| (idx, g as f32 / ROUTER_GATE_Q15_DENOM))
    }
}

// =============================================================================
// Layer Routing Chain
// =============================================================================

/// Full routing chain for a generation step (per-token, per-layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRoutingChain {
    /// Generation step index
    pub step_idx: u32,
    /// Token ID generated at this step (0 for prompt tokens)
    pub token_id: u32,
    /// Per-layer routing decisions
    pub layer_decisions: Vec<LayerRoutingDecision>,
    /// BLAKE3 hash of the entire chain for verification
    pub chain_hash_b3: B3Hash,
    /// Total number of layers
    pub num_layers: u32,
}

impl LayerRoutingChain {
    /// Create a new layer routing chain
    pub fn new(step_idx: u32, token_id: u32, num_layers: u32) -> Self {
        Self {
            step_idx,
            token_id,
            layer_decisions: Vec::with_capacity(num_layers as usize),
            chain_hash_b3: B3Hash::zero(),
            num_layers,
        }
    }

    /// Add a layer decision
    pub fn add_decision(&mut self, decision: LayerRoutingDecision) {
        self.layer_decisions.push(decision);
    }

    /// Finalize the chain by computing the chain hash
    pub fn finalize(&mut self) {
        self.chain_hash_b3 = self.compute_chain_hash();
    }

    /// Compute BLAKE3 hash of the entire routing chain
    pub fn compute_chain_hash(&self) -> B3Hash {
        // Collect all bytes to hash
        let mut data = Vec::with_capacity(256);

        // Hash metadata
        data.extend_from_slice(&self.step_idx.to_le_bytes());
        data.extend_from_slice(&self.token_id.to_le_bytes());
        data.extend_from_slice(&self.num_layers.to_le_bytes());

        // Hash each layer decision
        for decision in &self.layer_decisions {
            let layer_hash = decision.compute_hash();
            data.extend_from_slice(layer_hash.as_bytes());
        }

        B3Hash::hash(&data)
    }

    /// Get decision for a specific layer
    pub fn get_layer_decision(&self, layer_idx: u32) -> Option<&LayerRoutingDecision> {
        self.layer_decisions
            .iter()
            .find(|d| d.layer_idx == layer_idx)
    }

    /// Check if all layers have decisions
    pub fn is_complete(&self) -> bool {
        self.layer_decisions.len() == self.num_layers as usize
    }

    /// Get summary statistics for the chain
    pub fn summary(&self) -> LayerRoutingChainSummary {
        let active_layers = self
            .layer_decisions
            .iter()
            .filter(|d| d.decision.is_active())
            .count();

        let total_adapters: usize = self
            .layer_decisions
            .iter()
            .map(|d| d.decision.indices.len())
            .sum();

        let avg_entropy = if self.layer_decisions.is_empty() {
            0.0
        } else {
            self.layer_decisions
                .iter()
                .map(|d| d.decision.entropy)
                .sum::<f32>()
                / self.layer_decisions.len() as f32
        };

        LayerRoutingChainSummary {
            step_idx: self.step_idx,
            num_layers: self.num_layers,
            active_layers: active_layers as u32,
            total_adapters,
            avg_entropy,
            chain_hash_b3: self.chain_hash_b3,
        }
    }
}

/// Summary statistics for a layer routing chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRoutingChainSummary {
    pub step_idx: u32,
    pub num_layers: u32,
    pub active_layers: u32,
    pub total_adapters: usize,
    pub avg_entropy: f32,
    pub chain_hash_b3: B3Hash,
}

// =============================================================================
// Layer Router Configuration
// =============================================================================

/// Configuration for per-layer adapter routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRouterConfig {
    /// Number of transformer layers
    pub num_layers: u32,
    /// K value for top-k selection (per layer)
    pub k_per_layer: usize,
    /// Whether to share routing decisions across similar layers
    pub share_similar_layers: bool,
    /// Layer groups for shared routing (e.g., [[0,1,2], [3,4,5]])
    #[serde(default)]
    pub layer_groups: Vec<Vec<u32>>,
    /// Per-layer weight multipliers (indexed by layer)
    #[serde(default)]
    pub layer_weights: Vec<f32>,
    /// Minimum layers that must use adapters
    pub min_active_layers: u32,
    /// Maximum layers that can use adapters (0 = unlimited)
    pub max_active_layers: u32,
}

impl Default for LayerRouterConfig {
    fn default() -> Self {
        Self {
            num_layers: 32, // Typical for 7B models
            k_per_layer: 2, // Sparse selection per layer
            share_similar_layers: false,
            layer_groups: Vec::new(),
            layer_weights: Vec::new(),
            min_active_layers: 0,
            max_active_layers: 0, // Unlimited
        }
    }
}

impl LayerRouterConfig {
    /// Create config for a specific model size
    pub fn for_model(num_layers: u32) -> Self {
        Self {
            num_layers,
            ..Default::default()
        }
    }

    /// Get weight for a specific layer (defaults to 1.0)
    pub fn get_layer_weight(&self, layer_idx: u32) -> f32 {
        self.layer_weights
            .get(layer_idx as usize)
            .copied()
            .unwrap_or(1.0)
    }

    /// Check if a layer is in a shared group
    pub fn get_layer_group(&self, layer_idx: u32) -> Option<&[u32]> {
        self.layer_groups
            .iter()
            .find(|group| group.contains(&layer_idx))
            .map(|v| v.as_slice())
    }
}

// =============================================================================
// Layer-Aware Scoring
// =============================================================================

/// Context for layer-aware adapter scoring
#[derive(Debug, Clone)]
pub struct LayerScoringContext<'a> {
    /// Global input features (from prompt)
    pub input_features: &'a CodeFeatures,
    /// Layer-specific features
    pub layer_features: &'a LayerFeatures,
    /// Available adapters
    pub adapters: &'a [AdapterInfo],
    /// Layer router configuration
    pub config: &'a LayerRouterConfig,
}

/// Compute layer-specific adapter scores
pub fn compute_layer_adapter_scores(ctx: &LayerScoringContext, base_scores: &[f32]) -> Vec<f32> {
    let layer_weight = ctx.config.get_layer_weight(ctx.layer_features.layer_idx);
    let layer_type_mult = ctx.layer_features.layer_type.default_weight_multiplier();
    let difficulty = ctx.layer_features.difficulty_score();

    base_scores
        .iter()
        .enumerate()
        .map(|(i, &score)| {
            // Apply layer-specific modifiers
            let mut adjusted = score * layer_weight * layer_type_mult;

            // Boost adapters for high-difficulty layers
            if difficulty > 0.7 {
                adjusted *= 1.0 + 0.2 * (difficulty - 0.7);
            }

            // Apply adapter-specific layer preferences if available
            if let Some(adapter) = ctx.adapters.get(i) {
                // MoE-recommended adapters get a boost in deep layers
                if adapter.recommended_for_moe
                    && ctx.layer_features.layer_idx > ctx.config.num_layers / 2
                {
                    adjusted *= 1.1;
                }
            }

            adjusted
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_type_routable() {
        assert!(LayerType::Attention.is_routable());
        assert!(LayerType::Ffn.is_routable());
        assert!(!LayerType::LayerNorm.is_routable());
    }

    #[test]
    fn test_layer_features_difficulty() {
        let features = LayerFeatures::new(0, LayerType::Attention, 50.0)
            .with_attention_entropy(0.8)
            .with_activation_stats(0.5, 0.25);

        let difficulty = features.difficulty_score();
        assert!(difficulty > 0.0 && difficulty < 1.0);
    }

    #[test]
    fn test_layer_decision_gates() {
        let decision = LayerDecision::new(
            smallvec::smallvec![0, 1],
            smallvec::smallvec![16384, 16383], // ~0.5 each
            0.5,
        );

        let gates = decision.gates_f32();
        assert_eq!(gates.len(), 2);
        assert!((gates[0] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_layer_routing_chain_hash() {
        let mut chain = LayerRoutingChain::new(0, 0, 2);

        let decision1 = LayerRoutingDecision::new(
            0,
            LayerType::Attention,
            LayerDecision::new(smallvec::smallvec![0], smallvec::smallvec![32767], 0.0),
        );
        let decision2 = LayerRoutingDecision::new(
            1,
            LayerType::Ffn,
            LayerDecision::new(smallvec::smallvec![1], smallvec::smallvec![32767], 0.0),
        );

        chain.add_decision(decision1);
        chain.add_decision(decision2);
        chain.finalize();

        assert!(!chain.chain_hash_b3.is_zero());

        // Hash should be deterministic
        let hash2 = chain.compute_chain_hash();
        assert_eq!(chain.chain_hash_b3, hash2);
    }

    #[test]
    fn test_layer_router_config() {
        let config = LayerRouterConfig::for_model(32);
        assert_eq!(config.num_layers, 32);
        assert_eq!(config.get_layer_weight(0), 1.0);
    }
}
