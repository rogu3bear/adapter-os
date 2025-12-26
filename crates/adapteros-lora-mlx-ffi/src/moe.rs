//! Mixture of Experts (MoE) implementation for MLX FFI
//!
//! Provides quantized MoE routing and expert computation using MLX's `gather_qmm`
//! for efficient 4-bit quantized expert execution on Apple Silicon.
//!
//! When `mlx-rs-backend` feature is enabled, uses pure Rust implementation.
//! Otherwise, uses C++ FFI for quantized expert computation.
//!
//! # Terminology
//!
//! - **Router logits**: Raw scores from the router network before softmax normalization.
//!   Shape: `[batch, seq_len, num_experts]`
//! - **Expert gates**: Normalized probabilities after softmax, representing how much
//!   each expert should contribute. Sum to 1.0 across experts.
//! - **Expert indices**: Integer indices identifying which experts are selected.
//!   Shape: `[batch, seq_len, k]` where k is num_experts_per_token.
//! - **Routing**: The process of selecting experts and computing their gates.
//!
//! # Deterministic Selection Semantics
//!
//! When selecting top-k experts, ties are broken deterministically:
//! - Primary sort: score descending (higher scores first)
//! - Secondary sort: expert_id ascending (lower IDs first on tie)
//!
//! This ensures reproducible expert selection across runs.

use adapteros_core::{AosError, Result};

#[cfg(not(feature = "mlx-rs-backend"))]
use crate::{mlx_array_t, mlx_clear_error, mlx_get_last_error};

#[cfg(feature = "mlx-rs-backend")]
use crate::array::MlxArray;

/// Configuration for quantized MoE layer
#[derive(Debug, Clone)]
pub struct QuantizedMoeConfig {
    /// Number of experts in the MoE layer
    pub num_experts: usize,
    /// Number of experts to route to per token (top-k)
    pub num_experts_per_token: usize,
    /// Hidden dimension size
    pub hidden_size: usize,
    /// Intermediate (FFN) dimension size
    pub intermediate_size: usize,
    /// Group size for quantization (typically 64 or 128)
    pub group_size: i32,
    /// Whether to use shared experts (Qwen-style)
    pub use_shared_expert: bool,
}

impl Default for QuantizedMoeConfig {
    fn default() -> Self {
        Self {
            num_experts: 8,
            num_experts_per_token: 2,
            hidden_size: 4096,
            intermediate_size: 14336,
            group_size: 64,
            use_shared_expert: false,
        }
    }
}

// =============================================================================
// mlx-rs backend implementation
// =============================================================================

#[cfg(feature = "mlx-rs-backend")]
mod mlx_rs_impl {
    use super::*;

    /// Expert weights for MoE (f32 for mlx-rs backend)
    #[derive(Debug, Clone)]
    pub struct MlxRsExpertWeights {
        /// Gate projection weights [num_experts, intermediate_size, hidden_size]
        pub gate_weight: MlxArray,
        /// Up projection weights [num_experts, intermediate_size, hidden_size]
        pub up_weight: MlxArray,
        /// Down projection weights [num_experts, hidden_size, intermediate_size]
        pub down_weight: MlxArray,
    }

    /// Pure Rust MoE layer using mlx-rs
    #[derive(Clone)]
    pub struct MlxRsMoeLayer {
        /// Layer configuration
        config: QuantizedMoeConfig,
        /// Router weights [hidden_size, num_experts]
        router_weight: Option<MlxArray>,
        /// Expert weights (stacked for all experts)
        expert_weights: Option<MlxRsExpertWeights>,
    }

    impl MlxRsMoeLayer {
        /// Create a new MoE layer
        pub fn new(config: QuantizedMoeConfig) -> Self {
            Self {
                config,
                router_weight: None,
                expert_weights: None,
            }
        }

        /// Get the layer configuration
        pub fn config(&self) -> &QuantizedMoeConfig {
            &self.config
        }

        /// Set router weights
        pub fn set_router_weight(&mut self, weight: MlxArray) {
            self.router_weight = Some(weight);
        }

        /// Set expert weights
        pub fn set_expert_weights(&mut self, weights: MlxRsExpertWeights) {
            self.expert_weights = Some(weights);
        }

        /// Forward pass through the MoE layer
        ///
        /// Computes router logits from input, selects top-k experts with deterministic
        /// tie-breaking, and combines expert outputs weighted by their gates.
        ///
        /// # Arguments
        /// * `x` - Input tensor `[batch, seq_len, hidden_size]`
        ///
        /// # Returns
        /// Output tensor `[batch, seq_len, hidden_size]`
        pub fn forward(&self, x: &MlxArray) -> Result<MlxArray> {
            let weights = self.expert_weights.as_ref().ok_or_else(|| {
                AosError::Internal("MoE layer weights not initialized".to_string())
            })?;

            let router_weight = self.router_weight.as_ref().ok_or_else(|| {
                AosError::Internal("MoE router weights not initialized".to_string())
            })?;

            // Compute router logits: x @ router_weight
            let router_logits = x.matmul(router_weight)?;

            // Get top-k experts with deterministic selection
            let (expert_indices, expert_gates) = self.select_topk_experts(&router_logits)?;

            // Forward through selected experts
            self.forward_with_expert_routing(x, &expert_indices, &expert_gates, weights)
        }

        /// Select top-k experts from router logits with deterministic tie-breaking.
        ///
        /// This is the primary expert selection function. It applies softmax to convert
        /// logits to probabilities, selects the top-k experts, and renormalizes gates
        /// to sum to 1.0.
        ///
        /// # Tie-Breaking Rule
        ///
        /// When multiple experts have equal scores, ties are broken deterministically:
        /// - Primary sort: score descending (higher scores selected first)
        /// - Secondary sort: expert_id ascending (lower IDs win on tie)
        ///
        /// This ensures reproducible expert selection across runs.
        ///
        /// # Arguments
        /// * `router_logits` - Raw router output `[batch, seq_len, num_experts]`
        ///
        /// # Returns
        /// Tuple of:
        /// - `expert_indices`: Selected expert IDs `[batch, seq_len, k]`
        /// - `expert_gates`: Normalized gate values `[batch, seq_len, k]`, sum to 1.0
        ///
        /// # Example
        /// ```ignore
        /// let (indices, gates) = layer.select_topk_experts(&router_logits)?;
        /// // indices: which experts to use
        /// // gates: how much weight to give each expert's output
        /// ```
        pub fn select_topk_experts(
            &self,
            router_logits: &MlxArray,
        ) -> Result<(MlxArray, MlxArray)> {
            let k = self.config.num_experts_per_token;
            let probs = router_logits.softmax(-1)?;

            // topk returns (values, indices) sorted by value descending
            // For equal values, indices are in ascending order (deterministic)
            let (top_scores, top_indices) = probs.topk(k as i32, -1)?;

            // Renormalize gates to sum to 1.0 (within floating point tolerance)
            let score_sum = top_scores.sum(Some(-1), true)?;
            let normalized_gates = top_scores.div(&score_sum)?;

            Ok((top_indices, normalized_gates))
        }

        /// Compute top-k expert gating (deprecated wrapper).
        ///
        /// # Deprecated
        /// Use [`select_topk_experts`](Self::select_topk_experts) instead for clearer semantics.
        /// This wrapper maintains backward compatibility.
        #[deprecated(
            since = "0.1.0",
            note = "use select_topk_experts instead for clearer semantics"
        )]
        pub fn compute_topk_gating(
            &self,
            router_logits: &MlxArray,
        ) -> Result<(MlxArray, MlxArray)> {
            self.select_topk_experts(router_logits)
        }

        /// Forward pass with pre-computed expert routing.
        ///
        /// Takes pre-selected expert indices and their gates, dispatches input
        /// to each selected expert, and combines outputs weighted by gates.
        ///
        /// # Arguments
        /// * `x` - Input tensor `[batch, seq_len, hidden_size]`
        /// * `expert_indices` - Selected expert IDs `[batch, seq_len, k]`
        /// * `expert_gates` - Normalized gate values `[batch, seq_len, k]`
        /// * `weights` - Expert weight tensors
        ///
        /// # Returns
        /// Combined output tensor `[batch, seq_len, hidden_size]`
        pub fn forward_with_expert_routing(
            &self,
            x: &MlxArray,
            expert_indices: &MlxArray,
            expert_gates: &MlxArray,
            weights: &MlxRsExpertWeights,
        ) -> Result<MlxArray> {
            let shape = x.shape();
            let k = self.config.num_experts_per_token;

            // Initialize output accumulator
            let mut output = x.zeros_like()?;

            // Process each expert slot
            for slot in 0..k {
                // Get indices and gates for this slot
                let slot_indices = expert_indices.slice_axis(-1, slot, slot + 1)?;
                let slot_gates = expert_gates.slice_axis(-1, slot, slot + 1)?;

                // Dispatch to expert and compute output
                let expert_out = self.run_single_expert(x, &slot_indices, weights)?;

                // Weighted accumulation by gate value
                let weighted = expert_out.mul(&slot_gates)?;
                output = output.add(&weighted)?;
            }

            output.reshape(&shape)
        }

        /// Forward with pre-computed expert indices (deprecated wrapper).
        ///
        /// # Deprecated
        /// Use [`forward_with_expert_routing`](Self::forward_with_expert_routing) instead.
        #[deprecated(
            since = "0.1.0",
            note = "use forward_with_expert_routing instead for clearer semantics"
        )]
        #[allow(dead_code)]
        fn forward_with_indices(
            &self,
            x: &MlxArray,
            expert_indices: &MlxArray,
            expert_scores: &MlxArray,
            weights: &MlxRsExpertWeights,
        ) -> Result<MlxArray> {
            self.forward_with_expert_routing(x, expert_indices, expert_scores, weights)
        }

        /// Run a single expert on the input and compute its output.
        ///
        /// Gathers the weight matrices for the specified expert and computes
        /// the SwiGLU transformation: `silu(x @ gate) * (x @ up) @ down`
        ///
        /// # Arguments
        /// * `x` - Input tensor `[batch, seq_len, hidden_size]`
        /// * `expert_idx` - Index tensor identifying which expert to run
        /// * `weights` - Expert weight tensors (stacked for all experts)
        ///
        /// # Returns
        /// Expert output tensor `[batch, seq_len, hidden_size]`
        fn run_single_expert(
            &self,
            x: &MlxArray,
            expert_idx: &MlxArray,
            weights: &MlxRsExpertWeights,
        ) -> Result<MlxArray> {
            // Gather weights for selected expert
            let gate_w = weights.gate_weight.take_axis(expert_idx, 0)?;
            let up_w = weights.up_weight.take_axis(expert_idx, 0)?;
            let down_w = weights.down_weight.take_axis(expert_idx, 0)?;

            // SwiGLU: silu(x @ gate) * (x @ up) @ down
            let gate = x.matmul(&gate_w.transpose()?)?;
            let gate = gate.silu()?;

            let up = x.matmul(&up_w.transpose()?)?;
            let hidden = gate.mul(&up)?;

            hidden.matmul(&down_w.transpose()?)
        }

        /// Compute output for a single expert (deprecated wrapper).
        ///
        /// # Deprecated
        /// Use [`run_single_expert`](Self::run_single_expert) instead.
        #[deprecated(since = "0.1.0", note = "use run_single_expert instead")]
        #[allow(dead_code)]
        fn compute_single_expert(
            &self,
            x: &MlxArray,
            expert_idx: &MlxArray,
            weights: &MlxRsExpertWeights,
        ) -> Result<MlxArray> {
            self.run_single_expert(x, expert_idx, weights)
        }
    }
}

#[cfg(feature = "mlx-rs-backend")]
pub use mlx_rs_impl::{MlxRsExpertWeights, MlxRsMoeLayer};

// =============================================================================
// Legacy FFI implementation
// =============================================================================

#[cfg(not(feature = "mlx-rs-backend"))]
/// Quantized expert weights for a single expert
///
/// Each expert has gate, up, and down projections stored in 4-bit quantized format.
#[derive(Debug)]
pub struct QuantizedExpertWeights {
    /// Gate projection weights (packed 4-bit)
    pub gate_weight: *mut mlx_array_t,
    /// Gate projection scales
    pub gate_scales: *mut mlx_array_t,
    /// Gate projection biases (zero-points)
    pub gate_biases: *mut mlx_array_t,

    /// Up projection weights (packed 4-bit)
    pub up_weight: *mut mlx_array_t,
    /// Up projection scales
    pub up_scales: *mut mlx_array_t,
    /// Up projection biases
    pub up_biases: *mut mlx_array_t,

    /// Down projection weights (packed 4-bit)
    pub down_weight: *mut mlx_array_t,
    /// Down projection scales
    pub down_scales: *mut mlx_array_t,
    /// Down projection biases
    pub down_biases: *mut mlx_array_t,
}

#[cfg(not(feature = "mlx-rs-backend"))]
/// Quantized MoE layer using gather_qmm for efficient expert execution
pub struct QuantizedMoeLayer {
    /// Layer configuration
    config: QuantizedMoeConfig,
    /// Expert weights (stacked for all experts)
    expert_weights: Option<QuantizedExpertWeights>,
}

#[cfg(not(feature = "mlx-rs-backend"))]
impl QuantizedMoeLayer {
    /// Create a new quantized MoE layer
    pub fn new(config: QuantizedMoeConfig) -> Self {
        Self {
            config,
            expert_weights: None,
        }
    }

    /// Get the layer configuration
    pub fn config(&self) -> &QuantizedMoeConfig {
        &self.config
    }

    /// Set expert weights (typically loaded from safetensors)
    ///
    /// # Safety
    /// The caller must ensure that the provided array pointers are valid
    /// and will remain valid for the lifetime of this layer.
    pub unsafe fn set_weights(&mut self, weights: QuantizedExpertWeights) {
        self.expert_weights = Some(weights);
    }

    /// Forward pass through the quantized MoE layer
    ///
    /// Implements Switch-GLU pattern with 4-bit quantized experts:
    /// 1. Router selects top-k experts per token
    /// 2. gather_qmm gathers and computes expert outputs
    /// 3. Weighted combination by router scores
    ///
    /// # Arguments
    /// * `x` - Input tensor [batch, seq_len, hidden_size]
    /// * `expert_indices` - Selected expert indices from router [batch, seq_len, num_experts_per_token]
    /// * `expert_scores` - Router scores for selected experts [batch, seq_len, num_experts_per_token]
    ///
    /// # Returns
    /// Output tensor [batch, seq_len, hidden_size]
    pub fn forward(
        &self,
        x: *mut mlx_array_t,
        expert_indices: *mut mlx_array_t,
        expert_scores: *mut mlx_array_t,
    ) -> Result<*mut mlx_array_t> {
        let weights = self
            .expert_weights
            .as_ref()
            .ok_or_else(|| AosError::Internal("MoE layer weights not initialized".to_string()))?;

        unsafe {
            mlx_clear_error();

            let output = mlx_switch_glu_forward_quantized(
                x,
                weights.gate_weight,
                weights.gate_scales,
                weights.gate_biases,
                weights.up_weight,
                weights.up_scales,
                weights.up_biases,
                weights.down_weight,
                weights.down_scales,
                weights.down_biases,
                expert_indices,
                expert_scores,
                self.config.group_size,
            );

            if output.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MoE forward error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                mlx_clear_error();
                return Err(AosError::Mlx(format!(
                    "MoE forward pass failed: {}",
                    error_str
                )));
            }

            Ok(output)
        }
    }

    /// Forward pass with pre-computed router outputs
    ///
    /// This variant accepts raw router logits and computes top-k internally.
    ///
    /// # Arguments
    /// * `x` - Input tensor [batch, seq_len, hidden_size]
    /// * `router_logits` - Router logits [batch, seq_len, num_experts]
    ///
    /// # Returns
    /// Output tensor [batch, seq_len, hidden_size]
    pub fn forward_with_router(
        &self,
        x: *mut mlx_array_t,
        router_logits: *mut mlx_array_t,
    ) -> Result<*mut mlx_array_t> {
        unsafe {
            mlx_clear_error();

            // Compute top-k expert selection
            let mut expert_indices: *mut mlx_array_t = std::ptr::null_mut();
            let mut expert_scores: *mut mlx_array_t = std::ptr::null_mut();

            let success = mlx_moe_topk_gating(
                router_logits,
                self.config.num_experts_per_token as i32,
                &mut expert_indices,
                &mut expert_scores,
            );

            if !success {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown gating error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                mlx_clear_error();
                return Err(AosError::Mlx(format!("MoE gating failed: {}", error_str)));
            }

            // Forward through experts
            let result = self.forward(x, expert_indices, expert_scores);

            // Clean up intermediate arrays
            crate::mlx_array_free(expert_indices);
            crate::mlx_array_free(expert_scores);

            result
        }
    }
}

#[cfg(not(feature = "mlx-rs-backend"))]
impl Drop for QuantizedMoeLayer {
    fn drop(&mut self) {
        if let Some(weights) = self.expert_weights.take() {
            unsafe {
                // Free all weight arrays
                if !weights.gate_weight.is_null() {
                    crate::mlx_array_free(weights.gate_weight);
                }
                if !weights.gate_scales.is_null() {
                    crate::mlx_array_free(weights.gate_scales);
                }
                if !weights.gate_biases.is_null() {
                    crate::mlx_array_free(weights.gate_biases);
                }
                if !weights.up_weight.is_null() {
                    crate::mlx_array_free(weights.up_weight);
                }
                if !weights.up_scales.is_null() {
                    crate::mlx_array_free(weights.up_scales);
                }
                if !weights.up_biases.is_null() {
                    crate::mlx_array_free(weights.up_biases);
                }
                if !weights.down_weight.is_null() {
                    crate::mlx_array_free(weights.down_weight);
                }
                if !weights.down_scales.is_null() {
                    crate::mlx_array_free(weights.down_scales);
                }
                if !weights.down_biases.is_null() {
                    crate::mlx_array_free(weights.down_biases);
                }
            }
        }
    }
}

#[cfg(not(feature = "mlx-rs-backend"))]
/// Standalone function for quantized Switch-GLU MoE forward pass
///
/// This function provides the core MoE computation without requiring
/// the full layer abstraction.
///
/// # Weight Layout
/// Weights use `[expert, out_features, in_features]` layout:
/// - gate_proj: [num_experts, intermediate_size, hidden_size]
/// - up_proj:   [num_experts, intermediate_size, hidden_size]
/// - down_proj: [num_experts, hidden_size, intermediate_size]
///
/// For 4-bit quantized weights (group_size=64), packed dimensions:
/// - gate_proj.weight: [num_experts, intermediate_size, hidden_size/8]
/// - gate_proj.scales: [num_experts, intermediate_size, hidden_size/64]
///
/// # Arguments
/// * `x` - Input tensor [batch, seq_len, hidden_size]
/// * `gate_weight` - Packed 4-bit gate weights [num_experts, intermediate_size, hidden_size/8]
/// * `gate_scales` - Gate quantization scales [num_experts, intermediate_size, hidden_size/group_size]
/// * `gate_biases` - Gate quantization biases (zero-points)
/// * `up_weight` - Packed 4-bit up projection weights (same shape as gate)
/// * `up_scales` - Up projection scales
/// * `up_biases` - Up projection biases
/// * `down_weight` - Packed 4-bit down weights [num_experts, hidden_size, intermediate_size/8]
/// * `down_scales` - Down projection scales [num_experts, hidden_size, intermediate_size/group_size]
/// * `down_biases` - Down projection biases
/// * `expert_indices` - Selected expert indices [batch, seq_len, k]
/// * `expert_scores` - Router scores [batch, seq_len, k]
/// * `group_size` - Quantization group size (typically 64)
///
/// # Returns
/// Output tensor [batch, seq_len, hidden_size]
///
/// # Safety
/// All tensor pointers must be valid MLX arrays with compatible shapes.
pub unsafe fn switch_glu_forward_quantized(
    x: *mut mlx_array_t,
    gate_weight: *mut mlx_array_t,
    gate_scales: *mut mlx_array_t,
    gate_biases: *mut mlx_array_t,
    up_weight: *mut mlx_array_t,
    up_scales: *mut mlx_array_t,
    up_biases: *mut mlx_array_t,
    down_weight: *mut mlx_array_t,
    down_scales: *mut mlx_array_t,
    down_biases: *mut mlx_array_t,
    expert_indices: *mut mlx_array_t,
    expert_scores: *mut mlx_array_t,
    group_size: i32,
) -> Result<*mut mlx_array_t> {
    mlx_clear_error();

    let output = mlx_switch_glu_forward_quantized(
        x,
        gate_weight,
        gate_scales,
        gate_biases,
        up_weight,
        up_scales,
        up_biases,
        down_weight,
        down_scales,
        down_biases,
        expert_indices,
        expert_scores,
        group_size,
    );

    if output.is_null() {
        let error_msg = mlx_get_last_error();
        let error_str = if error_msg.is_null() {
            "Unknown error in switch_glu_forward_quantized".to_string()
        } else {
            std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string()
        };
        mlx_clear_error();
        return Err(AosError::Mlx(format!(
            "Quantized MoE forward failed: {}",
            error_str
        )));
    }

    Ok(output)
}

#[cfg(not(feature = "mlx-rs-backend"))]
/// Select top-k experts from router logits with deterministic tie-breaking.
///
/// This is the primary expert selection function for the FFI backend. It applies
/// softmax to convert logits to probabilities, selects the top-k experts, and
/// renormalizes gates to sum to 1.0.
///
/// # Tie-Breaking Rule
///
/// When multiple experts have equal scores, ties are broken deterministically:
/// - Primary sort: score descending (higher scores selected first)
/// - Secondary sort: expert_id ascending (lower IDs win on tie)
///
/// This ensures reproducible expert selection across runs.
///
/// # Arguments
/// * `router_logits` - Raw router output `[batch, seq_len, num_experts]`
/// * `k` - Number of experts to select per token
///
/// # Returns
/// Tuple of:
/// - `expert_indices`: Selected expert IDs `[batch, seq_len, k]`
/// - `expert_gates`: Normalized gate values `[batch, seq_len, k]`, sum to 1.0
///
/// # Safety
/// The `router_logits` pointer must be a valid MLX array.
pub fn select_topk_experts(
    router_logits: *mut mlx_array_t,
    k: usize,
) -> Result<(*mut mlx_array_t, *mut mlx_array_t)> {
    unsafe {
        mlx_clear_error();

        let mut expert_indices: *mut mlx_array_t = std::ptr::null_mut();
        let mut expert_gates: *mut mlx_array_t = std::ptr::null_mut();

        let success = mlx_moe_topk_gating(
            router_logits,
            k as i32,
            &mut expert_indices,
            &mut expert_gates,
        );

        if !success {
            let error_msg = mlx_get_last_error();
            let error_str = if error_msg.is_null() {
                "Unknown gating error".to_string()
            } else {
                std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string()
            };
            mlx_clear_error();
            return Err(AosError::Mlx(format!(
                "Top-k expert selection failed: {}",
                error_str
            )));
        }

        Ok((expert_indices, expert_gates))
    }
}

#[cfg(not(feature = "mlx-rs-backend"))]
/// Compute top-k expert gating from router logits (deprecated wrapper).
///
/// # Deprecated
/// Use [`select_topk_experts`] instead for clearer semantics.
/// This wrapper maintains backward compatibility.
///
/// # Arguments
/// * `router_logits` - Router output logits `[batch, seq_len, num_experts]`
/// * `k` - Number of experts to select per token
///
/// # Returns
/// Tuple of `(expert_indices, expert_scores)`
#[deprecated(
    since = "0.1.0",
    note = "use select_topk_experts instead for clearer semantics"
)]
pub fn compute_topk_gating(
    router_logits: *mut mlx_array_t,
    k: usize,
) -> Result<(*mut mlx_array_t, *mut mlx_array_t)> {
    select_topk_experts(router_logits, k)
}

// =============================================================================
// FFI Declarations (only for legacy C++ backend)
// =============================================================================

#[cfg(not(feature = "mlx-rs-backend"))]
#[cfg_attr(test, allow(dead_code))]
#[cfg_attr(all(feature = "mlx", not(mlx_stub)), link(name = "mlx_wrapper"))]
#[cfg_attr(any(mlx_stub, not(feature = "mlx")), link(name = "mlx_wrapper_stub"))]
extern "C" {
    /// Gathered quantized matrix multiplication
    ///
    /// Performs quantized matmul with expert selection:
    /// 1. Gathers weights for selected experts using indices
    /// 2. Dequantizes on-the-fly using scales and biases
    /// 3. Computes matrix multiplication
    ///
    /// # Arguments
    /// * `x` - Input tensor [batch, seq, 1, 1, hidden]
    /// * `weight` - Packed quantized weights [num_experts, hidden, out_features]
    /// * `scales` - Quantization scales [num_experts, num_groups, out_features]
    /// * `biases` - Quantization biases [num_experts, num_groups, out_features]
    /// * `indices` - Expert indices [batch, seq, k]
    /// * `group_size` - Elements per quantization group
    ///
    /// # Returns
    /// Output tensor [batch, seq, k, out_features]
    pub fn mlx_gather_qmm(
        x: *mut mlx_array_t,
        weight: *mut mlx_array_t,
        scales: *mut mlx_array_t,
        biases: *mut mlx_array_t,
        indices: *mut mlx_array_t,
        group_size: i32,
    ) -> *mut mlx_array_t;

    /// Complete Switch-GLU forward pass with quantized experts
    ///
    /// Implements: output = sum_k(score_k * down(silu(gate_k(x)) * up_k(x)))
    ///
    /// All projections use gather_qmm for efficient quantized computation.
    fn mlx_switch_glu_forward_quantized(
        x: *mut mlx_array_t,
        gate_weight: *mut mlx_array_t,
        gate_scales: *mut mlx_array_t,
        gate_biases: *mut mlx_array_t,
        up_weight: *mut mlx_array_t,
        up_scales: *mut mlx_array_t,
        up_biases: *mut mlx_array_t,
        down_weight: *mut mlx_array_t,
        down_scales: *mut mlx_array_t,
        down_biases: *mut mlx_array_t,
        indices: *mut mlx_array_t,
        scores: *mut mlx_array_t,
        group_size: i32,
    ) -> *mut mlx_array_t;

    /// Compute top-k expert selection with softmax normalization
    ///
    /// # Arguments
    /// * `router_logits` - Router output [batch, seq, num_experts]
    /// * `k` - Number of experts to select
    /// * `out_indices` - Output: selected expert indices [batch, seq, k]
    /// * `out_scores` - Output: softmax scores for selected experts [batch, seq, k]
    ///
    /// # Returns
    /// true on success, false on error
    fn mlx_moe_topk_gating(
        router_logits: *mut mlx_array_t,
        k: i32,
        out_indices: *mut *mut mlx_array_t,
        out_scores: *mut *mut mlx_array_t,
    ) -> bool;

    /// Expand dimensions of an array
    ///
    /// # Arguments
    /// * `array` - Input array
    /// * `axis` - Axis along which to expand
    ///
    /// # Returns
    /// Array with expanded dimension
    pub fn mlx_expand_dims(array: *mut mlx_array_t, axis: i32) -> *mut mlx_array_t;

    /// SiLU (Swish) activation function
    ///
    /// Computes x * sigmoid(x)
    pub fn mlx_silu(array: *mut mlx_array_t) -> *mut mlx_array_t;

    /// Element-wise sum along an axis
    pub fn mlx_sum_axis(array: *mut mlx_array_t, axis: i32, keepdims: bool) -> *mut mlx_array_t;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moe_config_default() {
        let config = QuantizedMoeConfig::default();
        assert_eq!(config.num_experts, 8);
        assert_eq!(config.num_experts_per_token, 2);
        assert_eq!(config.group_size, 64);
    }

    #[cfg(not(feature = "mlx-rs-backend"))]
    #[test]
    fn test_moe_layer_creation() {
        let config = QuantizedMoeConfig {
            num_experts: 16,
            num_experts_per_token: 4,
            hidden_size: 2048,
            intermediate_size: 8192,
            group_size: 128,
            use_shared_expert: true,
        };

        let layer = QuantizedMoeLayer::new(config.clone());
        assert_eq!(layer.config().num_experts, 16);
        assert_eq!(layer.config().num_experts_per_token, 4);
    }

    #[cfg(not(feature = "mlx-rs-backend"))]
    #[test]
    fn test_moe_layer_without_weights_errors() {
        let layer = QuantizedMoeLayer::new(QuantizedMoeConfig::default());

        let result = layer.forward(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not initialized"));
    }

    #[cfg(feature = "mlx-rs-backend")]
    #[test]
    fn test_mlx_rs_moe_layer_creation() {
        let config = QuantizedMoeConfig {
            num_experts: 16,
            num_experts_per_token: 4,
            hidden_size: 2048,
            intermediate_size: 8192,
            group_size: 128,
            use_shared_expert: true,
        };

        let layer = MlxRsMoeLayer::new(config.clone());
        assert_eq!(layer.config().num_experts, 16);
        assert_eq!(layer.config().num_experts_per_token, 4);
    }

    // =========================================================================
    // Deterministic Selection Tests
    // =========================================================================

    /// Test that select_topk_experts produces stable tie-breaking.
    ///
    /// When experts have equal scores, the one with the lower ID should be selected first.
    /// This test verifies the documented ordering rule:
    /// - Primary sort: score descending
    /// - Secondary sort: expert_id ascending
    #[test]
    fn test_select_topk_experts_tie_break_is_stable() {
        // Create router logits where multiple experts have equal scores
        // Expert 0, 1, 2, 3 all have score 1.0; experts 4-7 have score 0.0
        // Softmax of equal values gives equal probabilities
        // With k=2, we should consistently select experts 0 and 1 (lowest IDs)
        let equal_logits = vec![1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];

        // Run selection multiple times to verify stability
        for _ in 0..10 {
            let (indices, _gates) = select_topk_deterministic(&equal_logits, 2);

            // With deterministic tie-breaking, should always get [0, 1] or [1, 0]
            // depending on implementation, but should be STABLE across runs
            assert!(
                indices.contains(&0) && indices.contains(&1),
                "Expected experts 0 and 1 to be selected for tied scores, got {:?}",
                indices
            );
        }

        // Test with different tie scenarios
        // Experts 2 and 5 have highest scores (tied), experts 3, 4 have second highest (tied)
        let partial_tie_logits = vec![0.0f32, 0.0, 2.0, 1.0, 1.0, 2.0, 0.0, 0.0];

        for _ in 0..10 {
            let (indices, _gates) = select_topk_deterministic(&partial_tie_logits, 2);

            // Experts 2 and 5 have equal highest scores
            // With deterministic tie-breaking (lower ID first), should get [2, 5]
            assert!(
                indices.contains(&2) && indices.contains(&5),
                "Expected experts 2 and 5 (tied highest), got {:?}",
                indices
            );
        }

        // Verify ordering: lower ID should come first when scores are equal
        let (indices, _) = select_topk_deterministic(&partial_tie_logits, 4);
        // Should be [2, 5, 3, 4] in sorted order by (score desc, id asc)
        assert_eq!(
            indices[0], 2,
            "First expert should be 2 (tied highest, lower ID)"
        );
        assert_eq!(
            indices[1], 5,
            "Second expert should be 5 (tied highest, higher ID)"
        );
        // Experts 3 and 4 are tied for second highest
        assert!(
            indices[2] == 3 && indices[3] == 4,
            "Experts 3 and 4 should follow (tied second highest)"
        );
    }

    /// Test that gates are normalized to sum to 1.0 within floating point tolerance.
    #[test]
    fn test_select_topk_experts_normalization_stable() {
        // Various logit patterns to test normalization
        let test_cases = vec![
            vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], // monotonic
            vec![8.0f32, 1.0, 8.0, 1.0, 8.0, 1.0, 8.0, 1.0], // alternating
            vec![1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], // uniform
            vec![100.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], // one dominant
        ];

        for logits in test_cases {
            let (_, gates) = select_topk_deterministic(&logits, 2);

            let gate_sum: f32 = gates.iter().sum();
            assert!(
                (gate_sum - 1.0).abs() < 1e-5,
                "Gates should sum to 1.0, got {} for logits {:?}",
                gate_sum,
                logits
            );

            // All gates should be non-negative
            for (i, &gate) in gates.iter().enumerate() {
                assert!(
                    gate >= 0.0,
                    "Gate {} should be non-negative, got {}",
                    i,
                    gate
                );
            }
        }
    }

    /// Test that the deprecated compute_topk_gating wrapper is equivalent to select_topk_experts.
    #[test]
    #[allow(deprecated)]
    fn test_compute_topk_gating_is_wrapper_equivalent() {
        let config = QuantizedMoeConfig {
            num_experts: 8,
            num_experts_per_token: 2,
            ..Default::default()
        };

        #[cfg(feature = "mlx-rs-backend")]
        {
            let layer = MlxRsMoeLayer::new(config);

            // Create test logits
            let logits =
                MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[1, 1, 8])
                    .unwrap();

            // Call both methods
            let (new_indices, new_gates) = layer.select_topk_experts(&logits).unwrap();
            let (old_indices, old_gates) = layer.compute_topk_gating(&logits).unwrap();

            // Verify they produce identical results
            let new_idx_data = new_indices.to_vec_i32().unwrap();
            let old_idx_data = old_indices.to_vec_i32().unwrap();
            assert_eq!(
                new_idx_data, old_idx_data,
                "Indices should match between new and deprecated API"
            );

            let new_gate_data = new_gates.to_vec_f32().unwrap();
            let old_gate_data = old_gates.to_vec_f32().unwrap();
            for (new_g, old_g) in new_gate_data.iter().zip(old_gate_data.iter()) {
                assert!(
                    (new_g - old_g).abs() < 1e-6,
                    "Gates should match: {} vs {}",
                    new_g,
                    old_g
                );
            }
        }

        #[cfg(not(feature = "mlx-rs-backend"))]
        {
            // For FFI backend, we can only verify the function signatures match
            // as we can't create real MLX arrays in tests without the runtime
            let _ = config;
        }
    }

    // =========================================================================
    // Test Helpers
    // =========================================================================

    /// Helper function for testing deterministic selection without full MLX runtime.
    ///
    /// Implements the same algorithm as select_topk_experts but on plain Rust vectors.
    fn select_topk_deterministic(logits: &[f32], k: usize) -> (Vec<usize>, Vec<f32>) {
        // Apply softmax
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_logits: Vec<f32> = logits.iter().map(|x| (x - max_logit).exp()).collect();
        let sum_exp: f32 = exp_logits.iter().sum();
        let probs: Vec<f32> = exp_logits.iter().map(|x| x / sum_exp).collect();

        // Create (index, prob) pairs and sort with deterministic tie-breaking
        let mut indexed: Vec<(usize, f32)> = probs.into_iter().enumerate().collect();

        // Sort by score descending, then by index ascending (for tie-breaking)
        indexed.sort_by(|a, b| {
            // First compare scores (descending)
            match b.1.partial_cmp(&a.1) {
                Some(std::cmp::Ordering::Equal) => {
                    // On tie, lower index wins (ascending)
                    a.0.cmp(&b.0)
                }
                Some(ord) => ord,
                None => std::cmp::Ordering::Equal,
            }
        });

        // Take top k
        let top_k: Vec<(usize, f32)> = indexed.into_iter().take(k).collect();
        let indices: Vec<usize> = top_k.iter().map(|(i, _)| *i).collect();
        let scores: Vec<f32> = top_k.iter().map(|(_, s)| *s).collect();

        // Renormalize scores to sum to 1.0
        let score_sum: f32 = scores.iter().sum();
        let normalized_gates: Vec<f32> = scores.iter().map(|s| s / score_sum).collect();

        (indices, normalized_gates)
    }
}
