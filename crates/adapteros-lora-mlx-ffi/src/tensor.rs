//! MLX FFI tensor operations

use adapteros_core::{AosError, Result};

use crate::{
    mlx_add, mlx_array_data, mlx_array_free, mlx_array_from_data, mlx_array_from_ints,
    mlx_array_size, mlx_array_t, mlx_clear_error, mlx_get_last_error, mlx_matmul, mlx_multiply,
};

/// MLX FFI tensor wrapper
pub struct MLXFFITensor {
    /// Raw MLX array pointer
    pub inner: *mut mlx_array_t,
    /// Tensor shape
    shape: Vec<usize>,
    /// Data type
    dtype: TensorDtype,
}

/// Tensor data types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TensorDtype {
    Float32,
    Int32,
    UInt32,
}

impl MLXFFITensor {
    /// Create a new tensor from data
    pub fn from_data(data: &[f32], shape: Vec<usize>) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let array = mlx_array_from_data(data.as_ptr(), data.len() as i32);
            if array.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to create MLX array: {}",
                    error_str
                )));
            }

            Ok(Self {
                inner: array,
                shape,
                dtype: TensorDtype::Float32,
            })
        }
    }

    /// Create a new tensor from integer data
    pub fn from_ints(data: &[i32], shape: Vec<usize>) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let array = mlx_array_from_ints(data.as_ptr(), data.len() as i32);
            if array.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to create MLX array: {}",
                    error_str
                )));
            }

            Ok(Self {
                inner: array,
                shape,
                dtype: TensorDtype::Int32,
            })
        }
    }

    /// Get tensor data as slice
    pub fn data(&self) -> Result<&[f32]> {
        if self.dtype != TensorDtype::Float32 {
            return Err(AosError::Other("Tensor is not Float32 type".to_string()));
        }

        let data_ptr = unsafe { mlx_array_data(self.inner) };
        if data_ptr.is_null() {
            return Err(AosError::Other("Failed to get tensor data".to_string()));
        }

        let size = unsafe { mlx_array_size(self.inner) };
        Ok(unsafe { std::slice::from_raw_parts(data_ptr, size as usize) })
    }

    /// Get tensor shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get tensor size (total elements)
    pub fn size(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get data type
    pub fn dtype(&self) -> TensorDtype {
        self.dtype
    }

    /// Add two tensors
    pub fn add(&self, other: &Self) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let result_array = mlx_add(self.inner, other.inner);
            if result_array.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to add tensors: {}",
                    error_str
                )));
            }

            // For now, assume same shape (simplified)
            Ok(Self {
                inner: result_array,
                shape: self.shape.clone(),
                dtype: self.dtype,
            })
        }
    }

    /// Multiply two tensors
    pub fn multiply(&self, other: &Self) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let result_array = mlx_multiply(self.inner, other.inner);
            if result_array.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to multiply tensors: {}",
                    error_str
                )));
            }

            // For now, assume same shape (simplified)
            Ok(Self {
                inner: result_array,
                shape: self.shape.clone(),
                dtype: self.dtype,
            })
        }
    }

    /// Matrix multiplication
    pub fn matmul(&self, other: &Self) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let result_array = mlx_matmul(self.inner, other.inner);
            if result_array.is_null() {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to perform matrix multiplication: {}",
                    error_str
                )));
            }

            // For now, assume compatible shapes (simplified)
            let result_shape = if self.shape.len() >= 2 && other.shape.len() >= 2 {
                let mut new_shape = self.shape.clone();
                let last_dim = other.shape[other.shape.len() - 1];
                let last_idx = new_shape.len() - 1;
                new_shape[last_idx] = last_dim;
                new_shape
            } else {
                self.shape.clone()
            };

            Ok(Self {
                inner: result_array,
                shape: result_shape,
                dtype: self.dtype,
            })
        }
    }
}

impl Drop for MLXFFITensor {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe {
                mlx_array_free(self.inner);
            }
        }
    }
}

// Safety: MLX FFI tensor is thread-safe
unsafe impl Send for MLXFFITensor {}
unsafe impl Sync for MLXFFITensor {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_creation() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];
        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        assert_eq!(tensor.shape(), &[2, 2]);
        assert_eq!(tensor.size(), 4);
        assert_eq!(tensor.dtype(), TensorDtype::Float32);
    }

    #[test]
    fn test_tensor_operations() {
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![2.0, 3.0, 4.0, 5.0];
        let shape = vec![2, 2];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        // Test addition
        let result = tensor1.add(&tensor2).unwrap();
        assert_eq!(result.shape(), &[2, 2]);
    }
}
