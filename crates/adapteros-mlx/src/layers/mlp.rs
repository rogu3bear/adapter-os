//! Feed-Forward / MLP Layer
//!
//! Provides the feed-forward network block used in transformers.

use crate::{Array, MlxError, Result};

/// Activation function type for MLP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activation {
    /// ReLU activation
    ReLU,
    /// GELU activation (GPT-style)
    #[default]
    GELU,
    /// SiLU/Swish activation (LLaMA-style)
    SiLU,
}

/// MLP / Feed-Forward Network Layer
///
/// Standard transformer FFN block with two linear projections and activation:
/// ```text
/// output = down_proj(activation(up_proj(x)))
/// ```
///
/// For gated architectures (LLaMA), uses:
/// ```text
/// output = down_proj(activation(gate_proj(x)) * up_proj(x))
/// ```
///
/// # Example
///
/// ```ignore
/// // Standard FFN (GPT-style)
/// let ffn = MLP::new(512, 2048, Activation::GELU, false)?;
/// let output = ffn.forward(&hidden_states)?;
///
/// // Gated FFN (LLaMA-style)
/// let ffn = MLP::new(512, 2048, Activation::SiLU, true)?;
/// let output = ffn.forward(&hidden_states)?;
/// ```
#[derive(Debug, Clone)]
pub struct MLP {
    /// Up projection / first linear layer [hidden_dim, intermediate_dim]
    pub up_proj: Array,
    /// Down projection / second linear layer [intermediate_dim, hidden_dim]
    pub down_proj: Array,
    /// Gate projection for gated architectures [hidden_dim, intermediate_dim]
    pub gate_proj: Option<Array>,
    /// Activation function
    pub activation: Activation,
    /// Hidden dimension
    pub hidden_dim: i32,
    /// Intermediate (expanded) dimension
    pub intermediate_dim: i32,
}

impl MLP {
    /// Create a new MLP layer
    ///
    /// # Arguments
    /// * `hidden_dim` - Input/output dimension
    /// * `intermediate_dim` - Expanded dimension (typically 4x hidden_dim)
    /// * `activation` - Activation function
    /// * `gated` - Whether to use gated architecture (LLaMA-style)
    pub fn new(
        hidden_dim: i32,
        intermediate_dim: i32,
        activation: Activation,
        gated: bool,
    ) -> Result<Self> {
        // Initialize with small values (real implementations should use proper init)
        let up_proj = Array::ones(&[hidden_dim, intermediate_dim])?.scale(0.01)?;
        let down_proj = Array::ones(&[intermediate_dim, hidden_dim])?.scale(0.01)?;

        let gate_proj = if gated {
            Some(Array::ones(&[hidden_dim, intermediate_dim])?.scale(0.01)?)
        } else {
            None
        };

        Ok(Self {
            up_proj,
            down_proj,
            gate_proj,
            activation,
            hidden_dim,
            intermediate_dim,
        })
    }

    /// Create MLP with custom weights
    pub fn from_weights(
        up_proj: Array,
        down_proj: Array,
        gate_proj: Option<Array>,
        activation: Activation,
    ) -> Result<Self> {
        let up_shape = up_proj.shape();
        if up_shape.len() != 2 {
            return Err(MlxError::ArrayOp(format!(
                "up_proj must be 2D, got {:?}",
                up_shape
            )));
        }

        let hidden_dim = up_shape[0];
        let intermediate_dim = up_shape[1];

        let down_shape = down_proj.shape();
        if down_shape != vec![intermediate_dim, hidden_dim] {
            return Err(MlxError::ArrayOp(format!(
                "down_proj shape {:?} doesn't match expected [{}, {}]",
                down_shape, intermediate_dim, hidden_dim
            )));
        }

        if let Some(ref gate) = gate_proj {
            let gate_shape = gate.shape();
            if gate_shape != vec![hidden_dim, intermediate_dim] {
                return Err(MlxError::ArrayOp(format!(
                    "gate_proj shape {:?} doesn't match up_proj {:?}",
                    gate_shape, up_shape
                )));
            }
        }

        Ok(Self {
            up_proj,
            down_proj,
            gate_proj,
            activation,
            hidden_dim,
            intermediate_dim,
        })
    }

    /// Apply activation function
    fn apply_activation(&self, x: &Array) -> Result<Array> {
        match self.activation {
            Activation::ReLU => x.relu(),
            Activation::GELU => x.gelu(),
            Activation::SiLU => x.silu(),
        }
    }

    /// Forward pass
    ///
    /// # Arguments
    /// * `x` - Input tensor [..., hidden_dim]
    ///
    /// # Returns
    /// Output tensor [..., hidden_dim]
    pub fn forward(&self, x: &Array) -> Result<Array> {
        if let Some(ref gate) = self.gate_proj {
            // Gated architecture: down(act(gate(x)) * up(x))
            let gate_out = x.matmul(gate)?;
            let gate_activated = self.apply_activation(&gate_out)?;
            let up_out = x.matmul(&self.up_proj)?;
            let combined = gate_activated.mul(&up_out)?;
            combined.matmul(&self.down_proj)
        } else {
            // Standard: down(act(up(x)))
            let up_out = x.matmul(&self.up_proj)?;
            let activated = self.apply_activation(&up_out)?;
            activated.matmul(&self.down_proj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlp_creation() {
        let mlp = MLP::new(32, 128, Activation::GELU, false).unwrap();
        assert_eq!(mlp.hidden_dim, 32);
        assert_eq!(mlp.intermediate_dim, 128);
        assert!(mlp.gate_proj.is_none());
    }

    #[test]
    fn test_mlp_gated_creation() {
        let mlp = MLP::new(32, 128, Activation::SiLU, true).unwrap();
        assert!(mlp.gate_proj.is_some());
    }

    #[test]
    fn test_mlp_forward_standard() {
        let mlp = MLP::new(16, 64, Activation::GELU, false).unwrap();

        // Input: [batch=1, seq=4, hidden=16]
        let x = Array::ones(&[1, 4, 16]).unwrap();
        let output = mlp.forward(&x).unwrap();

        assert_eq!(output.shape(), vec![1, 4, 16]);
    }

    #[test]
    fn test_mlp_forward_gated() {
        let mlp = MLP::new(16, 64, Activation::SiLU, true).unwrap();

        // Input: [batch=1, seq=4, hidden=16]
        let x = Array::ones(&[1, 4, 16]).unwrap();
        let output = mlp.forward(&x).unwrap();

        assert_eq!(output.shape(), vec![1, 4, 16]);
    }

    #[test]
    fn test_mlp_activations() {
        // Test each activation type
        for activation in [Activation::ReLU, Activation::GELU, Activation::SiLU] {
            let mlp = MLP::new(8, 32, activation, false).unwrap();
            let x = Array::ones(&[1, 2, 8]).unwrap();
            let output = mlp.forward(&x).unwrap();
            assert_eq!(output.shape(), vec![1, 2, 8]);
        }
    }
}
