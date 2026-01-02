//! MLX Array abstraction layer
//!
//! This module provides a unified array interface via the adapteros-mlx SSoT.
//! The mlx-rs backend is deprecated/unsupported; C++ FFI is the primary path.
//!
//! When the `mlx-rs-backend` feature is enabled (deprecated), this re-exports from `adapteros-mlx`.
//! When disabled, a stub implementation is provided for testing on non-MLX builds.

#[cfg(not(feature = "mlx-rs-backend"))]
use adapteros_core::{AosError, Result};

// =========================================================================
// mlx-rs backend implementation via adapteros-mlx SSoT
// =========================================================================

#[cfg(feature = "mlx-rs-backend")]
pub use adapteros_mlx::Dtype;

#[cfg(feature = "mlx-rs-backend")]
pub use adapteros_mlx::Array as MlxArray;

// =========================================================================
// Stub implementation when mlx-rs-backend is not enabled
// =========================================================================

/// Data types for MLX arrays (stub version)
#[cfg(not(feature = "mlx-rs-backend"))]
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

#[cfg(not(feature = "mlx-rs-backend"))]
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

    pub fn from_f32(data: &[f32], shape: &[i32]) -> Result<Self> {
        Self::from_slice_f32(data, shape)
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

    pub fn evaluate(&self) -> Result<()> {
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

// Tests for stub implementation (no GPU required)
#[cfg(all(test, not(feature = "mlx-rs-backend")))]
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

// Tests for mlx-rs backend via adapteros-mlx SSoT
#[cfg(all(test, feature = "mlx-rs-backend"))]
mod mlx_rs_tests {
    use super::*;

    // Shape-only tests work in test harness
    #[test]
    fn test_array_transpose() {
        let arr = MlxArray::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let transposed = arr.transpose().unwrap();
        assert_eq!(transposed.shape(), vec![3, 2]);
    }

    #[test]
    fn test_array_expand_squeeze() {
        let arr = MlxArray::from_f32(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();

        let expanded = arr.expand_dims(0).unwrap();
        assert_eq!(expanded.shape(), vec![1, 4]);

        // Use squeeze_axis for specific axis, or squeeze() for all size-1 dims
        let squeezed = expanded.squeeze_axis(0).unwrap();
        assert_eq!(squeezed.shape(), vec![4]);
    }
}
