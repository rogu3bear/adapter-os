//! MLX Array abstraction layer
//!
//! This module provides a unified array interface that wraps mlx-rs Array types.
//! It serves as the foundation for the mlx-rs backend, replacing the legacy C++ FFI.

use adapteros_core::{AosError, Result};

/// Data types for MLX arrays
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
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
    pub fn size_bytes(&self) -> usize {
        match self {
            Dtype::Float32 | Dtype::Int32 | Dtype::UInt32 => 4,
            Dtype::Float16 | Dtype::BFloat16 => 2,
            Dtype::Int64 | Dtype::UInt64 => 8,
            Dtype::Bool => 1,
        }
    }
}

// =========================================================================
// mlx-rs backend implementation
// =========================================================================

#[cfg(feature = "mlx-rs-backend")]
mod mlx_rs_impl {
    use super::*;
    use mlx_rs::Array;

    /// MLX Array wrapper providing a safe, ergonomic interface to mlx-rs
    #[derive(Debug, Clone)]
    pub struct MlxArray {
        inner: Array,
    }

    impl MlxArray {
        // =====================================================================
        // Construction
        // =====================================================================

        pub fn from_slice_f32(data: &[f32], shape: &[i32]) -> Result<Self> {
            Ok(Self {
                inner: Array::from_slice(data, shape),
            })
        }

        pub fn from_slice_i32(data: &[i32], shape: &[i32]) -> Result<Self> {
            Ok(Self {
                inner: Array::from_slice(data, shape),
            })
        }

        pub fn from_slice_u32(data: &[u32], shape: &[i32]) -> Result<Self> {
            Ok(Self {
                inner: Array::from_slice(data, shape),
            })
        }

        pub fn zeros(shape: &[i32]) -> Result<Self> {
            let arr = mlx_rs::ops::zeros::<f32>(shape)
                .map_err(|e| AosError::Mlx(format!("zeros failed: {}", e)))?;
            Ok(Self { inner: arr })
        }

        pub fn ones(shape: &[i32]) -> Result<Self> {
            let arr = mlx_rs::ops::ones::<f32>(shape)
                .map_err(|e| AosError::Mlx(format!("ones failed: {}", e)))?;
            Ok(Self { inner: arr })
        }

        pub fn from_inner(inner: Array) -> Self {
            Self { inner }
        }

        pub fn into_inner(self) -> Array {
            self.inner
        }

        pub fn inner(&self) -> &Array {
            &self.inner
        }

        // =====================================================================
        // Properties
        // =====================================================================

        pub fn shape(&self) -> Vec<i32> {
            self.inner.shape().to_vec()
        }

        pub fn ndim(&self) -> usize {
            self.inner.ndim()
        }

        pub fn size(&self) -> usize {
            self.inner.size()
        }

        pub fn dtype(&self) -> Dtype {
            match self.inner.dtype() {
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

        // =====================================================================
        // Data Access
        // =====================================================================

        pub fn to_vec_f32(&self) -> Result<Vec<f32>> {
            // as_slice returns &[T] directly in mlx-rs
            Ok(self.inner.as_slice::<f32>().to_vec())
        }

        pub fn to_vec_i32(&self) -> Result<Vec<i32>> {
            Ok(self.inner.as_slice::<i32>().to_vec())
        }

        pub fn to_vec_u32(&self) -> Result<Vec<u32>> {
            Ok(self.inner.as_slice::<u32>().to_vec())
        }

        // =====================================================================
        // Evaluation
        // =====================================================================

        pub fn eval(&self) -> Result<()> {
            self.inner
                .eval()
                .map_err(|e| AosError::Mlx(format!("eval failed: {}", e)))
        }

        // =====================================================================
        // Arithmetic Operations
        // =====================================================================

        pub fn add(&self, other: &Self) -> Result<Self> {
            Ok(Self {
                inner: &self.inner + &other.inner,
            })
        }

        pub fn sub(&self, other: &Self) -> Result<Self> {
            Ok(Self {
                inner: &self.inner - &other.inner,
            })
        }

        pub fn mul(&self, other: &Self) -> Result<Self> {
            Ok(Self {
                inner: &self.inner * &other.inner,
            })
        }

        pub fn div(&self, other: &Self) -> Result<Self> {
            Ok(Self {
                inner: &self.inner / &other.inner,
            })
        }

        pub fn matmul(&self, other: &Self) -> Result<Self> {
            let result = self
                .inner
                .matmul(&other.inner)
                .map_err(|e| AosError::Mlx(format!("matmul failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn scale(&self, scalar: f32) -> Result<Self> {
            let scalar_arr = Array::from_slice(&[scalar], &[1]);
            Ok(Self {
                inner: &self.inner * &scalar_arr,
            })
        }

        // =====================================================================
        // Shape Operations
        // =====================================================================

        pub fn reshape(&self, new_shape: &[i32]) -> Result<Self> {
            let result = self
                .inner
                .reshape(new_shape)
                .map_err(|e| AosError::Mlx(format!("reshape failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn transpose(&self) -> Result<Self> {
            // t() returns Array directly in mlx-rs
            Ok(Self {
                inner: self.inner.t(),
            })
        }

        pub fn squeeze(&self, _axis: Option<i32>) -> Result<Self> {
            // mlx-rs squeeze() takes no arguments - squeezes all size-1 dims
            let result = self
                .inner
                .squeeze()
                .map_err(|e| AosError::Mlx(format!("squeeze failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn expand_dims(&self, axis: i32) -> Result<Self> {
            // mlx-rs expand_dims takes a single i32, not a slice
            let result = self
                .inner
                .expand_dims(axis)
                .map_err(|e| AosError::Mlx(format!("expand_dims failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        // =====================================================================
        // Reduction Operations
        // =====================================================================

        pub fn sum(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
            let result = match axis {
                Some(ax) => self
                    .inner
                    .sum_axis(ax, keep_dims)
                    .map_err(|e| AosError::Mlx(format!("sum_axis failed: {}", e)))?,
                None => self
                    .inner
                    .sum(keep_dims)
                    .map_err(|e| AosError::Mlx(format!("sum failed: {}", e)))?,
            };
            Ok(Self { inner: result })
        }

        pub fn mean(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
            let result = match axis {
                Some(ax) => self
                    .inner
                    .mean_axis(ax, keep_dims)
                    .map_err(|e| AosError::Mlx(format!("mean_axis failed: {}", e)))?,
                None => self
                    .inner
                    .mean(keep_dims)
                    .map_err(|e| AosError::Mlx(format!("mean failed: {}", e)))?,
            };
            Ok(Self { inner: result })
        }

        pub fn max(&self, axis: Option<i32>, keep_dims: bool) -> Result<Self> {
            let result = match axis {
                Some(ax) => self
                    .inner
                    .max_axis(ax, keep_dims)
                    .map_err(|e| AosError::Mlx(format!("max_axis failed: {}", e)))?,
                None => self
                    .inner
                    .max(keep_dims)
                    .map_err(|e| AosError::Mlx(format!("max failed: {}", e)))?,
            };
            Ok(Self { inner: result })
        }

        pub fn argmax(&self, axis: i32, keep_dims: bool) -> Result<Self> {
            // Use argmax_axis for axis-specific argmax
            let result = mlx_rs::ops::indexing::argmax_axis(&self.inner, axis, keep_dims)
                .map_err(|e| AosError::Mlx(format!("argmax failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        // =====================================================================
        // Activation Functions
        // =====================================================================

        pub fn softmax(&self, axis: i32) -> Result<Self> {
            // Use softmax_axis for axis-specific softmax (precise=false)
            let result = mlx_rs::ops::softmax_axis(&self.inner, axis, false)
                .map_err(|e| AosError::Mlx(format!("softmax failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn sqrt(&self) -> Result<Self> {
            let result = self
                .inner
                .sqrt()
                .map_err(|e| AosError::Mlx(format!("sqrt failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn exp(&self) -> Result<Self> {
            let result = self
                .inner
                .exp()
                .map_err(|e| AosError::Mlx(format!("exp failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn silu(&self) -> Result<Self> {
            let sigmoid = mlx_rs::ops::sigmoid(&self.inner)
                .map_err(|e| AosError::Mlx(format!("sigmoid failed: {}", e)))?;
            Ok(Self {
                inner: &self.inner * &sigmoid,
            })
        }

        pub fn rsqrt(&self) -> Result<Self> {
            let result = mlx_rs::ops::rsqrt(&self.inner)
                .map_err(|e| AosError::Mlx(format!("rsqrt failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn relu(&self) -> Result<Self> {
            // ReLU: max(0, x) - implement using mlx-rs ops
            let zeros = mlx_rs::ops::zeros_like(&self.inner)
                .map_err(|e| AosError::Mlx(format!("zeros_like failed: {}", e)))?;
            let result = mlx_rs::ops::maximum(&self.inner, &zeros)
                .map_err(|e| AosError::Mlx(format!("maximum failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn gelu(&self) -> Result<Self> {
            // GELU approximation: x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
            // We use the fast tanh approximation form
            let half = Array::from_slice(&[0.5f32], &[1]);
            let one = Array::from_slice(&[1.0f32], &[1]);
            let coef = Array::from_slice(&[0.044715f32], &[1]);
            let sqrt_2_over_pi = Array::from_slice(&[(2.0f32 / std::f32::consts::PI).sqrt()], &[1]);

            // x^3
            let x_cubed = &self.inner * &(&self.inner * &self.inner);
            // 0.044715 * x^3
            let scaled = &coef * &x_cubed;
            // x + 0.044715 * x^3
            let inner_sum = &self.inner + &scaled;
            // sqrt(2/pi) * (x + 0.044715 * x^3)
            let scaled_inner = &sqrt_2_over_pi * &inner_sum;
            // tanh(...)
            let tanh_result = mlx_rs::ops::tanh(&scaled_inner)
                .map_err(|e| AosError::Mlx(format!("tanh failed: {}", e)))?;
            // 1 + tanh(...)
            let one_plus = &one + &tanh_result;
            // 0.5 * (1 + tanh(...))
            let half_result = &half * &one_plus;
            // x * 0.5 * (1 + tanh(...))
            Ok(Self {
                inner: &self.inner * &half_result,
            })
        }

        /// Transpose with specific axis permutation
        pub fn transpose_axes(&self, axes: &[i32]) -> Result<Self> {
            let result = self
                .inner
                .transpose_axes(axes)
                .map_err(|e| AosError::Mlx(format!("transpose_axes failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        // =====================================================================
        // Indexing Operations
        // =====================================================================

        pub fn take(&self, indices: &Self) -> Result<Self> {
            // take() flattens to 1D and takes indices
            let result = self
                .inner
                .take(&indices.inner)
                .map_err(|e| AosError::Mlx(format!("take failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn take_axis(&self, indices: &Self, axis: i32) -> Result<Self> {
            // take_axis() preserves shape and takes along specific axis
            let result = self
                .inner
                .take_axis(&indices.inner, axis)
                .map_err(|e| AosError::Mlx(format!("take_axis failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn concat(arrays: &[&Self]) -> Result<Self> {
            if arrays.is_empty() {
                return Err(AosError::Mlx("Cannot concat empty array list".to_string()));
            }
            let inner_refs: Vec<&Array> = arrays.iter().map(|a| &a.inner).collect();
            let result = mlx_rs::ops::concatenate(&inner_refs)
                .map_err(|e| AosError::Mlx(format!("concat failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        pub fn concat_axis(arrays: &[&Self], _axis: i32) -> Result<Self> {
            Self::concat(arrays)
        }

        // =====================================================================
        // Additional Operations for Transformer Support
        // =====================================================================

        /// Create a zeros array with the same shape as self
        pub fn zeros_like(&self) -> Result<Self> {
            let result = mlx_rs::ops::zeros_like(&self.inner)
                .map_err(|e| AosError::Mlx(format!("zeros_like failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        /// Top-k selection along an axis
        /// Returns (values, indices) where both have shape [..., k] along the specified axis
        pub fn topk(&self, k: i32, axis: i32) -> Result<(Self, Self)> {
            // Use mlx-rs topk_axis which returns only values
            // Then use argpartition to get the indices
            use mlx_rs::ops::indexing::topk_axis;
            use mlx_rs::ops::argpartition_axis;

            // Get top-k values
            let k_values = topk_axis(&self.inner, k, axis)
                .map_err(|e| AosError::Mlx(format!("topk_axis failed: {}", e)))?;

            // Get indices of top-k elements using argpartition
            // argpartition puts k largest at the end, we need to take last k
            let shape = self.inner.shape();
            let ndim = shape.len();
            let axis_idx = if axis < 0 {
                (ndim as i32 + axis) as usize
            } else {
                axis as usize
            };
            let axis_len = shape[axis_idx];

            // Negate to get indices of largest elements first
            let neg_arr = mlx_rs::ops::negative(&self.inner)
                .map_err(|e| AosError::Mlx(format!("negative failed: {}", e)))?;

            // argpartition with kth=k-1 puts k smallest of negated (= k largest of original) first
            let partitioned_indices = argpartition_axis(&neg_arr, k - 1, axis)
                .map_err(|e| AosError::Mlx(format!("argpartition_axis failed: {}", e)))?;

            // Take first k indices
            let indices: Vec<i32> = (0..k).collect();
            let idx_arr = Array::from_slice(&indices, &[k]);
            let k_indices = partitioned_indices
                .take_axis(&idx_arr, axis)
                .map_err(|e| AosError::Mlx(format!("take_axis failed: {}", e)))?;

            Ok((Self { inner: k_values }, Self { inner: k_indices }))
        }

        /// Tile (repeat) array along each dimension
        pub fn tile(&self, reps: &[i32]) -> Result<Self> {
            let result = mlx_rs::ops::tile(&self.inner, reps)
                .map_err(|e| AosError::Mlx(format!("tile failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        /// Slice along a specific axis from start to end (exclusive)
        pub fn slice_axis(&self, axis: i32, start: usize, end: usize) -> Result<Self> {
            // Build slice indices for all dimensions
            let ndim = self.inner.ndim();
            let axis_idx = if axis < 0 {
                (ndim as i32 + axis) as usize
            } else {
                axis as usize
            };

            // Use array indexing via take_axis with range indices
            let len = end - start;
            let indices: Vec<i32> = (start as i32..(end as i32)).collect();
            let idx_array = Array::from_slice(&indices, &[len as i32]);
            let result = self
                .inner
                .take_axis(&idx_array, axis_idx as i32)
                .map_err(|e| AosError::Mlx(format!("slice_axis failed: {}", e)))?;
            Ok(Self { inner: result })
        }

        /// Split array into two parts at a given index along an axis
        pub fn split_at_dim(&self, axis: i32, split_idx: usize) -> Result<(Self, Self)> {
            let shape = self.inner.shape();
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

        /// Slice along first axis (row slicing) - for RoPE cache access
        pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
            self.slice_axis(0, start, end)
        }

        // =====================================================================
        // Type Casting
        // =====================================================================

        pub fn astype(&self, dtype: Dtype) -> Result<Self> {
            let mlx_dtype = match dtype {
                Dtype::Float32 => mlx_rs::Dtype::Float32,
                Dtype::Float16 => mlx_rs::Dtype::Float16,
                Dtype::BFloat16 => mlx_rs::Dtype::Bfloat16,
                Dtype::Int32 => mlx_rs::Dtype::Int32,
                Dtype::Int64 => mlx_rs::Dtype::Int64,
                Dtype::UInt32 => mlx_rs::Dtype::Uint32,
                Dtype::UInt64 => mlx_rs::Dtype::Uint64,
                Dtype::Bool => mlx_rs::Dtype::Bool,
            };
            let result = self
                .inner
                .as_dtype(mlx_dtype)
                .map_err(|e| AosError::Mlx(format!("astype failed: {}", e)))?;
            Ok(Self { inner: result })
        }
    }
}

#[cfg(feature = "mlx-rs-backend")]
pub use mlx_rs_impl::MlxArray;

// =========================================================================
// Stub implementation when mlx-rs-backend is not enabled
// =========================================================================

#[cfg(not(feature = "mlx-rs-backend"))]
#[derive(Debug, Clone)]
pub struct MlxArray {
    data: Vec<f32>,
    shape: Vec<i32>,
    dtype: Dtype,
}

#[cfg(not(feature = "mlx-rs-backend"))]
impl MlxArray {
    pub fn from_slice_f32(data: &[f32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            data: data.to_vec(),
            shape: shape.to_vec(),
            dtype: Dtype::Float32,
        })
    }

    pub fn from_slice_i32(_data: &[i32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            data: vec![0.0; shape.iter().map(|&x| x as usize).product()],
            shape: shape.to_vec(),
            dtype: Dtype::Int32,
        })
    }

    pub fn from_slice_u32(_data: &[u32], shape: &[i32]) -> Result<Self> {
        Ok(Self {
            data: vec![0.0; shape.iter().map(|&x| x as usize).product()],
            shape: shape.to_vec(),
            dtype: Dtype::UInt32,
        })
    }

    pub fn zeros(shape: &[i32]) -> Result<Self> {
        let size: usize = shape.iter().map(|&x| x as usize).product();
        Ok(Self {
            data: vec![0.0; size],
            shape: shape.to_vec(),
            dtype: Dtype::Float32,
        })
    }

    pub fn ones(shape: &[i32]) -> Result<Self> {
        let size: usize = shape.iter().map(|&x| x as usize).product();
        Ok(Self {
            data: vec![1.0; size],
            shape: shape.to_vec(),
            dtype: Dtype::Float32,
        })
    }

    pub fn shape(&self) -> Vec<i32> {
        self.shape.clone()
    }

    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    pub fn size(&self) -> usize {
        self.shape.iter().map(|&x| x as usize).product()
    }

    pub fn dtype(&self) -> Dtype {
        self.dtype
    }

    pub fn to_vec_f32(&self) -> Result<Vec<f32>> {
        Ok(self.data.clone())
    }

    pub fn to_vec_i32(&self) -> Result<Vec<i32>> {
        Ok(self.data.iter().map(|&x| x as i32).collect())
    }

    pub fn to_vec_u32(&self) -> Result<Vec<u32>> {
        Ok(self.data.iter().map(|&x| x as u32).collect())
    }

    pub fn eval(&self) -> Result<()> {
        Ok(())
    }

    pub fn add(&self, other: &Self) -> Result<Self> {
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a + b)
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn sub(&self, other: &Self) -> Result<Self> {
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a - b)
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn mul(&self, other: &Self) -> Result<Self> {
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn div(&self, other: &Self) -> Result<Self> {
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a / b)
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn matmul(&self, _other: &Self) -> Result<Self> {
        Ok(self.clone())
    }

    pub fn scale(&self, scalar: f32) -> Result<Self> {
        let data: Vec<f32> = self.data.iter().map(|x| x * scalar).collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn reshape(&self, new_shape: &[i32]) -> Result<Self> {
        Ok(Self {
            data: self.data.clone(),
            shape: new_shape.to_vec(),
            dtype: self.dtype,
        })
    }

    pub fn transpose(&self) -> Result<Self> {
        let new_shape: Vec<i32> = self.shape.iter().rev().cloned().collect();
        Ok(Self {
            data: self.data.clone(),
            shape: new_shape,
            dtype: self.dtype,
        })
    }

    pub fn squeeze(&self, _axis: Option<i32>) -> Result<Self> {
        let new_shape: Vec<i32> = self.shape.iter().filter(|&&x| x != 1).cloned().collect();
        Ok(Self {
            data: self.data.clone(),
            shape: if new_shape.is_empty() {
                vec![1]
            } else {
                new_shape
            },
            dtype: self.dtype,
        })
    }

    pub fn expand_dims(&self, axis: i32) -> Result<Self> {
        let mut new_shape = self.shape.clone();
        let idx = if axis < 0 {
            (new_shape.len() as i32 + axis + 1) as usize
        } else {
            axis as usize
        };
        new_shape.insert(idx.min(new_shape.len()), 1);
        Ok(Self {
            data: self.data.clone(),
            shape: new_shape,
            dtype: self.dtype,
        })
    }

    pub fn sum(&self, _axis: Option<i32>, _keep_dims: bool) -> Result<Self> {
        let total: f32 = self.data.iter().sum();
        Ok(Self {
            data: vec![total],
            shape: vec![1],
            dtype: self.dtype,
        })
    }

    pub fn mean(&self, _axis: Option<i32>, _keep_dims: bool) -> Result<Self> {
        let total: f32 = self.data.iter().sum();
        let mean = total / self.data.len() as f32;
        Ok(Self {
            data: vec![mean],
            shape: vec![1],
            dtype: self.dtype,
        })
    }

    pub fn max(&self, _axis: Option<i32>, _keep_dims: bool) -> Result<Self> {
        let max = self.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        Ok(Self {
            data: vec![max],
            shape: vec![1],
            dtype: self.dtype,
        })
    }

    pub fn argmax(&self, _axis: i32, _keep_dims: bool) -> Result<Self> {
        let (idx, _) = self
            .data
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap_or((0, &0.0));
        Ok(Self {
            data: vec![idx as f32],
            shape: vec![1],
            dtype: Dtype::Int32,
        })
    }

    pub fn softmax(&self, _axis: i32) -> Result<Self> {
        let max = self.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = self.data.iter().map(|x| (x - max).exp()).sum();
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| (x - max).exp() / exp_sum)
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn sqrt(&self) -> Result<Self> {
        let data: Vec<f32> = self.data.iter().map(|x| x.sqrt()).collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn exp(&self) -> Result<Self> {
        let data: Vec<f32> = self.data.iter().map(|x| x.exp()).collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn silu(&self) -> Result<Self> {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| x * (1.0 / (1.0 + (-x).exp())))
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn rsqrt(&self) -> Result<Self> {
        let data: Vec<f32> = self.data.iter().map(|x| 1.0 / x.sqrt()).collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn relu(&self) -> Result<Self> {
        let data: Vec<f32> = self.data.iter().map(|x| x.max(0.0)).collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn gelu(&self) -> Result<Self> {
        // Approximate GELU: x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| {
                let c = (2.0_f32 / std::f32::consts::PI).sqrt();
                x * 0.5 * (1.0 + (c * (x + 0.044715 * x.powi(3))).tanh())
            })
            .collect();
        Ok(Self {
            data,
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn transpose_axes(&self, _axes: &[i32]) -> Result<Self> {
        // Simplified: just reverse shape for stub
        let new_shape: Vec<i32> = self.shape.iter().rev().cloned().collect();
        Ok(Self {
            data: self.data.clone(),
            shape: new_shape,
            dtype: self.dtype,
        })
    }

    pub fn take(&self, _indices: &Self) -> Result<Self> {
        Ok(self.clone())
    }

    pub fn take_axis(&self, _indices: &Self, _axis: i32) -> Result<Self> {
        Ok(self.clone())
    }

    pub fn concat(arrays: &[&Self]) -> Result<Self> {
        if arrays.is_empty() {
            return Err(AosError::Mlx("Cannot concat empty array list".to_string()));
        }
        let mut data = Vec::new();
        for arr in arrays {
            data.extend_from_slice(&arr.data);
        }
        let mut shape = arrays[0].shape.clone();
        shape[0] = arrays.iter().map(|a| a.shape[0]).sum();
        Ok(Self {
            data,
            shape,
            dtype: arrays[0].dtype,
        })
    }

    pub fn concat_axis(arrays: &[&Self], _axis: i32) -> Result<Self> {
        Self::concat(arrays)
    }

    pub fn zeros_like(&self) -> Result<Self> {
        Ok(Self {
            data: vec![0.0; self.data.len()],
            shape: self.shape.clone(),
            dtype: self.dtype,
        })
    }

    pub fn topk(&self, k: i32, _axis: i32) -> Result<(Self, Self)> {
        // Simplified: return first k elements
        let k = (k as usize).min(self.data.len());
        let mut indexed: Vec<(usize, f32)> = self.data.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let values: Vec<f32> = indexed.iter().take(k).map(|(_, v)| *v).collect();
        let indices: Vec<f32> = indexed.iter().take(k).map(|(i, _)| *i as f32).collect();

        Ok((
            Self {
                data: values,
                shape: vec![k as i32],
                dtype: self.dtype,
            },
            Self {
                data: indices,
                shape: vec![k as i32],
                dtype: Dtype::Int32,
            },
        ))
    }

    pub fn tile(&self, reps: &[i32]) -> Result<Self> {
        // Simplified: just repeat data
        let total_reps: usize = reps.iter().map(|&x| x as usize).product();
        let mut data = Vec::with_capacity(self.data.len() * total_reps);
        for _ in 0..total_reps {
            data.extend_from_slice(&self.data);
        }
        let new_shape: Vec<i32> = self
            .shape
            .iter()
            .zip(reps.iter())
            .map(|(&s, &r)| s * r)
            .collect();
        Ok(Self {
            data,
            shape: new_shape,
            dtype: self.dtype,
        })
    }

    pub fn slice_axis(&self, _axis: i32, start: usize, end: usize) -> Result<Self> {
        let len = end - start;
        let data = self.data[start..end.min(self.data.len())].to_vec();
        Ok(Self {
            data,
            shape: vec![len as i32],
            dtype: self.dtype,
        })
    }

    pub fn split_at_dim(&self, _axis: i32, split_idx: usize) -> Result<(Self, Self)> {
        let first_data = self.data[..split_idx.min(self.data.len())].to_vec();
        let second_data = self.data[split_idx.min(self.data.len())..].to_vec();
        Ok((
            Self {
                data: first_data.clone(),
                shape: vec![first_data.len() as i32],
                dtype: self.dtype,
            },
            Self {
                data: second_data.clone(),
                shape: vec![second_data.len() as i32],
                dtype: self.dtype,
            },
        ))
    }

    pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
        self.slice_axis(0, start, end)
    }

    pub fn astype(&self, dtype: Dtype) -> Result<Self> {
        Ok(Self {
            data: self.data.clone(),
            shape: self.shape.clone(),
            dtype,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_creation() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        assert_eq!(arr.shape(), vec![2, 2]);
        assert_eq!(arr.size(), 4);
        assert_eq!(arr.ndim(), 2);
    }

    #[test]
    fn test_array_zeros_ones() {
        let zeros = MlxArray::zeros(&[3, 3]).unwrap();
        assert_eq!(zeros.shape(), vec![3, 3]);
        assert_eq!(zeros.size(), 9);

        let ones = MlxArray::ones(&[2, 4]).unwrap();
        assert_eq!(ones.shape(), vec![2, 4]);
        assert_eq!(ones.size(), 8);
    }

    #[test]
    fn test_array_arithmetic() {
        let a = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let b = MlxArray::from_slice_f32(&[2.0, 3.0, 4.0, 5.0], &[4]).unwrap();

        let sum = a.add(&b).unwrap();
        let data = sum.to_vec_f32().unwrap();
        assert_eq!(data, vec![3.0, 5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_array_reshape() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[6]).unwrap();
        let reshaped = arr.reshape(&[2, 3]).unwrap();
        assert_eq!(reshaped.shape(), vec![2, 3]);
        assert_eq!(reshaped.size(), 6);
    }

    #[test]
    fn test_array_transpose() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let transposed = arr.transpose().unwrap();
        assert_eq!(transposed.shape(), vec![3, 2]);
    }

    #[test]
    fn test_array_scale() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
        let scaled = arr.scale(2.0).unwrap();
        let data = scaled.to_vec_f32().unwrap();
        assert_eq!(data, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_array_softmax() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0], &[3]).unwrap();
        let softmaxed = arr.softmax(-1).unwrap();
        let data = softmaxed.to_vec_f32().unwrap();

        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_array_expand_squeeze() {
        let arr = MlxArray::from_slice_f32(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();

        let expanded = arr.expand_dims(0).unwrap();
        assert_eq!(expanded.shape(), vec![1, 4]);

        let squeezed = expanded.squeeze(Some(0)).unwrap();
        assert_eq!(squeezed.shape(), vec![4]);
    }
}
