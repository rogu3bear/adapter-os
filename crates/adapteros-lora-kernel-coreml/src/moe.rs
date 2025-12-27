//! Mixture of Experts (MoE) configuration for CoreML backend
//!
//! This module provides configuration and adapter structures for MoE models,
//! including support for routing-weighted shared LoRA with Q15 gates.
//!
//! # Formula Glossary
//!
//! - `gate_q15`: Q15 fixed-point gate value (0..32767), converted to float via `gate_q15 / 32767.0`
//! - `routing_score`: Per-expert routing weight from the MoE router (sum to 1.0 across active experts)
//! - `lora_scale`: LoRA scaling factor computed as `alpha / rank`
//!
//! The full LoRA contribution formula:
//! ```text
//! expert_out += (gate_q15 / 32767.0) * routing_score[e] * lora_scale * (B @ A) @ x
//! ```

use std::collections::HashMap;

/// Configuration for MoE (Mixture of Experts) models
#[derive(Debug, Clone)]
pub struct MoEConfig {
    /// Number of experts
    pub num_experts: usize,
    /// Number of experts to activate per token
    pub num_experts_per_token: usize,
    /// Number of shared experts (if model uses shared experts, e.g., Qwen3-MoE)
    pub num_shared_experts: Option<usize>,
    /// Hidden size of the model
    pub hidden_size: usize,
    /// MoE intermediate size per expert
    pub moe_intermediate_size: usize,
}

impl MoEConfig {
    /// Create a new MoE config for Qwen3-Coder-30B-A3B
    pub fn qwen3_30b() -> Self {
        Self {
            num_experts: 128,
            num_experts_per_token: 8,
            num_shared_experts: Some(0), // Qwen3 doesn't use shared experts
            hidden_size: 2048,
            moe_intermediate_size: 768,
        }
    }

    /// Create a minimal config for testing
    pub fn minimal(num_experts: usize, num_experts_per_token: usize) -> Self {
        Self {
            num_experts,
            num_experts_per_token,
            num_shared_experts: None,
            hidden_size: 2048,
            moe_intermediate_size: 768,
        }
    }
}

/// LoRA fusion strategy for MoE models
///
/// Determines how LoRA adapters are applied during MoE inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoELoRAStrategy {
    /// Shared LoRA with routing-weighted contribution per expert
    ///
    /// Formula: `expert_out += (gate_q15 / 32767.0) * routing_score[e] * lora_scale * (B @ A) @ x`
    ///
    /// Where:
    /// - `gate_q15`: Q15 fixed-point gate (0..32767)
    /// - `routing_score[e]`: Per-expert routing weight from MoE router
    /// - `lora_scale`: `alpha / rank` scaling factor
    /// - `B @ A`: Precomputed LoRA delta matrix
    RoutingWeightedShared {
        /// Use expert routing scores for per-expert weighting
        use_routing_weights: bool,
    },
    /// Per-expert LoRA (not yet implemented, placeholder)
    #[allow(dead_code)]
    PerExpertLoRA,
}

impl Default for MoELoRAStrategy {
    fn default() -> Self {
        MoELoRAStrategy::RoutingWeightedShared {
            use_routing_weights: true,
        }
    }
}

/// Target layer type for MoE LoRA
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoELoRATarget {
    /// Query projection (attention)
    QProj,
    /// Key projection (attention)
    KProj,
    /// Value projection (attention)
    VProj,
    /// Output projection (attention)
    OProj,
    /// Expert gate projection (MoE MLP)
    GateProj,
    /// Expert up projection (MoE MLP)
    UpProj,
    /// Expert down projection (MoE MLP)
    DownProj,
}

/// Per-target LoRA weights for MoE models
#[derive(Debug, Clone)]
pub struct MoELoRAWeights {
    /// LoRA A matrix (rank x in_features)
    pub lora_a: Vec<f32>,
    /// LoRA B matrix (out_features x rank)
    pub lora_b: Vec<f32>,
    /// Precomputed delta: B @ A (out_features x in_features)
    /// Computed at load time for faster inference
    pub precomputed_delta: Option<Vec<f32>>,
    /// LoRA rank (inner dimension)
    pub rank: usize,
    /// LoRA alpha (scaling factor numerator)
    pub alpha: f32,
    /// Input dimension
    pub in_features: usize,
    /// Output dimension
    pub out_features: usize,
}

impl MoELoRAWeights {
    /// Compute the precomputed delta (B @ A) for faster inference
    pub fn precompute_delta(&mut self) {
        if self.precomputed_delta.is_some() {
            return;
        }

        // B: (out_features, rank), A: (rank, in_features)
        // delta = B @ A: (out_features, in_features)
        let mut delta = vec![0.0f32; self.out_features * self.in_features];

        for out_idx in 0..self.out_features {
            for in_idx in 0..self.in_features {
                let mut sum = 0.0f32;
                for r in 0..self.rank {
                    // B[out_idx, r] * A[r, in_idx]
                    let b_val = self.lora_b[out_idx * self.rank + r];
                    let a_val = self.lora_a[r * self.in_features + in_idx];
                    sum += b_val * a_val;
                }
                delta[out_idx * self.in_features + in_idx] = sum;
            }
        }

        self.precomputed_delta = Some(delta);
    }

    /// Get the LoRA scaling factor (alpha / rank)
    ///
    /// This is the standard LoRA scaling applied to the low-rank update:
    /// `output += lora_scale * (B @ A) @ x`
    #[inline]
    pub fn lora_scale(&self) -> f32 {
        self.alpha / self.rank as f32
    }

    /// Alias for `lora_scale()` - returns alpha / rank
    #[deprecated(since = "0.1.0", note = "Use `lora_scale()` for clarity")]
    #[inline]
    pub fn scale(&self) -> f32 {
        self.lora_scale()
    }
}

/// MoE adapter weights collection
#[derive(Debug, Clone, Default)]
pub struct MoEAdapterWeights {
    /// Target-specific LoRA weights
    pub targets: HashMap<MoELoRATarget, MoELoRAWeights>,
    /// Strategy for applying LoRA during MoE inference
    pub strategy: MoELoRAStrategy,
}

impl MoEAdapterWeights {
    /// Create new empty adapter weights
    pub fn new() -> Self {
        Self::default()
    }

    /// Precompute all deltas for faster inference
    pub fn precompute_all_deltas(&mut self) {
        for weights in self.targets.values_mut() {
            weights.precompute_delta();
        }
    }

    /// Get total memory usage in bytes
    pub fn memory_bytes(&self) -> usize {
        self.targets
            .values()
            .map(|w| {
                let base = (w.lora_a.len() + w.lora_b.len()) * std::mem::size_of::<f32>();
                let delta = w
                    .precomputed_delta
                    .as_ref()
                    .map(|d| d.len() * std::mem::size_of::<f32>())
                    .unwrap_or(0);
                base + delta
            })
            .sum()
    }
}

/// GPU fingerprint for MoE adapters (extends regular GPU fingerprint)
#[derive(Debug, Clone)]
pub struct MoEGpuFingerprint {
    /// Adapter slot ID
    pub adapter_id: u16,
    /// Total buffer size in bytes
    pub total_buffer_bytes: u64,
    /// Combined hash of all expert weights
    pub combined_hash: adapteros_core::B3Hash,
    /// Per-expert fingerprints (expert_id -> hash)
    pub expert_fingerprints: HashMap<u16, adapteros_core::B3Hash>,
    /// Number of loaded experts (for cross-layer determinism verification)
    pub loaded_expert_count: usize,
}

// =============================================================================
// Deprecated type aliases for backwards compatibility
// =============================================================================

/// Deprecated: Use [`MoEConfig`] instead
#[deprecated(since = "0.2.0", note = "Use `MoEConfig` instead (correct MoE casing)")]
pub type MoeConfig = MoEConfig;

/// Deprecated: Use [`MoELoRAStrategy`] instead
#[deprecated(
    since = "0.2.0",
    note = "Use `MoELoRAStrategy` instead (correct MoE/LoRA casing)"
)]
pub type MoeLoraStrategy = MoELoRAStrategy;

/// Deprecated: Use [`MoELoRATarget`] instead
#[deprecated(
    since = "0.2.0",
    note = "Use `MoELoRATarget` instead (correct MoE/LoRA casing)"
)]
pub type MoeLoraTarget = MoELoRATarget;

/// Deprecated: Use [`MoELoRAWeights`] instead
#[deprecated(
    since = "0.2.0",
    note = "Use `MoELoRAWeights` instead (correct MoE/LoRA casing)"
)]
pub type MoeLoraWeights = MoELoRAWeights;

/// Deprecated: Use [`MoEAdapterWeights`] instead
#[deprecated(
    since = "0.2.0",
    note = "Use `MoEAdapterWeights` instead (correct MoE casing)"
)]
pub type MoeAdapterWeights = MoEAdapterWeights;

/// Deprecated: Use [`MoEGpuFingerprint`] instead
#[deprecated(
    since = "0.2.0",
    note = "Use `MoEGpuFingerprint` instead (correct MoE casing)"
)]
pub type MoeGpuFingerprint = MoEGpuFingerprint;

/// Deprecated: Use [`PerExpertLoRA`] variant instead
#[deprecated(since = "0.2.0", note = "Use `MoELoRAStrategy::PerExpertLoRA` instead")]
pub const PER_EXPERT_LORA: MoELoRAStrategy = MoELoRAStrategy::PerExpertLoRA;

// =============================================================================
// Compile-only tests for name resolution
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_names_resolve() {
        let _config = MoEConfig::minimal(4, 2);
        let _strategy = MoELoRAStrategy::default();
        let _target = MoELoRATarget::QProj;
        let _weights = MoEAdapterWeights::new();
    }

    #[test]
    #[allow(deprecated)]
    fn deprecated_names_still_resolve() {
        // These should compile but emit deprecation warnings
        // Old (wrong) names now point to new (correct) names
        let _config: MoeConfig = MoEConfig::minimal(4, 2);
        let _strategy: MoeLoraStrategy = MoELoRAStrategy::default();
        let _target: MoeLoraTarget = MoELoRATarget::QProj;
        let _weights: MoeAdapterWeights = MoEAdapterWeights::new();
    }

    #[test]
    fn lora_scale_method_works() {
        let weights = MoELoRAWeights {
            lora_a: vec![],
            lora_b: vec![],
            precomputed_delta: None,
            rank: 16,
            alpha: 32.0,
            in_features: 0,
            out_features: 0,
        };
        assert_eq!(weights.lora_scale(), 2.0); // 32 / 16 = 2
    }

    #[test]
    #[allow(deprecated)]
    fn deprecated_scale_method_works() {
        let weights = MoELoRAWeights {
            lora_a: vec![],
            lora_b: vec![],
            precomputed_delta: None,
            rank: 16,
            alpha: 32.0,
            in_features: 0,
            out_features: 0,
        };
        assert_eq!(weights.scale(), 2.0); // deprecated wrapper
    }
}
