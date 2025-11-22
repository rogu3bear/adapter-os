//! Scaled dot-product attention and RoPE (Rotary Position Embeddings) implementation for MLX backend
//!
//! This module implements:
//! - Scaled dot-product attention (SDPA) with multi-head support
//! - Rotary position embeddings (RoPE) for position-aware representations
//! - Attention masking (causal and arbitrary masks)
//! - Numerically stable softmax computation

use crate::{
    mlx_add, mlx_array_copy, mlx_array_data, mlx_array_free, mlx_array_from_data, mlx_array_shape,
    mlx_array_size, mlx_array_t, mlx_clear_error, mlx_divide, mlx_get_last_error, mlx_matmul,
    mlx_mean, mlx_multiply, mlx_sqrt, mlx_sum,
};
use adapteros_core::{AosError, Result};
use std::f32::consts::PI;

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
    device: &str,
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
    let shape = scores.shape().to_vec();

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
fn apply_softmax(logits: &MLXFFITensor) -> Result<MLXFFITensor> {
    let data = logits.to_float_vec()?;
    let shape = logits.shape().to_vec();

    if data.is_empty() {
        return Err(AosError::Validation(
            "Cannot apply softmax to empty tensor".to_string(),
        ));
    }

    let mut result = data.clone();

    // Find max for numerical stability
    let max_val = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Compute exp(x - max)
    for val in &mut result {
        *val = (*val - max_val).exp();
    }

    // Sum and normalize
    let sum: f32 = result.iter().sum();
    if sum <= 0.0 {
        return Err(AosError::Validation(
            "Softmax denominator is non-positive".to_string(),
        ));
    }

    for val in &mut result {
        *val /= sum;
    }

    MLXFFITensor::from_data(&result, shape)
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
    let mut final_shape = q_shape.to_vec();
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
        let rope_freq = RoPEFrequencies::new(4, 10000.0);
        let tensor = MLXFFITensor::from_data(&[1.0, 0.0, 0.0, 1.0], vec![2, 2]).unwrap();

        let result = mlx_rope(&tensor, 0, &rope_freq, "cpu").unwrap();
        let data = result.to_float_vec().unwrap();

        // At position 0, rotation should be identity
        assert!((data[0] - 1.0).abs() < 1e-5);
        assert!((data[1] - 0.0).abs() < 1e-5);
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
}
