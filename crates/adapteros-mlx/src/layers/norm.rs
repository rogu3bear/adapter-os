//! Normalization Layers
//!
//! Provides LayerNorm and RMSNorm layer implementations with optional
//! ANE acceleration support.

use crate::ane::AneAccelerator;
use crate::{Array, Result};

/// Layer Normalization Layer
///
/// Normalizes inputs across the last dimension:
/// `output = (x - mean) / sqrt(var + eps) * weight + bias`
///
/// # ANE Acceleration
///
/// When an `AneAccelerator` is provided and batch size >= threshold,
/// normalization executes on the Neural Engine for better power efficiency.
///
/// # Example
///
/// ```ignore
/// let norm = LayerNorm::new(512, 1e-5)?;
/// let output = norm.forward(&input, None)?;
/// ```
#[derive(Debug, Clone)]
pub struct LayerNorm {
    /// Scale parameter (gamma)
    pub weight: Array,
    /// Shift parameter (beta)
    pub bias: Array,
    /// Epsilon for numerical stability
    pub eps: f32,
    /// Hidden dimension (for validation)
    pub dim: i32,
}

impl LayerNorm {
    /// Create a new LayerNorm layer with ones weight and zeros bias
    pub fn new(dim: i32, eps: f32) -> Result<Self> {
        let weight = Array::ones(&[dim])?;
        let bias = Array::zeros(&[dim])?;
        Ok(Self {
            weight,
            bias,
            eps,
            dim,
        })
    }

    /// Create LayerNorm with custom weights
    pub fn from_weights(weight: Array, bias: Array, eps: f32) -> Result<Self> {
        let shape = weight.shape();
        if shape.len() != 1 {
            return Err(crate::MlxError::ArrayOp(format!(
                "LayerNorm weight must be 1D, got {:?}",
                shape
            )));
        }
        let dim = shape[0];

        let bias_shape = bias.shape();
        if bias_shape != vec![dim] {
            return Err(crate::MlxError::ArrayOp(format!(
                "LayerNorm bias shape {:?} doesn't match weight shape {:?}",
                bias_shape, shape
            )));
        }

        Ok(Self {
            weight,
            bias,
            eps,
            dim,
        })
    }

    /// Forward pass
    ///
    /// # Arguments
    /// * `x` - Input tensor, last dimension must match `dim`
    /// * `ane_accel` - Optional ANE accelerator for LayerNorm delegation
    pub fn forward(&self, x: &Array, ane_accel: Option<&AneAccelerator>) -> Result<Array> {
        // Delegate to ANE when accelerator is provided (handles threshold check internally)
        if let Some(accel) = ane_accel {
            accel.layernorm(x, &self.weight, &self.bias, self.eps)
        } else {
            x.layernorm(&self.weight, &self.bias, self.eps)
        }
    }
}

/// RMS (Root Mean Square) Normalization Layer
///
/// Normalizes inputs using RMS:
/// `output = x * rsqrt(mean(x^2) + eps) * weight`
///
/// Used in LLaMA-style transformer models. Unlike LayerNorm, RMSNorm
/// does not center activations (no mean subtraction), making it slightly
/// more efficient.
///
/// # Example
///
/// ```ignore
/// let norm = RMSNorm::new(512, 1e-5)?;
/// let output = norm.forward(&input, None)?;
/// ```
#[derive(Debug, Clone)]
pub struct RMSNorm {
    /// Scale parameter
    pub weight: Array,
    /// Epsilon for numerical stability
    pub eps: f32,
    /// Hidden dimension
    pub dim: i32,
}

impl RMSNorm {
    /// Create a new RMSNorm layer with ones weight
    pub fn new(dim: i32, eps: f32) -> Result<Self> {
        let weight = Array::ones(&[dim])?;
        Ok(Self { weight, eps, dim })
    }

    /// Create RMSNorm with custom weights
    pub fn from_weights(weight: Array, eps: f32) -> Result<Self> {
        let shape = weight.shape();
        if shape.len() != 1 {
            return Err(crate::MlxError::ArrayOp(format!(
                "RMSNorm weight must be 1D, got {:?}",
                shape
            )));
        }
        let dim = shape[0];
        Ok(Self { weight, eps, dim })
    }

    /// Forward pass
    ///
    /// # Arguments
    /// * `x` - Input tensor, last dimension must match `dim`
    /// * `ane_accel` - Optional ANE accelerator for RMSNorm delegation
    pub fn forward(&self, x: &Array, ane_accel: Option<&AneAccelerator>) -> Result<Array> {
        // Delegate to ANE when accelerator is provided (handles threshold check internally)
        if let Some(accel) = ane_accel {
            accel.rms_norm(x, &self.weight, self.eps)
        } else {
            x.rms_norm(&self.weight, self.eps)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layernorm_layer() {
        let norm = LayerNorm::new(4, 1e-5).unwrap();
        let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
        let result = norm.forward(&x, None).unwrap();
        assert_eq!(result.shape(), vec![1, 4]);
    }

    #[test]
    fn test_rmsnorm_layer() {
        let norm = RMSNorm::new(4, 1e-5).unwrap();
        let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
        let result = norm.forward(&x, None).unwrap();
        assert_eq!(result.shape(), vec![1, 4]);
    }

    #[test]
    fn test_layernorm_from_weights() {
        let weight = Array::from_f32(&[2.0, 2.0, 2.0, 2.0], &[4]).unwrap();
        let bias = Array::from_f32(&[1.0, 1.0, 1.0, 1.0], &[4]).unwrap();
        let norm = LayerNorm::from_weights(weight, bias, 1e-5).unwrap();

        let x = Array::from_f32(&[0.0, 1.0, 2.0, 3.0], &[1, 4]).unwrap();
        let result = norm.forward(&x, None).unwrap();

        let data = result.to_vec_f32().unwrap();
        let mean = (data[0] + data[1] + data[2] + data[3]) / 4.0;
        assert!(
            (mean - 1.0).abs() < 1e-4,
            "Mean should be ~1 with bias=1: {}",
            mean
        );
    }

    #[test]
    fn test_rmsnorm_from_weights() {
        let weight = Array::from_f32(&[0.5, 0.5, 0.5, 0.5], &[4]).unwrap();
        let norm = RMSNorm::from_weights(weight, 1e-5).unwrap();

        let x = Array::from_f32(&[2.0, 2.0, 2.0, 2.0], &[1, 4]).unwrap();
        let result = norm.forward(&x, None).unwrap();

        // x = [2,2,2,2], RMS = 2, so normalized = [1,1,1,1], scaled by 0.5 = [0.5,0.5,0.5,0.5]
        let data = result.to_vec_f32().unwrap();
        for val in &data {
            assert!((val - 0.5).abs() < 0.01, "Expected ~0.5, got {}", val);
        }
    }
}
