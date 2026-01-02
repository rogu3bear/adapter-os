//! Multi-Head Attention Layer
//!
//! Provides multi-head self-attention with optional ANE acceleration
//! for softmax operations.

use crate::{Array, Result, MlxError};

/// Multi-Head Self-Attention Layer
///
/// Implements scaled dot-product attention with multiple heads:
/// ```text
/// Attention(Q, K, V) = softmax(QK^T / sqrt(d_k)) * V
/// ```
///
/// # ANE Acceleration
///
/// When an `AneAccelerator` is provided and batch size >= threshold,
/// the softmax operation can be delegated to the Neural Engine.
///
/// # Example
///
/// ```ignore
/// let attn = MultiHeadAttention::new(512, 8)?;
/// let output = attn.forward(&hidden_states, None, None)?;
/// ```
#[derive(Debug, Clone)]
pub struct MultiHeadAttention {
    /// Query projection weights [hidden_dim, hidden_dim]
    pub wq: Array,
    /// Key projection weights [hidden_dim, hidden_dim]
    pub wk: Array,
    /// Value projection weights [hidden_dim, hidden_dim]
    pub wv: Array,
    /// Output projection weights [hidden_dim, hidden_dim]
    pub wo: Array,
    /// Number of attention heads
    pub n_heads: i32,
    /// Hidden dimension
    pub hidden_dim: i32,
    /// Head dimension (hidden_dim / n_heads)
    pub head_dim: i32,
    /// Scale factor for attention scores (1 / sqrt(head_dim))
    pub scale: f32,
}

impl MultiHeadAttention {
    /// Create a new MultiHeadAttention layer with identity-like initialization
    ///
    /// Note: For actual use, you should load pretrained weights.
    pub fn new(hidden_dim: i32, n_heads: i32) -> Result<Self> {
        if hidden_dim % n_heads != 0 {
            return Err(MlxError::ArrayOp(
                format!("hidden_dim {} must be divisible by n_heads {}", hidden_dim, n_heads)
            ));
        }

        let head_dim = hidden_dim / n_heads;
        let scale = 1.0 / (head_dim as f32).sqrt();

        // Initialize with small random-like values (simplified for now)
        // Real implementations should use proper initialization
        let wq = Array::ones(&[hidden_dim, hidden_dim])?.scale(0.01)?;
        let wk = Array::ones(&[hidden_dim, hidden_dim])?.scale(0.01)?;
        let wv = Array::ones(&[hidden_dim, hidden_dim])?.scale(0.01)?;
        let wo = Array::ones(&[hidden_dim, hidden_dim])?.scale(0.01)?;

        Ok(Self {
            wq,
            wk,
            wv,
            wo,
            n_heads,
            hidden_dim,
            head_dim,
            scale,
        })
    }

    /// Create MultiHeadAttention with custom weights
    pub fn from_weights(
        wq: Array,
        wk: Array,
        wv: Array,
        wo: Array,
        n_heads: i32,
    ) -> Result<Self> {
        let shape = wq.shape();
        if shape.len() != 2 || shape[0] != shape[1] {
            return Err(MlxError::ArrayOp(
                format!("Weight matrices must be square, got {:?}", shape)
            ));
        }

        let hidden_dim = shape[0];
        if hidden_dim % n_heads != 0 {
            return Err(MlxError::ArrayOp(
                format!("hidden_dim {} must be divisible by n_heads {}", hidden_dim, n_heads)
            ));
        }

        let head_dim = hidden_dim / n_heads;
        let scale = 1.0 / (head_dim as f32).sqrt();

        Ok(Self {
            wq,
            wk,
            wv,
            wo,
            n_heads,
            hidden_dim,
            head_dim,
            scale,
        })
    }

    /// Forward pass for self-attention
    ///
    /// # Arguments
    /// * `x` - Input tensor [batch, seq_len, hidden_dim]
    /// * `mask` - Optional attention mask [batch, 1, seq_len, seq_len]
    /// * `_ane_accel` - Reserved for future ANE acceleration
    ///
    /// # Returns
    /// Output tensor [batch, seq_len, hidden_dim]
    pub fn forward(
        &self,
        x: &Array,
        mask: Option<&Array>,
        _ane_accel: Option<&()>,
    ) -> Result<Array> {
        let shape = x.shape();
        if shape.len() != 3 {
            return Err(MlxError::ArrayOp(
                format!("Input must be 3D [batch, seq_len, hidden], got {:?}", shape)
            ));
        }

        let batch = shape[0];
        let seq_len = shape[1];

        // Project to Q, K, V: [batch, seq_len, hidden] @ [hidden, hidden]
        let q = x.matmul(&self.wq)?;
        let k = x.matmul(&self.wk)?;
        let v = x.matmul(&self.wv)?;

        // Reshape for multi-head: [batch, seq_len, n_heads, head_dim]
        let q = q.reshape(&[batch, seq_len, self.n_heads, self.head_dim])?;
        let k = k.reshape(&[batch, seq_len, self.n_heads, self.head_dim])?;
        let v = v.reshape(&[batch, seq_len, self.n_heads, self.head_dim])?;

        // Transpose to [batch, n_heads, seq_len, head_dim]
        let q = q.transpose_axes(&[0, 2, 1, 3])?;
        let k = k.transpose_axes(&[0, 2, 1, 3])?;
        let v = v.transpose_axes(&[0, 2, 1, 3])?;

        // Attention scores: Q @ K^T / sqrt(d_k)
        // K^T: [batch, n_heads, head_dim, seq_len]
        let k_t = k.transpose_axes(&[0, 1, 3, 2])?;
        let scores = q.matmul(&k_t)?.scale(self.scale)?;

        // Apply mask if provided
        let scores = if let Some(m) = mask {
            // Mask is typically -inf for positions to ignore
            scores.add(m)?
        } else {
            scores
        };

        // Softmax over last axis (seq_len)
        // TODO: When AneAccelerator is implemented, delegate softmax to ANE
        // if batch_size >= ANE_BATCH_THRESHOLD
        let attn_weights = scores.softmax(-1)?;

        // Apply attention to values
        let attn_output = attn_weights.matmul(&v)?;

        // Transpose back: [batch, seq_len, n_heads, head_dim]
        let attn_output = attn_output.transpose_axes(&[0, 2, 1, 3])?;

        // Reshape: [batch, seq_len, hidden_dim]
        let attn_output = attn_output.reshape(&[batch, seq_len, self.hidden_dim])?;

        // Output projection
        attn_output.matmul(&self.wo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attention_creation() {
        let attn = MultiHeadAttention::new(64, 4).unwrap();
        assert_eq!(attn.n_heads, 4);
        assert_eq!(attn.hidden_dim, 64);
        assert_eq!(attn.head_dim, 16);
    }

    #[test]
    fn test_attention_forward() {
        let attn = MultiHeadAttention::new(32, 4).unwrap();

        // Input: [batch=1, seq_len=4, hidden=32]
        let x = Array::ones(&[1, 4, 32]).unwrap();

        let output = attn.forward(&x, None, None).unwrap();
        assert_eq!(output.shape(), vec![1, 4, 32]);
    }

    #[test]
    fn test_attention_invalid_heads() {
        // hidden_dim=32 not divisible by n_heads=5
        let result = MultiHeadAttention::new(32, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_attention_batch() {
        let attn = MultiHeadAttention::new(32, 4).unwrap();

        // Larger batch
        let x = Array::ones(&[2, 8, 32]).unwrap();
        let output = attn.forward(&x, None, None).unwrap();
        assert_eq!(output.shape(), vec![2, 8, 32]);
    }
}
