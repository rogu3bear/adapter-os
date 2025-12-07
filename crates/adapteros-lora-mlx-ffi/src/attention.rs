//! Scaled dot-product attention and RoPE (Rotary Position Embeddings) implementation for MLX backend
//!
//! This module implements:
//! - Scaled dot-product attention (SDPA) with multi-head support
//! - Rotary position embeddings (RoPE) for position-aware representations
//! - Attention masking (causal and arbitrary masks)
//! - Numerically stable softmax computation

use adapteros_core::{AosError, Result};

use crate::MLXFFITensor;

/// Configuration for scaled dot-product attention
#[derive(Debug, Clone, Copy)]
pub struct AttentionConfig {
    /// Number of attention heads
    pub num_heads: usize,
    /// Dimension per head (d_k = hidden_size / num_heads)
    pub head_dim: usize,
    /// Whether to apply causal mask (for autoregressive models)
    pub causal_mask: bool,
    /// Dropout probability (0.0 = no dropout)
    pub dropout_prob: f32,
    /// Scale factor for attention scores (typically sqrt(d_k))
    pub scale: f32,
}

impl AttentionConfig {
    /// Create attention config from hidden size and number of heads
    pub fn new(hidden_size: usize, num_heads: usize, causal_mask: bool) -> Result<Self> {
        #[allow(clippy::manual_is_multiple_of)]
        if hidden_size % num_heads != 0 {
            return Err(AosError::Validation(format!(
                "hidden_size ({}) must be divisible by num_heads ({})",
                hidden_size, num_heads
            )));
        }

        let head_dim = hidden_size / num_heads;
        let scale = (head_dim as f32).sqrt().recip();

        Ok(Self {
            num_heads,
            head_dim,
            causal_mask,
            dropout_prob: 0.0,
            scale,
        })
    }

    /// Set dropout probability
    pub fn with_dropout(mut self, dropout_prob: f32) -> Self {
        self.dropout_prob = dropout_prob;
        self
    }
}

/// Rotary position embedding frequencies
///
/// Pre-computed for efficient application of RoPE transformations.
#[derive(Debug, Clone)]
pub struct RoPEFrequencies {
    /// Inverse frequencies: [10000^(-2i/d) for i in 0..d/2]
    pub inv_freq: Vec<f32>,
    /// Dimension
    pub dim: usize,
    /// RoPE theta parameter (default 10000.0)
    pub theta: f32,
}

impl RoPEFrequencies {
    /// Create RoPE frequencies for given dimension
    ///
    /// Precomputes inverse frequencies for faster RoPE application
    ///
    /// # Arguments
    /// * `dim` - Head dimension (d_k)
    /// * `theta` - Rotation parameter (typically 10000.0)
    ///
    /// # Returns
    /// RoPE frequencies
    pub fn new(dim: usize, theta: f32) -> Self {
        let mut inv_freq = Vec::with_capacity(dim / 2);

        // Compute inverse frequencies: 10000^(-2i/d)
        for i in 0..dim / 2 {
            let freq = 1.0 / theta.powf((2.0 * i as f32) / dim as f32);
            inv_freq.push(freq);
        }

        Self {
            inv_freq,
            dim,
            theta,
        }
    }

    /// Get dimension
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Get theta parameter
    pub fn theta(&self) -> f32 {
        self.theta
    }
}

/// Apply RoPE (Rotary Position Embeddings) to a tensor
///
/// Applies 2D rotations to consecutive pairs of dimensions based on position:
/// - [x1, x2, ..., xd] -> [x1*cos(m*θ₁) - x2*sin(m*θ₁), x1*sin(m*θ₁) + x2*cos(m*θ₁), ...]
///
/// # Arguments
/// * `tensor` - Input tensor of shape [..., d] where d is head dimension
/// * `position` - Position index in sequence
/// * `rope_freq` - Pre-computed RoPE frequencies
/// * `device` - Device specification (currently unused, for future multi-device support)
///
/// # Returns
/// Tensor with RoPE applied, same shape as input
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::attention::{mlx_rope, RoPEFrequencies};
///
/// let rope_freq = RoPEFrequencies::new(64, 10000.0);
/// let x = MLXFFITensor::from_data(&[...data], vec![1, 1, 64])?;
/// let rotated = mlx_rope(&x, 5, &rope_freq, "gpu")?;
/// ```
pub fn mlx_rope(
    tensor: &MLXFFITensor,
    position: usize,
    rope_freq: &RoPEFrequencies,
    _device: &str,
) -> Result<MLXFFITensor> {
    let shape = tensor.shape().to_vec();

    if shape.is_empty() {
        return Err(AosError::Validation(
            "Input tensor must have at least 1 dimension".to_string(),
        ));
    }

    let last_dim = shape[shape.len() - 1];
    if last_dim != rope_freq.dim {
        return Err(AosError::Validation(format!(
            "Tensor last dimension ({}) must match RoPE dimension ({})",
            last_dim, rope_freq.dim
        )));
    }

    let data = tensor.to_float_vec()?;
    let mut output = data.clone();

    // Apply RoPE rotations to each element
    for i in 0..rope_freq.inv_freq.len() {
        let theta = position as f32 * rope_freq.inv_freq[i];
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();

        // For each position in the data that corresponds to this dimension pair
        let stride = rope_freq.dim;
        for j in (0..data.len()).step_by(stride) {
            let idx_0 = j + 2 * i;
            let idx_1 = j + 2 * i + 1;

            if idx_0 < output.len() && idx_1 < output.len() {
                let x0 = output[idx_0];
                let x1 = output[idx_1];

                output[idx_0] = x0 * cos_theta - x1 * sin_theta;
                output[idx_1] = x0 * sin_theta + x1 * cos_theta;
            }
        }
    }

    MLXFFITensor::from_data(&output, shape)
}

/// Scaled dot-product attention (SDPA)
///
/// Implements the core attention mechanism:
/// 1. Compute attention scores: scores = Q @ K^T / sqrt(d_k)
/// 2. Apply mask if provided
/// 3. Apply softmax
/// 4. Multiply by values: output = softmax(scores) @ V
///
/// # Arguments
/// * `query` - Query tensor of shape [batch, seq_len, num_heads, head_dim]
/// * `key` - Key tensor of shape [batch, kv_seq_len, num_heads, head_dim]
/// * `value` - Value tensor of shape [batch, kv_seq_len, num_heads, head_dim]
/// * `config` - Attention configuration
/// * `mask` - Optional attention mask of shape [1, 1, seq_len, kv_seq_len] or compatible
///
/// # Returns
/// Output tensor of shape [batch, seq_len, num_heads, head_dim]
///
/// # Notes
/// - For multi-head attention, tensors should be pre-reshaped
/// - Mask values should be 0.0 for attend, -inf for mask out
/// - Numerically stable: uses max subtraction before softmax
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::attention::{mlx_scaled_dot_product_attention, AttentionConfig};
///
/// let config = AttentionConfig::new(256, 4, true)?;
/// let query = MLXFFITensor::from_data(&[...], vec![1, 10, 4, 64])?;
/// let key = MLXFFITensor::from_data(&[...], vec![1, 10, 4, 64])?;
/// let value = MLXFFITensor::from_data(&[...], vec![1, 10, 4, 64])?;
///
/// let output = mlx_scaled_dot_product_attention(&query, &key, &value, &config, None)?;
/// ```
pub fn mlx_scaled_dot_product_attention(
    query: &MLXFFITensor,
    key: &MLXFFITensor,
    value: &MLXFFITensor,
    config: &AttentionConfig,
    mask: Option<&MLXFFITensor>,
) -> Result<MLXFFITensor> {
    let q_shape = query.shape();
    let k_shape = key.shape();
    let v_shape = value.shape();

    // Validate shapes
    if q_shape.len() < 2 || k_shape.len() < 2 || v_shape.len() < 2 {
        return Err(AosError::Validation(
            "Query, key, and value must have at least 2 dimensions".to_string(),
        ));
    }

    // Ensure shapes are compatible
    let seq_len = q_shape[q_shape.len() - 2];
    let d_k = q_shape[q_shape.len() - 1];

    if k_shape[k_shape.len() - 1] != d_k {
        return Err(AosError::Validation(format!(
            "Key dimension ({}) must match query dimension ({})",
            k_shape[k_shape.len() - 1],
            d_k
        )));
    }

    if v_shape[v_shape.len() - 1] != d_k {
        return Err(AosError::Validation(format!(
            "Value dimension ({}) must match query dimension ({})",
            v_shape[v_shape.len() - 1],
            d_k
        )));
    }

    // Step 1: Compute Q @ K^T / sqrt(d_k)
    let k_transposed = key.transpose()?;
    let scores = query.matmul(&k_transposed)?;

    // Scale by 1/sqrt(d_k)
    let scale_tensor = MLXFFITensor::from_data(&[config.scale], vec![1])?;
    let scores = scores.multiply(&scale_tensor)?;

    // Step 2: Apply attention mask if provided
    let scores = if let Some(mask_tensor) = mask {
        apply_attention_mask(&scores, mask_tensor)?
    } else if config.causal_mask {
        apply_causal_mask(&scores, seq_len)?
    } else {
        scores
    };

    // Step 3: Apply softmax
    let attention_weights = apply_softmax(&scores)?;

    // Step 4: Multiply by V
    let output = attention_weights.matmul(value)?;

    Ok(output)
}

/// Apply attention mask to scores
///
/// Adds mask values to attention scores before softmax.
/// Mask should have -inf for positions to mask out, 0.0 for positions to attend.
fn apply_attention_mask(scores: &MLXFFITensor, mask: &MLXFFITensor) -> Result<MLXFFITensor> {
    let score_shape = scores.shape();
    let mask_shape = mask.shape();

    // Validate mask shape is broadcastable
    if mask_shape.len() > score_shape.len() {
        return Err(AosError::Validation(format!(
            "Mask dimensions ({}) cannot exceed score dimensions ({})",
            mask_shape.len(),
            score_shape.len()
        )));
    }

    // Add mask to scores
    scores.add(mask)
}

/// Apply causal mask for autoregressive attention
///
/// Creates a lower-triangular mask that prevents attention to future positions.
fn apply_causal_mask(scores: &MLXFFITensor, seq_len: usize) -> Result<MLXFFITensor> {
    // Create causal mask: lower triangular matrix with -inf above diagonal
    let mut mask_data = vec![0.0; seq_len * seq_len];

    for i in 0..seq_len {
        for j in 0..seq_len {
            if j > i {
                mask_data[i * seq_len + j] = f32::NEG_INFINITY;
            }
        }
    }

    let mask = MLXFFITensor::from_data(&mask_data, vec![seq_len, seq_len])?;

    // Broadcast and apply
    apply_attention_mask(scores, &mask)
}

/// Apply softmax with numerical stability
///
/// Uses the log-sum-exp trick: softmax(x) = exp(x - max(x)) / sum(exp(x - max(x)))
/// On aarch64, uses SIMD-optimized implementation with NEON intrinsics.
fn apply_softmax(logits: &MLXFFITensor) -> Result<MLXFFITensor> {
    let data = logits.to_float_vec()?;
    let shape = logits.shape().to_vec();

    if data.is_empty() {
        return Err(AosError::Validation(
            "Cannot apply softmax to empty tensor".to_string(),
        ));
    }

    let mut result = data.clone();

    // Use SIMD-optimized path on aarch64, scalar fallback otherwise
    #[cfg(target_arch = "aarch64")]
    {
        simd_softmax_inplace(&mut result);
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        scalar_softmax_inplace(&mut result)?;
    }

    MLXFFITensor::from_data(&result, shape)
}

/// Scalar softmax implementation (fallback for non-aarch64)
#[cfg(not(target_arch = "aarch64"))]
fn scalar_softmax_inplace(data: &mut [f32]) -> Result<()> {
    // Find max for numerical stability
    let max_val = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Compute exp(x - max)
    for val in data.iter_mut() {
        *val = (*val - max_val).exp();
    }

    // Sum and normalize
    let sum: f32 = data.iter().sum();
    if sum <= 0.0 {
        return Err(AosError::Validation(
            "Softmax denominator is non-positive".to_string(),
        ));
    }

    for val in data.iter_mut() {
        *val /= sum;
    }

    Ok(())
}

/// SIMD-optimized softmax using NEON intrinsics on aarch64
///
/// Uses polynomial approximation for exp() with ~1e-4 accuracy.
/// This is faster than calling libm's expf() and sufficient for ML inference.
///
/// # Safety considerations
/// - Handles arrays of any size (falls back to scalar for len < 4)
/// - Handles underflow (exp(x) → 0 for x < -87)
/// - Handles all-same-value inputs (uniform distribution)
/// - Numerical stability via max subtraction before exp
#[cfg(target_arch = "aarch64")]
fn simd_softmax_inplace(data: &mut [f32]) {
    use std::arch::aarch64::*;

    let len = data.len();

    // Fall back to scalar for small arrays
    if len < 4 {
        scalar_softmax_inplace_impl(data);
        return;
    }

    // Step 1: Find max (vectorized)
    let mut max_val = f32::NEG_INFINITY;

    // Process 4 elements at a time
    for chunk in data.chunks_exact(4) {
        let v = unsafe { vld1q_f32(chunk.as_ptr()) };
        let chunk_max = unsafe { vmaxvq_f32(v) };
        max_val = max_val.max(chunk_max);
    }

    // Handle remainder
    for &val in data.chunks_exact(4).remainder() {
        max_val = max_val.max(val);
    }

    // Handle edge case: all -inf (shouldn't happen in practice)
    if max_val == f32::NEG_INFINITY {
        // All values are -inf, set uniform distribution
        let uniform = 1.0 / len as f32;
        for val in data.iter_mut() {
            *val = uniform;
        }
        return;
    }

    // Step 2: Compute exp(x - max) and accumulate sum (vectorized)
    let max_splat = unsafe { vdupq_n_f32(max_val) };
    let mut sum_vec = unsafe { vdupq_n_f32(0.0) };

    for chunk in data.chunks_exact_mut(4) {
        let v = unsafe { vld1q_f32(chunk.as_ptr()) };
        // Subtract max (now all values are <= 0)
        let shifted = unsafe { vsubq_f32(v, max_splat) };
        // Polynomial exp approximation with underflow handling
        let exp_v = simd_exp_approx_neon_safe(shifted);
        // Store back
        unsafe { vst1q_f32(chunk.as_mut_ptr(), exp_v) };
        // Accumulate sum
        sum_vec = unsafe { vaddq_f32(sum_vec, exp_v) };
    }

    // Reduce sum vector to scalar
    let mut sum = unsafe { vaddvq_f32(sum_vec) };

    // Handle remainder (scalar)
    for val in data.chunks_exact_mut(4).into_remainder() {
        *val = fast_exp_safe(*val - max_val);
        sum += *val;
    }

    // Step 3: Normalize (vectorized)
    // sum > 0 is guaranteed since at least one exp(0) = 1 exists (the max element)
    let inv_sum = 1.0 / sum;
    let inv_sum_splat = unsafe { vdupq_n_f32(inv_sum) };

    for chunk in data.chunks_exact_mut(4) {
        let v = unsafe { vld1q_f32(chunk.as_ptr()) };
        let normalized = unsafe { vmulq_f32(v, inv_sum_splat) };
        unsafe { vst1q_f32(chunk.as_mut_ptr(), normalized) };
    }

    // Handle remainder
    for val in data.chunks_exact_mut(4).into_remainder() {
        *val *= inv_sum;
    }
}

/// Scalar softmax implementation used as fallback
#[cfg(target_arch = "aarch64")]
fn scalar_softmax_inplace_impl(data: &mut [f32]) {
    if data.is_empty() {
        return;
    }

    // Find max
    let max_val = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    if max_val == f32::NEG_INFINITY {
        let uniform = 1.0 / data.len() as f32;
        for val in data.iter_mut() {
            *val = uniform;
        }
        return;
    }

    // Compute exp(x - max) and sum
    let mut sum = 0.0f32;
    for val in data.iter_mut() {
        *val = fast_exp_safe(*val - max_val);
        sum += *val;
    }

    // Normalize
    let inv_sum = 1.0 / sum;
    for val in data.iter_mut() {
        *val *= inv_sum;
    }
}

/// NEON vectorized exp approximation with underflow/overflow handling
///
/// Uses a degree-5 polynomial approximation for exp(x).
/// - For x < -87.3: returns 0 (underflow)
/// - For x > 88.7: returns inf (overflow, shouldn't happen in softmax)
/// - Accuracy: ~1e-5 relative error in normal range
///
/// Formula: exp(x) = 2^(x * log2(e)) = 2^n * 2^f where n = floor(x*log2(e)), f = frac
#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn simd_exp_approx_neon_safe(
    x: std::arch::aarch64::float32x4_t,
) -> std::arch::aarch64::float32x4_t {
    use std::arch::aarch64::*;

    // Constants
    const LOG2E: f32 = std::f32::consts::LOG2_E;
    const LN2: f32 = std::f32::consts::LN_2;

    // Underflow threshold: exp(-87.3) ≈ 1e-38 (smallest normal f32)
    const EXP_UNDERFLOW: f32 = -87.3;

    // Polynomial coefficients for exp(x) on [-ln2/2, ln2/2]
    // exp(x) ≈ 1 + x + x²/2! + x³/3! + x⁴/4! + x⁵/5!
    // These are refined coefficients from Cephes library
    const C1: f32 = 1.0;
    const C2: f32 = 1.0;
    const C3: f32 = 0.5;
    const C4: f32 = 0.16666666666666666; // 1/6
    const C5: f32 = 0.041666666666666664; // 1/24
    const C6: f32 = 0.008333333333333333; // 1/120

    unsafe {
        // Check for underflow
        let underflow_mask = vcltq_f32(x, vdupq_n_f32(EXP_UNDERFLOW));

        // Compute n = round(x * log2(e)) and r = x - n * ln(2)
        // This keeps r in [-ln2/2, ln2/2] for better polynomial accuracy
        let log2e = vdupq_n_f32(LOG2E);
        let scaled = vmulq_f32(x, log2e);
        let n = vrndnq_f32(scaled); // Round to nearest
        let ln2 = vdupq_n_f32(LN2);
        let r = vmlsq_f32(x, n, ln2); // r = x - n * ln2

        // Polynomial evaluation using Horner's method: exp(r) for small r
        // p(r) = 1 + r * (1 + r * (0.5 + r * (1/6 + r * (1/24 + r * 1/120))))
        let c6 = vdupq_n_f32(C6);
        let c5 = vdupq_n_f32(C5);
        let c4 = vdupq_n_f32(C4);
        let c3 = vdupq_n_f32(C3);
        let c2 = vdupq_n_f32(C2);
        let c1 = vdupq_n_f32(C1);

        let mut poly = vfmaq_f32(c5, c6, r); // c5 + c6*r
        poly = vfmaq_f32(c4, poly, r); // c4 + poly*r
        poly = vfmaq_f32(c3, poly, r); // c3 + poly*r
        poly = vfmaq_f32(c2, poly, r); // c2 + poly*r
        poly = vfmaq_f32(c1, poly, r); // c1 + poly*r = exp(r)

        // Compute 2^n using IEEE 754 bit manipulation
        // 2^n = float with exponent (n + 127) and mantissa 1.0
        let n_int = vcvtq_s32_f32(n);
        let bias = vdupq_n_s32(127);
        let exp_bits = vaddq_s32(n_int, bias);

        // Clamp to valid exponent range [0, 254] to avoid garbage
        let zero = vdupq_n_s32(0);
        let max_exp = vdupq_n_s32(254);
        let exp_clamped = vmaxq_s32(vminq_s32(exp_bits, max_exp), zero);

        let exp_shifted = vshlq_n_s32::<23>(exp_clamped);
        let pow2n = vreinterpretq_f32_s32(exp_shifted);

        // Result = 2^n * exp(r)
        let result = vmulq_f32(poly, pow2n);

        // Apply underflow mask: if x < -87.3, return 0
        vbslq_f32(
            vreinterpretq_u32_s32(vreinterpretq_s32_u32(underflow_mask)),
            vdupq_n_f32(0.0),
            result,
        )
    }
}

/// Fast scalar exp approximation with underflow handling
///
/// Uses the same polynomial approach as SIMD version.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn fast_exp_safe(x: f32) -> f32 {
    // Underflow check
    if x < -87.3 {
        return 0.0;
    }

    const LOG2E: f32 = std::f32::consts::LOG2_E;
    const LN2: f32 = std::f32::consts::LN_2;
    const C1: f32 = 1.0;
    const C2: f32 = 1.0;
    const C3: f32 = 0.5;
    const C4: f32 = 0.16666666666666666;
    const C5: f32 = 0.041666666666666664;
    const C6: f32 = 0.008333333333333333;

    // Range reduction: x = n * ln(2) + r where |r| <= ln(2)/2
    let n = (x * LOG2E).round();
    let r = x - n * LN2;

    // Polynomial for exp(r)
    let poly = C1 + r * (C2 + r * (C3 + r * (C4 + r * (C5 + r * C6))));

    // 2^n via bit manipulation with clamping
    let n_int = n as i32;
    let exp_bits = (n_int + 127).clamp(0, 254);
    let pow2n = f32::from_bits((exp_bits as u32) << 23);

    poly * pow2n
}

/// Multi-head scaled dot-product attention (convenience wrapper)
///
/// Automatically handles reshaping for multi-head computation.
///
/// # Arguments
/// * `query` - Query tensor of shape [batch, seq_len, hidden_size]
/// * `key` - Key tensor of shape [batch, kv_seq_len, hidden_size]
/// * `value` - Value tensor of shape [batch, kv_seq_len, hidden_size]
/// * `num_heads` - Number of attention heads
/// * `causal_mask` - Whether to apply causal mask
///
/// # Returns
/// Output tensor of shape [batch, seq_len, hidden_size]
pub fn mlx_multihead_attention(
    query: &MLXFFITensor,
    key: &MLXFFITensor,
    value: &MLXFFITensor,
    num_heads: usize,
    causal_mask: bool,
) -> Result<MLXFFITensor> {
    let q_shape = query.shape();
    let hidden_size = q_shape[q_shape.len() - 1];

    let config = AttentionConfig::new(hidden_size, num_heads, causal_mask)?;

    // Reshape for multi-head attention
    // [batch, seq_len, hidden_size] -> [batch, seq_len, num_heads, head_dim]
    let mut q_reshaped = q_shape.to_vec();
    q_reshaped.pop(); // Remove hidden_size
    q_reshaped.push(num_heads);
    q_reshaped.push(config.head_dim);

    let query_reshaped = query.reshape(q_reshaped)?;

    let k_shape = key.shape();
    let mut k_reshaped = k_shape.to_vec();
    k_reshaped.pop();
    k_reshaped.push(num_heads);
    k_reshaped.push(config.head_dim);

    let key_reshaped = key.reshape(k_reshaped)?;

    let v_shape = value.shape();
    let mut v_reshaped = v_shape.to_vec();
    v_reshaped.pop();
    v_reshaped.push(num_heads);
    v_reshaped.push(config.head_dim);

    let value_reshaped = value.reshape(v_reshaped)?;

    // Apply SDPA
    let output = mlx_scaled_dot_product_attention(
        &query_reshaped,
        &key_reshaped,
        &value_reshaped,
        &config,
        None,
    )?;

    // Reshape back to [batch, seq_len, hidden_size]
    let final_shape = q_shape.to_vec();
    output.reshape(final_shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rope_frequencies_creation() {
        let rope = RoPEFrequencies::new(64, 10000.0);
        assert_eq!(rope.dim, 64);
        assert_eq!(rope.inv_freq.len(), 32);
        assert_eq!(rope.theta, 10000.0);

        // First frequency should be 1.0 (10000^0)
        assert!((rope.inv_freq[0] - 1.0).abs() < 1e-5);

        // Last frequency should be smaller
        assert!(rope.inv_freq[31] < rope.inv_freq[0]);
    }

    #[test]
    fn test_attention_config_new() {
        let config = AttentionConfig::new(256, 4, true).unwrap();
        assert_eq!(config.num_heads, 4);
        assert_eq!(config.head_dim, 64);
        assert!(config.causal_mask);
        assert!((config.scale - (64.0_f32).sqrt().recip()).abs() < 1e-5);
    }

    #[test]
    fn test_attention_config_invalid() {
        let result = AttentionConfig::new(256, 5, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("divisible"));
    }

    #[test]
    fn test_attention_config_with_dropout() {
        let config = AttentionConfig::new(256, 4, false)
            .unwrap()
            .with_dropout(0.1);
        assert_eq!(config.dropout_prob, 0.1);
    }

    #[test]
    fn test_rope_application() {
        // RoPE dimension must match tensor's last dimension
        let rope_freq = RoPEFrequencies::new(2, 10000.0);
        let tensor = MLXFFITensor::from_data(&[1.0, 0.0, 0.0, 1.0], vec![2, 2]).unwrap();

        let result = mlx_rope(&tensor, 0, &rope_freq, "cpu").unwrap();
        let data = result.to_float_vec().unwrap();

        // At position 0, theta = 0 * inv_freq = 0, so cos(0)=1, sin(0)=0
        // Rotation is identity: [x0, x1] -> [x0*1 - x1*0, x0*0 + x1*1] = [x0, x1]
        assert!((data[0] - 1.0).abs() < 1e-5);
        assert!((data[1] - 0.0).abs() < 1e-5);
        assert!((data[2] - 0.0).abs() < 1e-5);
        assert!((data[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_rope_nonzero_position() {
        // Test RoPE at position 1 to verify actual rotation occurs
        // For dim=2, theta=10000.0: inv_freq[0] = 1.0 / 10000^(0/2) = 1.0
        // At position 1: theta = 1.0 radian
        // cos(1.0) ≈ 0.5403, sin(1.0) ≈ 0.8415
        let rope_freq = RoPEFrequencies::new(2, 10000.0);
        let tensor = MLXFFITensor::from_data(&[1.0, 0.0, 0.0, 1.0], vec![2, 2]).unwrap();

        let result = mlx_rope(&tensor, 1, &rope_freq, "cpu").unwrap();
        let data = result.to_float_vec().unwrap();

        let cos_1 = 1.0_f32.cos();
        let sin_1 = 1.0_f32.sin();

        // First pair [1.0, 0.0] -> [1*cos - 0*sin, 1*sin + 0*cos] = [cos, sin]
        assert!(
            (data[0] - cos_1).abs() < 1e-5,
            "data[0]={} expected {}",
            data[0],
            cos_1
        );
        assert!(
            (data[1] - sin_1).abs() < 1e-5,
            "data[1]={} expected {}",
            data[1],
            sin_1
        );

        // Second pair [0.0, 1.0] -> [0*cos - 1*sin, 0*sin + 1*cos] = [-sin, cos]
        assert!(
            (data[2] - (-sin_1)).abs() < 1e-5,
            "data[2]={} expected {}",
            data[2],
            -sin_1
        );
        assert!(
            (data[3] - cos_1).abs() < 1e-5,
            "data[3]={} expected {}",
            data[3],
            cos_1
        );
    }

    #[test]
    fn test_rope_higher_position() {
        // Test at position 5 to verify rotation accumulates correctly
        let rope_freq = RoPEFrequencies::new(2, 10000.0);
        let tensor = MLXFFITensor::from_data(&[1.0, 0.0], vec![2]).unwrap();

        let result = mlx_rope(&tensor, 5, &rope_freq, "cpu").unwrap();
        let data = result.to_float_vec().unwrap();

        // theta = 5 * 1.0 = 5.0 radians
        let cos_5 = 5.0_f32.cos();
        let sin_5 = 5.0_f32.sin();

        assert!(
            (data[0] - cos_5).abs() < 1e-5,
            "data[0]={} expected {}",
            data[0],
            cos_5
        );
        assert!(
            (data[1] - sin_5).abs() < 1e-5,
            "data[1]={} expected {}",
            data[1],
            sin_5
        );
    }

    #[test]
    fn test_rope_invalid_dimension() {
        let rope_freq = RoPEFrequencies::new(4, 10000.0);
        let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0], vec![3]).unwrap();

        let result = mlx_rope(&tensor, 0, &rope_freq, "cpu");
        assert!(result.is_err());
    }

    #[test]
    fn test_softmax_basic() {
        let logits = MLXFFITensor::from_data(&[1.0, 2.0, 3.0], vec![3]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        // Probabilities should sum to 1
        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);

        // All values should be positive
        for val in &data {
            assert!(*val > 0.0);
        }

        // Larger logits should have higher probability
        assert!(data[2] > data[1]);
        assert!(data[1] > data[0]);
    }

    #[test]
    fn test_softmax_numerical_stability() {
        // Large logits that would overflow without stability
        let logits = MLXFFITensor::from_data(&[1000.0, 1001.0, 999.0], vec![3]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        // Should not contain NaN or Inf
        for val in &data {
            assert!(val.is_finite());
        }

        // Sum should still be 1
        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_causal_mask_creation() {
        let scores = MLXFFITensor::from_data(&[1.0; 9], vec![3, 3]).unwrap();
        let masked = apply_causal_mask(&scores, 3).unwrap();
        let data = masked.to_float_vec().unwrap();

        // Check causal pattern
        assert!(data[0].is_finite()); // (0,0) should be unmasked
        assert!(data[1].is_infinite()); // (0,1) should be masked
        assert!(data[2].is_infinite()); // (0,2) should be masked

        assert!(data[3].is_finite()); // (1,0) should be unmasked
        assert!(data[4].is_finite()); // (1,1) should be unmasked
        assert!(data[5].is_infinite()); // (1,2) should be masked
    }

    // =========================================================================
    // SIMD SOFTMAX ACCURACY TESTS
    // =========================================================================

    /// Test that fast_exp_safe matches libm::expf within tolerance
    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_fast_exp_accuracy() {
        // Test across the valid range for softmax (typically -87 to ~10 after max subtraction)
        let test_values = [
            0.0, 0.5, 1.0, -0.5, -1.0, -5.0, -10.0, -20.0, -50.0, -80.0, -87.0, 2.0, 3.0, 5.0,
            10.0, // Some positive values
        ];

        for &x in &test_values {
            let fast = fast_exp_safe(x);
            let reference = x.exp();

            // Allow ~1e-4 relative error for normal range
            if reference > 1e-30 {
                let rel_error = ((fast - reference) / reference).abs();
                assert!(
                    rel_error < 1e-4,
                    "fast_exp_safe({}) = {}, expected {}, rel_error = {}",
                    x,
                    fast,
                    reference,
                    rel_error
                );
            } else {
                // For very small values, just check both are tiny
                assert!(
                    fast < 1e-30,
                    "fast_exp_safe({}) = {} should be tiny",
                    x,
                    fast
                );
            }
        }
    }

    /// Test underflow handling in fast_exp
    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_fast_exp_underflow() {
        // Below -87.3, exp(x) underflows to 0
        let underflow_values = [-88.0, -100.0, -200.0, f32::NEG_INFINITY];

        for &x in &underflow_values {
            let result = fast_exp_safe(x);
            assert_eq!(
                result, 0.0,
                "fast_exp_safe({}) should be 0, got {}",
                x, result
            );
        }
    }

    /// Test softmax on small arrays (triggers scalar fallback)
    #[test]
    fn test_softmax_small_array() {
        // len = 1
        let logits1 = MLXFFITensor::from_data(&[5.0], vec![1]).unwrap();
        let result1 = apply_softmax(&logits1).unwrap();
        let data1 = result1.to_float_vec().unwrap();
        assert!(
            (data1[0] - 1.0).abs() < 1e-6,
            "Single element softmax should be 1.0"
        );

        // len = 2
        let logits2 = MLXFFITensor::from_data(&[1.0, 2.0], vec![2]).unwrap();
        let result2 = apply_softmax(&logits2).unwrap();
        let data2 = result2.to_float_vec().unwrap();
        let sum2: f32 = data2.iter().sum();
        assert!(
            (sum2 - 1.0).abs() < 1e-5,
            "Softmax sum should be 1.0, got {}",
            sum2
        );
        assert!(data2[1] > data2[0], "Higher logit should have higher prob");

        // len = 3
        let logits3 = MLXFFITensor::from_data(&[1.0, 2.0, 3.0], vec![3]).unwrap();
        let result3 = apply_softmax(&logits3).unwrap();
        let data3 = result3.to_float_vec().unwrap();
        let sum3: f32 = data3.iter().sum();
        assert!((sum3 - 1.0).abs() < 1e-5);
    }

    /// Test softmax on arrays that use SIMD path (len >= 4)
    #[test]
    fn test_softmax_simd_path() {
        // Exact multiple of 4
        let logits4 = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![4]).unwrap();
        let result4 = apply_softmax(&logits4).unwrap();
        let data4 = result4.to_float_vec().unwrap();
        let sum4: f32 = data4.iter().sum();
        assert!(
            (sum4 - 1.0).abs() < 1e-5,
            "SIMD softmax sum should be 1.0, got {}",
            sum4
        );

        // Not a multiple of 4 (tests remainder handling)
        let logits7 =
            MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0], vec![7]).unwrap();
        let result7 = apply_softmax(&logits7).unwrap();
        let data7 = result7.to_float_vec().unwrap();
        let sum7: f32 = data7.iter().sum();
        assert!(
            (sum7 - 1.0).abs() < 1e-5,
            "SIMD softmax with remainder sum should be 1.0, got {}",
            sum7
        );

        // Verify ordering is preserved
        for i in 1..data7.len() {
            assert!(
                data7[i] > data7[i - 1],
                "Higher logit should have higher prob"
            );
        }
    }

    /// Test softmax with uniform input (all same values)
    #[test]
    fn test_softmax_uniform_input() {
        // All same values should give uniform distribution
        let logits = MLXFFITensor::from_data(&[3.0, 3.0, 3.0, 3.0, 3.0], vec![5]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        let expected = 1.0 / 5.0;
        for (i, &val) in data.iter().enumerate() {
            assert!(
                (val - expected).abs() < 1e-5,
                "Uniform softmax[{}] = {}, expected {}",
                i,
                val,
                expected
            );
        }
    }

    /// Test softmax numerical stability with extreme value spread
    #[test]
    fn test_softmax_extreme_spread() {
        // One very large, others very small
        let logits = MLXFFITensor::from_data(&[-100.0, 0.0, 100.0, -100.0], vec![4]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        // The max element (100.0) should dominate
        assert!(
            data[2] > 0.99,
            "Max element should have prob ~1.0, got {}",
            data[2]
        );

        // Sum should still be 1
        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "Sum should be 1.0, got {}", sum);

        // No NaN or Inf
        for (i, &val) in data.iter().enumerate() {
            assert!(
                val.is_finite(),
                "softmax[{}] should be finite, got {}",
                i,
                val
            );
        }
    }

    /// Test SIMD softmax accuracy against scalar reference
    #[test]
    fn test_simd_vs_scalar_accuracy() {
        // Generate test data
        let input: Vec<f32> = (0..16).map(|i| (i as f32 * 0.5) - 4.0).collect();

        // Apply softmax
        let logits = MLXFFITensor::from_data(&input, vec![16]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let simd_data = result.to_float_vec().unwrap();

        // Compute reference softmax manually
        let max_val = input.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_vals: Vec<f32> = input.iter().map(|&x| (x - max_val).exp()).collect();
        let sum: f32 = exp_vals.iter().sum();
        let reference: Vec<f32> = exp_vals.iter().map(|&x| x / sum).collect();

        // Compare
        for i in 0..16 {
            let rel_error = if reference[i] > 1e-10 {
                ((simd_data[i] - reference[i]) / reference[i]).abs()
            } else {
                simd_data[i].abs()
            };
            assert!(
                rel_error < 1e-4,
                "SIMD softmax[{}] = {}, reference = {}, rel_error = {}",
                i,
                simd_data[i],
                reference[i],
                rel_error
            );
        }
    }

    /// Test softmax doesn't produce NaN with pathological inputs
    #[test]
    fn test_softmax_no_nan() {
        // Mixed inf and regular values
        let logits =
            MLXFFITensor::from_data(&[f32::NEG_INFINITY, 0.0, 1.0, f32::NEG_INFINITY], vec![4])
                .unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        for (i, &val) in data.iter().enumerate() {
            assert!(
                !val.is_nan(),
                "softmax[{}] should not be NaN, got {}",
                i,
                val
            );
        }

        // Sum should be 1 (only the finite elements contribute)
        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "Sum should be 1.0, got {}", sum);
    }

    /// Test large array to ensure SIMD loop handles many iterations
    #[test]
    fn test_softmax_large_array() {
        let size = 1024;
        let input: Vec<f32> = (0..size).map(|i| ((i % 100) as f32 - 50.0) * 0.1).collect();

        let logits = MLXFFITensor::from_data(&input, vec![size]).unwrap();
        let result = apply_softmax(&logits).unwrap();
        let data = result.to_float_vec().unwrap();

        // All finite
        for (i, &val) in data.iter().enumerate() {
            assert!(
                val.is_finite() && val >= 0.0,
                "softmax[{}] should be non-negative finite, got {}",
                i,
                val
            );
        }

        // Sum is 1
        let sum: f32 = data.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "Large array softmax sum should be 1.0, got {}",
            sum
        );
    }
}
