//! MLX Array - Real Inference Only
//!
//! This module provides the stable Array API wrapping mlx-rs.
//! No stubs, no demo mode - real GPU inference on Apple Silicon.

use crate::{MlxError, Result};
use mlx_rs::Array as MlxArray;

/// Data types for MLX arrays
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dtype {
    #[default]
    Float32,
    Float16,
    BFloat16,
    Int32,
    Int64,
    UInt32,
    UInt64,
    Bool,
}

impl Dtype {
    /// Size in bytes for this dtype
    pub fn size_bytes(&self) -> usize {
        match self {
            Dtype::Float32 | Dtype::Int32 | Dtype::UInt32 => 4,
            Dtype::Float16 | Dtype::BFloat16 => 2,
            Dtype::Int64 | Dtype::UInt64 => 8,
            Dtype::Bool => 1,
        }
    }

    /// Convert to mlx_rs Dtype
    pub fn to_mlx(&self) -> mlx_rs::Dtype {
        match self {
            Dtype::Float32 => mlx_rs::Dtype::Float32,
            Dtype::Float16 => mlx_rs::Dtype::Float16,
            Dtype::BFloat16 => mlx_rs::Dtype::Bfloat16,
            Dtype::Int32 => mlx_rs::Dtype::Int32,
            Dtype::Int64 => mlx_rs::Dtype::Int64,
            Dtype::UInt32 => mlx_rs::Dtype::Uint32,
            Dtype::UInt64 => mlx_rs::Dtype::Uint64,
            Dtype::Bool => mlx_rs::Dtype::Bool,
        }
    }

    /// Convert from mlx_rs Dtype
    pub fn from_mlx(dtype: mlx_rs::Dtype) -> Self {
        match dtype {
            mlx_rs::Dtype::Float32 => Dtype::Float32,
            mlx_rs::Dtype::Float16 => Dtype::Float16,
            mlx_rs::Dtype::Bfloat16 => Dtype::BFloat16,
            mlx_rs::Dtype::Int32 => Dtype::Int32,
            mlx_rs::Dtype::Int64 => Dtype::Int64,
            mlx_rs::Dtype::Uint32 => Dtype::UInt32,
            mlx_rs::Dtype::Uint64 => Dtype::UInt64,
            mlx_rs::Dtype::Bool => Dtype::Bool,
            _ => Dtype::Float32,
        }
    }
}

/// MLX Array - GPU-accelerated tensor on Apple Silicon
#[derive(Debug, Clone)]
pub struct Array {
    inner: MlxArray,
}

impl Array {
    // =========================================================================
    // Construction
    // =========================================================================

    /// Create array from f32 slice
    pub fn from_f32(data: &[f32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            inner: MlxArray::from_slice(data, shape),
        })
    }

    /// Alias for from_f32 (backward compatibility)
    #[inline]
    pub fn from_slice_f32(data: &[f32], shape: &[i32]) -> Result<Self> {
        Self::from_f32(data, shape)
    }

    /// Create array from i32 slice
    pub fn from_i32(data: &[i32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            inner: MlxArray::from_slice(data, shape),
        })
    }

    /// Alias for from_i32 (backward compatibility)
    #[inline]
    pub fn from_slice_i32(data: &[i32], shape: &[i32]) -> Result<Self> {
        Self::from_i32(data, shape)
    }

    /// Create array from u32 slice
    pub fn from_u32(data: &[u32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            inner: MlxArray::from_slice(data, shape),
        })
    }

    /// Alias for from_u32 (backward compatibility)
    #[inline]
    pub fn from_slice_u32(data: &[u32], shape: &[i32]) -> Result<Self> {
        Self::from_u32(data, shape)
    }

    /// Create zeros array
    pub fn zeros(shape: &[i32]) -> Result<Self> {
        let arr = mlx_rs::ops::zeros::<f32>(shape)
            .map_err(|e| MlxError::ArrayOp(format!("zeros: {e}")))?;
        Ok(Self { inner: arr })
    }

    /// Create ones array
    pub fn ones(shape: &[i32]) -> Result<Self> {
        let arr = mlx_rs::ops::ones::<f32>(shape)
            .map_err(|e| MlxError::ArrayOp(format!("ones: {e}")))?;
        Ok(Self { inner: arr })
    }

    /// Create from inner mlx_rs::Array
    pub fn from_inner(inner: MlxArray) -> Self {
        Self { inner }
    }

    // =========================================================================
    // Properties
    // =========================================================================

    /// Get array shape
    pub fn shape(&self) -> Vec<i32> {
        self.inner.shape().to_vec()
    }

    /// Get number of dimensions
    pub fn ndim(&self) -> usize {
        self.inner.ndim()
    }

    /// Get total number of elements
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// Get dtype
    pub fn dtype(&self) -> Dtype {
        Dtype::from_mlx(self.inner.dtype())
    }

    /// Reference to inner mlx_rs::Array
    pub fn inner(&self) -> &MlxArray {
        &self.inner
    }

    /// Consume and return inner mlx_rs::Array
    pub fn into_inner(self) -> MlxArray {
        self.inner
    }

    // =========================================================================
    // Data Access
    // =========================================================================

    /// Get data as f32 vec
    pub fn to_vec_f32(&self) -> Result<Vec<f32>> {
        // Force evaluation before accessing data (MLX uses lazy evaluation)
        self.inner
            .eval()
            .map_err(|e| MlxError::ArrayOp(format!("eval before f32 slice: {e}")))?;
        // Use try_as_slice to avoid panics
        self.inner
            .try_as_slice::<f32>()
            .map(|s| s.to_vec())
            .map_err(|e| MlxError::ArrayOp(format!("Failed to get f32 slice: {:?}", e)))
    }

    /// Get data as i32 vec
    pub fn to_vec_i32(&self) -> Result<Vec<i32>> {
        // Force evaluation before accessing data (MLX uses lazy evaluation)
        self.inner
            .eval()
            .map_err(|e| MlxError::ArrayOp(format!("eval before i32 slice: {e}")))?;
        // Use try_as_slice to avoid panics
        self.inner
            .try_as_slice::<i32>()
            .map(|s| s.to_vec())
            .map_err(|e| MlxError::ArrayOp(format!("Failed to get i32 slice: {:?}", e)))
    }

    /// Force evaluation of lazy computation
    pub fn evaluate(&self) -> Result<()> {
        self.inner
            .eval()
            .map_err(|e| MlxError::ArrayOp(format!("evaluate: {e}")))
    }

    /// Alias for evaluate (backward compatibility)
    #[inline]
    pub fn eval(&self) -> Result<()> {
        self.evaluate()
    }

    // =========================================================================
    // Arithmetic
    // =========================================================================

    /// Element-wise addition
    pub fn add(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            inner: &self.inner + &other.inner,
        })
    }

    /// Element-wise subtraction
    pub fn sub(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            inner: &self.inner - &other.inner,
        })
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            inner: &self.inner * &other.inner,
        })
    }

    /// Element-wise division
    pub fn div(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            inner: &self.inner / &other.inner,
        })
    }

    /// Matrix multiplication
    pub fn matmul(&self, other: &Self) -> Result<Self> {
        let result = self
            .inner
            .matmul(&other.inner)
            .map_err(|e| MlxError::ArrayOp(format!("matmul: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Scale by scalar
    pub fn scale(&self, scalar: f32) -> Result<Self> {
        let scalar_arr = MlxArray::from_slice(&[scalar], &[1]);
        Ok(Self {
            inner: &self.inner * &scalar_arr,
        })
    }

    // =========================================================================
    // Shape Operations
    // =========================================================================

    /// Reshape array
    pub fn reshape(&self, new_shape: &[i32]) -> Result<Self> {
        let result = self
            .inner
            .reshape(new_shape)
            .map_err(|e| MlxError::ArrayOp(format!("reshape: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Transpose
    pub fn transpose(&self) -> Result<Self> {
        Ok(Self {
            inner: self.inner.t(),
        })
    }

    /// Transpose with specific axes
    pub fn transpose_axes(&self, axes: &[i32]) -> Result<Self> {
        let result = self
            .inner
            .transpose_axes(axes)
            .map_err(|e| MlxError::ArrayOp(format!("transpose_axes: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Squeeze (remove size-1 dimensions)
    pub fn squeeze(&self) -> Result<Self> {
        let result = self
            .inner
            .squeeze()
            .map_err(|e| MlxError::ArrayOp(format!("squeeze: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Squeeze specific axis (remove size-1 dimension at given axis)
    pub fn squeeze_axis(&self, axis: i32) -> Result<Self> {
        let result = self
            .inner
            .squeeze_axes(&[axis])
            .map_err(|e| MlxError::ArrayOp(format!("squeeze_axis: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Expand dimensions
    pub fn expand_dims(&self, axis: i32) -> Result<Self> {
        let result = self
            .inner
            .expand_dims(axis)
            .map_err(|e| MlxError::ArrayOp(format!("expand_dims: {e}")))?;
        Ok(Self { inner: result })
    }

    // =========================================================================
    // Reductions
    // =========================================================================

    /// Sum along axis
    pub fn sum(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
        let result = match axis {
            Some(ax) => self
                .inner
                .sum_axis(ax, keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("sum_axis: {e}")))?,
            None => self
                .inner
                .sum(keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("sum: {e}")))?,
        };
        Ok(Self { inner: result })
    }

    /// Mean along axis
    pub fn mean(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
        let result = match axis {
            Some(ax) => self
                .inner
                .mean_axis(ax, keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("mean_axis: {e}")))?,
            None => self
                .inner
                .mean(keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("mean: {e}")))?,
        };
        Ok(Self { inner: result })
    }

    /// Max along axis
    pub fn max(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
        let result = match axis {
            Some(ax) => self
                .inner
                .max_axis(ax, keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("max_axis: {e}")))?,
            None => self
                .inner
                .max(keep_dims)
                .map_err(|e| MlxError::ArrayOp(format!("max: {e}")))?,
        };
        Ok(Self { inner: result })
    }

    /// Argmax along axis
    pub fn argmax(&self, axis: i32, keep_dims: bool) -> Result<Self> {
        let result = mlx_rs::ops::indexing::argmax_axis(&self.inner, axis, keep_dims)
            .map_err(|e| MlxError::ArrayOp(format!("argmax: {e}")))?;
        Ok(Self { inner: result })
    }

    // =========================================================================
    // Activations
    // =========================================================================

    /// Softmax along axis
    pub fn softmax(&self, axis: i32) -> Result<Self> {
        let result = mlx_rs::ops::softmax_axis(&self.inner, axis, false)
            .map_err(|e| MlxError::ArrayOp(format!("softmax: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Square root
    pub fn sqrt(&self) -> Result<Self> {
        let result = self
            .inner
            .sqrt()
            .map_err(|e| MlxError::ArrayOp(format!("sqrt: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Reciprocal square root
    pub fn rsqrt(&self) -> Result<Self> {
        let result = mlx_rs::ops::rsqrt(&self.inner)
            .map_err(|e| MlxError::ArrayOp(format!("rsqrt: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Exponential
    pub fn exp(&self) -> Result<Self> {
        let result = self
            .inner
            .exp()
            .map_err(|e| MlxError::ArrayOp(format!("exp: {e}")))?;
        Ok(Self { inner: result })
    }

    /// SiLU activation (x * sigmoid(x))
    pub fn silu(&self) -> Result<Self> {
        let sigmoid = mlx_rs::ops::sigmoid(&self.inner)
            .map_err(|e| MlxError::ArrayOp(format!("sigmoid: {e}")))?;
        Ok(Self {
            inner: &self.inner * &sigmoid,
        })
    }

    /// ReLU activation (max(0, x))
    pub fn relu(&self) -> Result<Self> {
        let zeros = mlx_rs::ops::zeros_like(&self.inner)
            .map_err(|e| MlxError::ArrayOp(format!("zeros_like: {e}")))?;
        let result = mlx_rs::ops::maximum(&self.inner, &zeros)
            .map_err(|e| MlxError::ArrayOp(format!("maximum: {e}")))?;
        Ok(Self { inner: result })
    }

    /// GELU activation
    pub fn gelu(&self) -> Result<Self> {
        let half = MlxArray::from_slice(&[0.5f32], &[1]);
        let one = MlxArray::from_slice(&[1.0f32], &[1]);
        let coef = MlxArray::from_slice(&[0.044715f32], &[1]);
        let sqrt_2_over_pi = MlxArray::from_slice(&[(2.0f32 / std::f32::consts::PI).sqrt()], &[1]);

        let x_cubed = &self.inner * &(&self.inner * &self.inner);
        let scaled = &coef * &x_cubed;
        let inner_sum = &self.inner + &scaled;
        let scaled_inner = &sqrt_2_over_pi * &inner_sum;
        let tanh_result = mlx_rs::ops::tanh(&scaled_inner)
            .map_err(|e| MlxError::ArrayOp(format!("tanh: {e}")))?;
        let one_plus = &one + &tanh_result;
        let half_result = &half * &one_plus;

        Ok(Self {
            inner: &self.inner * &half_result,
        })
    }

    // =========================================================================
    // Normalization
    // =========================================================================

    /// Layer Normalization
    ///
    /// Normalizes the input along the last axis:
    /// `output = (x - mean) / sqrt(variance + eps) * weight + bias`
    ///
    /// # Arguments
    /// * `weight` - Scale parameter (gamma), shape should match last dimension
    /// * `bias` - Shift parameter (beta), shape should match last dimension
    /// * `eps` - Small constant for numerical stability (typically 1e-5)
    ///
    /// # Example
    /// ```ignore
    /// let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
    /// let weight = Array::ones(&[2])?;
    /// let bias = Array::zeros(&[2])?;
    /// let normalized = x.layernorm(&weight, &bias, 1e-5)?;
    /// ```
    pub fn layernorm(&self, weight: &Array, bias: &Array, eps: f32) -> Result<Self> {
        // Compute mean along last axis
        let last_axis = self.ndim() as i32 - 1;
        let mean = self.mean(Some(last_axis), true)?;

        // Compute x - mean
        let centered = self.sub(&mean)?;

        // Compute variance = mean((x - mean)^2)
        let squared = centered.mul(&centered)?;
        let variance = squared.mean(Some(last_axis), true)?;

        // Compute 1 / sqrt(variance + eps)
        let eps_arr = Array::from_f32(&[eps], &[1])?;
        let var_plus_eps = variance.add(&eps_arr)?;
        let inv_std = var_plus_eps.rsqrt()?;

        // Normalize: (x - mean) / sqrt(var + eps)
        let normalized = centered.mul(&inv_std)?;

        // Scale and shift: normalized * weight + bias
        let scaled = normalized.mul(weight)?;
        scaled.add(bias)
    }

    /// RMS (Root Mean Square) Normalization
    ///
    /// Normalizes the input using RMS:
    /// `output = x * rsqrt(mean(x^2) + eps) * weight`
    ///
    /// Used in LLaMA-style transformer models. Unlike LayerNorm, RMSNorm
    /// does not center the activations (no mean subtraction).
    ///
    /// # Arguments
    /// * `weight` - Scale parameter, shape should match last dimension
    /// * `eps` - Small constant for numerical stability (typically 1e-5)
    ///
    /// # Example
    /// ```ignore
    /// let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
    /// let weight = Array::ones(&[2])?;
    /// let normalized = x.rms_norm(&weight, 1e-5)?;
    /// ```
    pub fn rms_norm(&self, weight: &Array, eps: f32) -> Result<Self> {
        // Compute x^2
        let squared = self.mul(self)?;

        // Compute mean(x^2) along last axis
        let last_axis = self.ndim() as i32 - 1;
        let mean_sq = squared.mean(Some(last_axis), true)?;

        // Compute rsqrt(mean(x^2) + eps)
        let eps_arr = Array::from_f32(&[eps], &[1])?;
        let mean_sq_plus_eps = mean_sq.add(&eps_arr)?;
        let inv_rms = mean_sq_plus_eps.rsqrt()?;

        // Scale: x * inv_rms * weight
        let normalized = self.mul(&inv_rms)?;
        normalized.mul(weight)
    }

    // =========================================================================
    // Indexing
    // =========================================================================

    /// Take elements along axis
    pub fn take(&self, indices: &Self, axis: i32) -> Result<Self> {
        let result = self
            .inner
            .take_axis(&indices.inner, axis)
            .map_err(|e| MlxError::ArrayOp(format!("take: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Slice along axis
    pub fn slice_axis(&self, axis: i32, start: usize, end: usize) -> Result<Self> {
        let len = (end - start) as i32;
        let indices: Vec<i32> = (start as i32..end as i32).collect();
        let idx_array = MlxArray::from_slice(&indices, &[len]);
        let result = self
            .inner
            .take_axis(&idx_array, axis)
            .map_err(|e| MlxError::ArrayOp(format!("slice_axis: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Slice along first axis (shorthand for slice_axis(0, start, end))
    pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
        self.slice_axis(0, start, end)
    }

    /// Take elements along axis (alias for take)
    pub fn take_axis(&self, indices: &Self, axis: i32) -> Result<Self> {
        self.take(indices, axis)
    }

    /// Split array at a given index along an axis
    pub fn split_at_dim(&self, axis: i32, split_idx: usize) -> Result<(Self, Self)> {
        let shape = self.shape();
        let axis_idx = if axis < 0 {
            (shape.len() as i32 + axis) as usize
        } else {
            axis as usize
        };
        let dim_size = shape[axis_idx] as usize;

        let first = self.slice_axis(axis, 0, split_idx)?;
        let second = self.slice_axis(axis, split_idx, dim_size)?;
        Ok((first, second))
    }

    /// Concatenate arrays along first axis
    pub fn concat(arrays: &[&Self]) -> Result<Self> {
        if arrays.is_empty() {
            return Err(MlxError::ArrayOp("Cannot concat empty array list".into()));
        }
        let inner_refs: Vec<&MlxArray> = arrays.iter().map(|a| &a.inner).collect();
        let result = mlx_rs::ops::concatenate(&inner_refs)
            .map_err(|e| MlxError::ArrayOp(format!("concat: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Concatenate arrays along specified axis
    pub fn concat_axis(arrays: &[&Self], axis: i32) -> Result<Self> {
        if arrays.is_empty() {
            return Err(MlxError::ArrayOp("Cannot concat empty array list".into()));
        }
        let inner_refs: Vec<&MlxArray> = arrays.iter().map(|a| &a.inner).collect();
        let result = mlx_rs::ops::concatenate_axis(&inner_refs, axis)
            .map_err(|e| MlxError::ArrayOp(format!("concat_axis: {e}")))?;
        Ok(Self { inner: result })
    }

    /// Tile (repeat) array along each dimension
    pub fn tile(&self, reps: &[i32]) -> Result<Self> {
        let result = mlx_rs::ops::tile(&self.inner, reps)
            .map_err(|e| MlxError::ArrayOp(format!("tile: {e}")))?;
        Ok(Self { inner: result })
    }

    // =========================================================================
    // Type Casting
    // =========================================================================

    /// Cast to different dtype
    pub fn astype(&self, dtype: Dtype) -> Result<Self> {
        let result = self
            .inner
            .as_dtype(dtype.to_mlx())
            .map_err(|e| MlxError::ArrayOp(format!("astype: {e}")))?;
        Ok(Self { inner: result })
    }
}

// =============================================================================
// Tests - run with: cargo test -p adapteros-mlx -- --test-threads=1
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_creation() {
        let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        assert_eq!(arr.shape(), vec![2, 2]);
        assert_eq!(arr.ndim(), 2);
    }

    #[test]
    fn test_array_zeros_ones() {
        let zeros = Array::zeros(&[3, 3]).unwrap();
        assert_eq!(zeros.shape(), vec![3, 3]);

        let ones = Array::ones(&[2, 4]).unwrap();
        assert_eq!(ones.shape(), vec![2, 4]);
    }

    #[test]
    fn test_array_transpose() {
        let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let transposed = arr.transpose().unwrap();
        assert_eq!(transposed.shape(), vec![3, 2]);
    }

    #[test]
    fn test_array_expand_squeeze() {
        let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();

        let expanded = arr.expand_dims(0).unwrap();
        assert_eq!(expanded.shape(), vec![1, 4]);

        let squeezed = expanded.squeeze().unwrap();
        assert_eq!(squeezed.shape(), vec![4]);
    }

    #[test]
    fn test_dtype_conversion() {
        assert_eq!(Dtype::Float32.to_mlx(), mlx_rs::Dtype::Float32);
        assert_eq!(Dtype::from_mlx(mlx_rs::Dtype::Float16), Dtype::Float16);
    }

    #[test]
    fn test_to_vec_f32() {
        let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let data = arr.to_vec_f32().unwrap();
        assert_eq!(data, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_runtime_init() {
        // This is the exact sequence used by the worker
        let result = crate::runtime_init();
        assert!(result.is_ok(), "runtime_init failed: {:?}", result);
        assert!(crate::runtime_is_initialized());
    }

    #[test]
    fn test_layernorm() {
        // Input: 2x4 matrix
        let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
        let weight = Array::ones(&[4]).unwrap();
        let bias = Array::zeros(&[4]).unwrap();

        let result = x.layernorm(&weight, &bias, 1e-5).unwrap();
        assert_eq!(result.shape(), vec![2, 4]);

        let data = result.to_vec_f32().unwrap();
        // Layer norm should produce mean ~0 and std ~1 per row
        // First row: [1,2,3,4] -> normalized
        // Check that values are approximately in [-2, 2] range (normalized)
        for val in &data {
            assert!(val.abs() < 2.5, "Layernorm output out of expected range: {}", val);
        }

        // Check that mean of each row is approximately 0
        let row1_mean = (data[0] + data[1] + data[2] + data[3]) / 4.0;
        let row2_mean = (data[4] + data[5] + data[6] + data[7]) / 4.0;
        assert!(row1_mean.abs() < 1e-5, "Row 1 mean should be ~0: {}", row1_mean);
        assert!(row2_mean.abs() < 1e-5, "Row 2 mean should be ~0: {}", row2_mean);
    }

    #[test]
    fn test_rms_norm() {
        // Input: 2x4 matrix
        let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
        let weight = Array::ones(&[4]).unwrap();

        let result = x.rms_norm(&weight, 1e-5).unwrap();
        assert_eq!(result.shape(), vec![2, 4]);

        let data = result.to_vec_f32().unwrap();
        // RMS norm normalizes by RMS, so values should be scaled
        // Each row should have similar scale factor applied
        for val in &data {
            assert!(val.is_finite(), "RMS norm output should be finite: {}", val);
        }

        // For [1,2,3,4], RMS = sqrt((1+4+9+16)/4) = sqrt(7.5) ≈ 2.739
        // First element should be ~1/2.739 ≈ 0.365
        let expected_first = 1.0 / (7.5_f32).sqrt();
        assert!((data[0] - expected_first).abs() < 0.01,
            "First element should be ~{}: got {}", expected_first, data[0]);
    }

    #[test]
    fn test_layernorm_with_scale_shift() {
        // Test that weight and bias are applied correctly
        let x = Array::from_f32(&[0.0, 1.0, 2.0, 3.0], &[1, 4]).unwrap();
        let weight = Array::from_f32(&[2.0, 2.0, 2.0, 2.0], &[4]).unwrap();
        let bias = Array::from_f32(&[1.0, 1.0, 1.0, 1.0], &[4]).unwrap();

        let result = x.layernorm(&weight, &bias, 1e-5).unwrap();
        let data = result.to_vec_f32().unwrap();

        // With weight=2 and bias=1, output = normalized * 2 + 1
        // So mean of output should be ~1 (bias), not 0
        let mean = (data[0] + data[1] + data[2] + data[3]) / 4.0;
        assert!((mean - 1.0).abs() < 1e-4, "Mean with bias=1 should be ~1: {}", mean);
    }
}
