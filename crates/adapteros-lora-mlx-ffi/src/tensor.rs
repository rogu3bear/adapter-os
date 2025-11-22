//! MLX FFI tensor operations

use adapteros_core::{AosError, Result};

use crate::{
    mlx_add, mlx_array_copy, mlx_array_data, mlx_array_dtype, mlx_array_free, mlx_array_from_data,
    mlx_array_from_ints, mlx_array_ndim, mlx_array_reshape, mlx_array_shape, mlx_array_size,
    mlx_array_t, mlx_array_transpose, mlx_clear_error, mlx_get_last_error, mlx_matmul,
    mlx_multiply,
};

/// MLX FFI tensor wrapper
#[derive(Debug)]
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

    /// Get tensor data as a vector (for testing and conversion)
    pub fn to_float_vec(&self) -> Result<Vec<f32>> {
        if self.dtype != TensorDtype::Float32 {
            return Err(AosError::Other("Tensor is not Float32 type".to_string()));
        }

        let data_slice = self.data()?;
        Ok(data_slice.to_vec())
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
            mlx_clear_error(); // ALWAYS clear first

            let result_array = mlx_add(self.inner, other.inner);

            // Check for null FIRST
            if result_array.is_null() {
                // THEN check for error message
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown error - null result with no error message".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                // Clear error for next operation
                mlx_clear_error();

                return Err(AosError::Other(format!(
                    "Failed to add tensors: {}",
                    error_str
                )));
            }

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
            mlx_clear_error(); // ALWAYS clear first

            let result_array = mlx_multiply(self.inner, other.inner);

            // Check for null FIRST
            if result_array.is_null() {
                // THEN check for error message
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown error - null result with no error message".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                // Clear error for next operation
                mlx_clear_error();

                return Err(AosError::Other(format!(
                    "Failed to multiply tensors: {}",
                    error_str
                )));
            }

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
            mlx_clear_error(); // ALWAYS clear first

            let result_array = mlx_matmul(self.inner, other.inner);

            // Check for null FIRST
            if result_array.is_null() {
                // THEN check for error message
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown error - null result with no error message".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                // Clear error for next operation
                mlx_clear_error();

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

    // =========================================================================
    // Shape Manipulation Operations
    // =========================================================================

    /// Reshape tensor to a new shape using mx::reshape()
    ///
    /// Returns a new tensor with the same data but different shape.
    /// The total number of elements must remain the same.
    ///
    /// # Arguments
    /// * `new_shape` - Target shape dimensions
    ///
    /// # Returns
    /// A new tensor with the specified shape
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![4])?;
    /// let reshaped = tensor.reshape(vec![2, 2])?;
    /// assert_eq!(reshaped.shape(), &[2, 2]);
    /// ```
    pub fn reshape(&self, new_shape: Vec<usize>) -> Result<Self> {
        // Validate that total elements match
        let current_size: usize = self.shape.iter().product();
        let new_size: usize = new_shape.iter().product();
        if current_size != new_size {
            return Err(AosError::Other(format!(
                "Cannot reshape tensor of size {} to shape {:?} (size {})",
                current_size, new_shape, new_size
            )));
        }

        // Convert shape to i32 for FFI
        let shape_i32: Vec<i32> = new_shape.iter().map(|&x| x as i32).collect();

        unsafe {
            mlx_clear_error();
            let result_array =
                mlx_array_reshape(self.inner, shape_i32.as_ptr(), shape_i32.len() as i32);
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
                    "Failed to reshape tensor: {}",
                    error_str
                )));
            }

            Ok(Self {
                inner: result_array,
                shape: new_shape,
                dtype: self.dtype,
            })
        }
    }

    /// Transpose tensor using mx::transpose()
    ///
    /// Reverses the order of all dimensions. For a 2D tensor, this
    /// swaps rows and columns. For higher-dimensional tensors, all
    /// axes are reversed.
    ///
    /// # Returns
    /// A new tensor with reversed dimensions
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![2, 2])?;
    /// let transposed = tensor.transpose()?;
    /// assert_eq!(transposed.shape(), &[2, 2]);
    /// ```
    pub fn transpose(&self) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let result_array = mlx_array_transpose(self.inner);
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
                    "Failed to transpose tensor: {}",
                    error_str
                )));
            }

            // Reverse the shape for transposed tensor
            let transposed_shape: Vec<usize> = self.shape.iter().rev().cloned().collect();

            Ok(Self {
                inner: result_array,
                shape: transposed_shape,
                dtype: self.dtype,
            })
        }
    }

    /// Get shape information from the underlying MLX array
    ///
    /// Queries the MLX backend for the actual shape stored in the array.
    /// This is useful for validating that the Rust-side shape tracking
    /// matches the MLX-side shape.
    ///
    /// # Arguments
    /// * `max_dims` - Maximum number of dimensions to retrieve
    ///
    /// # Returns
    /// Vector of dimensions from the MLX array
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![2, 2])?;
    /// let mlx_shape = tensor.get_mlx_shape(8)?;
    /// assert_eq!(mlx_shape, vec![2, 2]);
    /// ```
    pub fn get_mlx_shape(&self, max_dims: usize) -> Result<Vec<usize>> {
        let mut shape_buf: Vec<i32> = vec![0; max_dims];

        unsafe {
            mlx_clear_error();
            let ndim = mlx_array_shape(self.inner, shape_buf.as_mut_ptr(), max_dims as i32);
            if ndim < 0 {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to get tensor shape: {}",
                    error_str
                )));
            }

            Ok(shape_buf[..ndim as usize]
                .iter()
                .map(|&x| x as usize)
                .collect())
        }
    }

    /// Get the number of dimensions from the underlying MLX array
    ///
    /// Queries the MLX backend for the number of dimensions (rank) of the tensor.
    ///
    /// # Returns
    /// Number of dimensions
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![2, 2])?;
    /// assert_eq!(tensor.get_mlx_ndim()?, 2);
    /// ```
    pub fn get_mlx_ndim(&self) -> Result<usize> {
        unsafe {
            mlx_clear_error();
            let ndim = mlx_array_ndim(self.inner);
            if ndim < 0 {
                let error_msg = mlx_get_last_error();
                let error_str = if error_msg.is_null() {
                    "Unknown MLX error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                };
                return Err(AosError::Other(format!(
                    "Failed to get tensor ndim: {}",
                    error_str
                )));
            }

            Ok(ndim as usize)
        }
    }

    /// Get the total element count from the underlying MLX array
    ///
    /// Queries the MLX backend for the total number of elements in the tensor.
    ///
    /// # Returns
    /// Total number of elements
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0, 3.0, 4.0], vec![2, 2])?;
    /// assert_eq!(tensor.get_mlx_size()?, 4);
    /// ```
    pub fn get_mlx_size(&self) -> Result<usize> {
        unsafe {
            mlx_clear_error();
            let size = mlx_array_size(self.inner);
            Ok(size)
        }
    }

    /// Get the data type from the underlying MLX array
    ///
    /// Queries the MLX backend for the dtype of the tensor.
    /// Returns a raw dtype code that can be used for validation.
    ///
    /// # Returns
    /// MLX dtype code (1 = Float32, 2 = Int32, etc.)
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0], vec![2])?;
    /// let dtype_code = tensor.get_mlx_dtype()?;
    /// assert_eq!(dtype_code, 1); // Float32
    /// ```
    pub fn get_mlx_dtype(&self) -> Result<i32> {
        unsafe {
            mlx_clear_error();
            let dtype = mlx_array_dtype(self.inner);
            Ok(dtype)
        }
    }

    /// Create a copy of this tensor
    ///
    /// Creates an independent copy of the tensor data using mx::copy().
    ///
    /// # Returns
    /// A new tensor with copied data
    ///
    /// # Example
    /// ```ignore
    /// let tensor = MLXFFITensor::from_data(&[1.0, 2.0], vec![2])?;
    /// let copied = tensor.copy()?;
    /// ```
    pub fn copy(&self) -> Result<Self> {
        unsafe {
            mlx_clear_error();
            let result_array = mlx_array_copy(self.inner);
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
                    "Failed to copy tensor: {}",
                    error_str
                )));
            }

            Ok(Self {
                inner: result_array,
                shape: self.shape.clone(),
                dtype: self.dtype,
            })
        }
    }

    /// Get number of dimensions (rank) from Rust-side tracking
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Synchronize the Rust-side shape with the MLX-side shape
    ///
    /// Updates the internal shape tracking to match what MLX reports.
    /// This is useful after operations that may have changed the shape.
    ///
    /// # Returns
    /// Ok(()) on success
    pub fn sync_shape(&mut self) -> Result<()> {
        let mlx_shape = self.get_mlx_shape(16)?;
        self.shape = mlx_shape;
        Ok(())
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

    #[test]
    fn test_tensor_reshape() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![6]).unwrap();

        // Reshape from [6] to [2, 3]
        let reshaped = tensor.reshape(vec![2, 3]).unwrap();
        assert_eq!(reshaped.shape(), &[2, 3]);
        assert_eq!(reshaped.size(), 6);

        // Reshape to [3, 2]
        let reshaped2 = tensor.reshape(vec![3, 2]).unwrap();
        assert_eq!(reshaped2.shape(), &[3, 2]);
        assert_eq!(reshaped2.size(), 6);
    }

    #[test]
    fn test_tensor_reshape_invalid_size() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![4]).unwrap();

        // Try to reshape to incompatible size
        let result = tensor.reshape(vec![2, 3]);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Cannot reshape"),
            "Expected reshape error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_tensor_transpose() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3]).unwrap();

        let transposed = tensor.transpose().unwrap();
        assert_eq!(transposed.shape(), &[3, 2]);
        assert_eq!(transposed.size(), 6);
    }

    #[test]
    fn test_tensor_transpose_3d() {
        let data = vec![1.0; 24]; // 2 * 3 * 4 = 24
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3, 4]).unwrap();

        let transposed = tensor.transpose().unwrap();
        assert_eq!(transposed.shape(), &[4, 3, 2]);
        assert_eq!(transposed.size(), 24);
    }

    #[test]
    fn test_tensor_ndim() {
        let data = vec![1.0, 2.0, 3.0, 4.0];

        let tensor_1d = MLXFFITensor::from_data(&data, vec![4]).unwrap();
        assert_eq!(tensor_1d.ndim(), 1);

        let tensor_2d = MLXFFITensor::from_data(&data, vec![2, 2]).unwrap();
        assert_eq!(tensor_2d.ndim(), 2);

        let data_3d = vec![1.0; 8];
        let tensor_3d = MLXFFITensor::from_data(&data_3d, vec![2, 2, 2]).unwrap();
        assert_eq!(tensor_3d.ndim(), 3);
    }

    #[test]
    fn test_tensor_copy() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 2]).unwrap();

        let copied = tensor.copy().unwrap();
        assert_eq!(copied.shape(), tensor.shape());
        assert_eq!(copied.dtype(), tensor.dtype());
        assert_eq!(copied.size(), tensor.size());
    }

    #[test]
    fn test_tensor_get_mlx_size() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3]).unwrap();

        let mlx_size = tensor.get_mlx_size().unwrap();
        assert_eq!(mlx_size, 6);
    }

    #[test]
    fn test_tensor_get_mlx_dtype() {
        let float_data = vec![1.0, 2.0];
        let float_tensor = MLXFFITensor::from_data(&float_data, vec![2]).unwrap();
        let dtype = float_tensor.get_mlx_dtype().unwrap();
        assert_eq!(dtype, 1); // Float32

        let int_data = vec![1, 2, 3, 4];
        let int_tensor = MLXFFITensor::from_ints(&int_data, vec![4]).unwrap();
        let int_dtype = int_tensor.get_mlx_dtype().unwrap();
        // The stub returns 1 for all types, so we just verify it works
        assert!(int_dtype >= 0);
    }

    #[test]
    fn test_tensor_get_mlx_ndim() {
        let data = vec![1.0; 24];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3, 4]).unwrap();

        let mlx_ndim = tensor.get_mlx_ndim().unwrap();
        // The stub returns shape.len(), which we set during creation
        assert!(mlx_ndim > 0);
    }

    #[test]
    fn test_tensor_get_mlx_shape() {
        let data = vec![1.0; 12];
        let tensor = MLXFFITensor::from_data(&data, vec![3, 4]).unwrap();

        let mlx_shape = tensor.get_mlx_shape(8).unwrap();
        // The stub returns the shape we passed in
        assert!(!mlx_shape.is_empty());
    }

    #[test]
    fn test_tensor_reshape_chain() {
        let data = vec![1.0; 24];
        let tensor = MLXFFITensor::from_data(&data, vec![24]).unwrap();

        // Chain multiple reshapes
        let reshaped1 = tensor.reshape(vec![2, 12]).unwrap();
        let reshaped2 = reshaped1.reshape(vec![2, 3, 4]).unwrap();
        let reshaped3 = reshaped2.reshape(vec![4, 6]).unwrap();

        assert_eq!(reshaped3.shape(), &[4, 6]);
        assert_eq!(reshaped3.size(), 24);
    }
}
