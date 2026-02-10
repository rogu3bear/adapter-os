//! Tests for tensor operations in MLX FFI backend
//!
//! Tests verify basic tensor operations that are available in the public API.
//! Note: Activation functions (relu, gelu, sigmoid, tanh, softmax) are not yet
//! exposed in the MLXFFITensor public API. These tests cover the available operations.

#[cfg(test)]
mod tensor_operation_tests {
    use adapteros_lora_mlx_ffi::tensor::{MLXFFITensor, TensorDtype};

    fn mlx_test_guard() -> parking_lot::ReentrantMutexGuard<'static, ()> {
        adapteros_lora_mlx_ffi::mlx_test_lock_guard()
    }

    /// Test tensor creation from float data
    #[test]
    fn test_tensor_creation_float() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        assert_eq!(tensor.shape(), &[2, 2]);
        assert_eq!(tensor.size(), 4);
        assert_eq!(tensor.dtype(), TensorDtype::Float32);
    }

    /// Test tensor creation from integer data
    #[test]
    fn test_tensor_creation_int() {
        let _guard = mlx_test_guard();
        let data = vec![1, 2, 3, 4];
        let shape = vec![4];

        let tensor = MLXFFITensor::from_ints(&data, shape).unwrap();

        assert_eq!(tensor.shape(), &[4]);
        assert_eq!(tensor.size(), 4);
        assert_eq!(tensor.dtype(), TensorDtype::Int32);
    }

    /// Test tensor addition
    #[test]
    fn test_tensor_add() {
        let _guard = mlx_test_guard();
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![5.0, 6.0, 7.0, 8.0];
        let shape = vec![4];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.add(&tensor2).unwrap();
        assert_eq!(result.shape(), &[4]);
        assert_eq!(result.size(), 4);
    }

    /// Test tensor multiplication
    #[test]
    fn test_tensor_multiply() {
        let _guard = mlx_test_guard();
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![2.0, 2.0, 2.0, 2.0];
        let shape = vec![4];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.multiply(&tensor2).unwrap();
        assert_eq!(result.shape(), &[4]);
        assert_eq!(result.size(), 4);
    }

    /// Test matrix multiplication
    #[test]
    fn test_tensor_matmul() {
        let _guard = mlx_test_guard();
        let data1 = vec![1.0, 2.0, 3.0, 4.0]; // 2x2 matrix
        let data2 = vec![5.0, 6.0, 7.0, 8.0]; // 2x2 matrix
        let shape = vec![2, 2];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.matmul(&tensor2).unwrap();
        assert_eq!(result.shape(), &[2, 2]);
    }

    /// Test tensor reshape
    #[test]
    fn test_tensor_reshape() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![6]).unwrap();

        // Reshape to 2x3
        let reshaped = tensor.reshape(vec![2, 3]).unwrap();
        assert_eq!(reshaped.shape(), &[2, 3]);
        assert_eq!(reshaped.size(), 6);

        // Reshape to 3x2
        let reshaped2 = tensor.reshape(vec![3, 2]).unwrap();
        assert_eq!(reshaped2.shape(), &[3, 2]);
    }

    /// Test tensor reshape invalid size
    #[test]
    fn test_tensor_reshape_invalid() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![4]).unwrap();

        // Try to reshape to incompatible size
        let result = tensor.reshape(vec![2, 3]); // 6 != 4
        assert!(result.is_err());
    }

    /// Test tensor transpose
    #[test]
    fn test_tensor_transpose() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3]).unwrap();

        let transposed = tensor.transpose().unwrap();
        assert_eq!(transposed.shape(), &[3, 2]);
        assert_eq!(transposed.size(), 6);
    }

    /// Test tensor copy
    #[test]
    fn test_tensor_copy() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 2]).unwrap();

        let copied = tensor.copy().unwrap();
        assert_eq!(copied.shape(), tensor.shape());
        assert_eq!(copied.dtype(), tensor.dtype());
        assert_eq!(copied.size(), tensor.size());
    }

    /// Test getting tensor data
    #[test]
    fn test_tensor_data_access() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![4]).unwrap();

        let result_data = tensor.data().unwrap();
        assert_eq!(result_data.len(), 4);
    }

    /// Test getting tensor data as vec
    #[test]
    fn test_tensor_to_float_vec() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![4]).unwrap();

        let result_vec = tensor.to_float_vec().unwrap();
        assert_eq!(result_vec.len(), 4);
    }

    /// Test ndim accessor
    #[test]
    fn test_tensor_ndim() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];

        let tensor_1d = MLXFFITensor::from_data(&data, vec![4]).unwrap();
        assert_eq!(tensor_1d.ndim(), 1);

        let tensor_2d = MLXFFITensor::from_data(&data, vec![2, 2]).unwrap();
        assert_eq!(tensor_2d.ndim(), 2);
    }

    /// Test MLX-side shape retrieval
    #[test]
    fn test_tensor_mlx_shape() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3]).unwrap();

        let mlx_shape = tensor.get_mlx_shape(8).unwrap();
        // Shape should be non-empty
        assert!(!mlx_shape.is_empty());
    }

    /// Test MLX-side ndim retrieval
    #[test]
    fn test_tensor_mlx_ndim() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 2]).unwrap();

        let mlx_ndim = tensor.get_mlx_ndim().unwrap();
        assert!(mlx_ndim > 0);
    }

    /// Test MLX-side size retrieval
    #[test]
    fn test_tensor_mlx_size() {
        let _guard = mlx_test_guard();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3]).unwrap();

        let mlx_size = tensor.get_mlx_size().unwrap();
        assert_eq!(mlx_size, 6);
    }

    /// Test MLX-side dtype retrieval
    #[test]
    fn test_tensor_mlx_dtype() {
        let _guard = mlx_test_guard();
        let float_data = vec![1.0, 2.0];
        let float_tensor = MLXFFITensor::from_data(&float_data, vec![2]).unwrap();

        let dtype = float_tensor.get_mlx_dtype().unwrap();
        // Float32 dtype code should be valid
        assert!(dtype >= 0);
    }

    /// Test chained reshape operations
    #[test]
    fn test_tensor_reshape_chain() {
        let _guard = mlx_test_guard();
        let data = vec![1.0; 24];
        let tensor = MLXFFITensor::from_data(&data, vec![24]).unwrap();

        let reshaped1 = tensor.reshape(vec![2, 12]).unwrap();
        let reshaped2 = reshaped1.reshape(vec![2, 3, 4]).unwrap();
        let reshaped3 = reshaped2.reshape(vec![4, 6]).unwrap();

        assert_eq!(reshaped3.shape(), &[4, 6]);
        assert_eq!(reshaped3.size(), 24);
    }

    /// Test 3D tensor transpose
    #[test]
    fn test_tensor_transpose_3d() {
        let _guard = mlx_test_guard();
        let data = vec![1.0; 24]; // 2 * 3 * 4 = 24
        let tensor = MLXFFITensor::from_data(&data, vec![2, 3, 4]).unwrap();

        let transposed = tensor.transpose().unwrap();
        assert_eq!(transposed.shape(), &[4, 3, 2]);
        assert_eq!(transposed.size(), 24);
    }

    /// Test empty-ish tensor (single element)
    #[test]
    fn test_tensor_single_element() {
        let _guard = mlx_test_guard();
        let data = vec![42.0];
        let tensor = MLXFFITensor::from_data(&data, vec![1]).unwrap();

        assert_eq!(tensor.shape(), &[1]);
        assert_eq!(tensor.size(), 1);
        assert_eq!(tensor.ndim(), 1);
    }

    /// Test large tensor operations
    #[test]
    fn test_tensor_large() {
        let _guard = mlx_test_guard();
        // Create a 1000-element tensor
        let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let tensor = MLXFFITensor::from_data(&data, vec![1000]).unwrap();

        assert_eq!(tensor.size(), 1000);

        // Reshape to 10x100
        let reshaped = tensor.reshape(vec![10, 100]).unwrap();
        assert_eq!(reshaped.shape(), &[10, 100]);
    }
}
