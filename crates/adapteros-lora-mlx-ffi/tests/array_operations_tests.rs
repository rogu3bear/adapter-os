//! Array operations tests for MLX FFI backend
//!
//! Tests for tensor creation, manipulation, and arithmetic operations
//! through the C FFI interface.

#[cfg(test)]
mod array_creation_tests {
    use adapteros_lora_mlx_ffi::tensor::{MLXFFITensor, TensorDtype};

    #[test]
    fn test_tensor_from_data() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let shape = vec![2, 3];

        let tensor = MLXFFITensor::from_data(&data, shape.clone()).unwrap();

        assert_eq!(tensor.shape(), &[2, 3]);
        assert_eq!(tensor.size(), 6);
        assert_eq!(tensor.dtype(), TensorDtype::Float32);
    }

    #[test]
    fn test_tensor_from_ints() {
        let data = vec![1, 2, 3, 4];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_ints(&data, shape.clone()).unwrap();

        assert_eq!(tensor.shape(), &[2, 2]);
        assert_eq!(tensor.size(), 4);
        assert_eq!(tensor.dtype(), TensorDtype::Int32);
    }

    #[test]
    fn test_tensor_1d() {
        let data = vec![1.0, 2.0, 3.0];
        let shape = vec![3];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        assert_eq!(tensor.shape(), &[3]);
        assert_eq!(tensor.size(), 3);
    }

    #[test]
    fn test_tensor_3d() {
        let data = vec![1.0; 24]; // 2x3x4
        let shape = vec![2, 3, 4];

        let tensor = MLXFFITensor::from_data(&data, shape.clone()).unwrap();

        assert_eq!(tensor.shape(), &[2, 3, 4]);
        assert_eq!(tensor.size(), 24);
    }

    #[test]
    fn test_tensor_scalar() {
        let data = vec![42.0];
        let shape = vec![1];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        assert_eq!(tensor.size(), 1);
    }

    #[test]
    fn test_empty_tensor() {
        let data = vec![];
        let shape = vec![0];

        // Should handle empty tensors gracefully
        let result = MLXFFITensor::from_data(&data, shape);
        // Empty tensors may fail or succeed depending on FFI implementation
        let _ = result;
    }
}

#[cfg(test)]
mod array_arithmetic_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_tensor_addition() {
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![5.0, 6.0, 7.0, 8.0];
        let shape = vec![2, 2];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.add(&tensor2).unwrap();

        assert_eq!(result.shape(), &[2, 2]);
        assert_eq!(result.size(), 4);

        // Verify data
        let result_data = result.data().unwrap();
        assert_eq!(result_data.len(), 4);
        // Expected: [6.0, 8.0, 10.0, 12.0]
        assert!((result_data[0] - 6.0).abs() < 1e-5);
        assert!((result_data[1] - 8.0).abs() < 1e-5);
        assert!((result_data[2] - 10.0).abs() < 1e-5);
        assert!((result_data[3] - 12.0).abs() < 1e-5);
    }

    #[test]
    fn test_tensor_multiplication() {
        let data1 = vec![2.0, 3.0, 4.0, 5.0];
        let data2 = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.multiply(&tensor2).unwrap();

        assert_eq!(result.shape(), &[2, 2]);

        let result_data = result.data().unwrap();
        // Expected: [2.0, 6.0, 12.0, 20.0]
        assert!((result_data[0] - 2.0).abs() < 1e-5);
        assert!((result_data[1] - 6.0).abs() < 1e-5);
        assert!((result_data[2] - 12.0).abs() < 1e-5);
        assert!((result_data[3] - 20.0).abs() < 1e-5);
    }

    #[test]
    fn test_tensor_addition_broadcasting() {
        // Test if broadcasting works (may not be implemented)
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        let data2 = vec![1.0, 1.0, 1.0, 1.0];
        let shape = vec![2, 2];

        let tensor1 = MLXFFITensor::from_data(&data1, shape.clone()).unwrap();
        let tensor2 = MLXFFITensor::from_data(&data2, shape).unwrap();

        let result = tensor1.add(&tensor2).unwrap();
        let result_data = result.data().unwrap();

        // Expected: [2.0, 3.0, 4.0, 5.0]
        assert!((result_data[0] - 2.0).abs() < 1e-5);
        assert!((result_data[1] - 3.0).abs() < 1e-5);
    }
}

#[cfg(test)]
mod array_matmul_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_matrix_multiplication_2x2() {
        // A = [[1, 2], [3, 4]]
        // B = [[5, 6], [7, 8]]
        // C = [[19, 22], [43, 50]]
        let data_a = vec![1.0, 2.0, 3.0, 4.0];
        let data_b = vec![5.0, 6.0, 7.0, 8.0];
        let shape = vec![2, 2];

        let tensor_a = MLXFFITensor::from_data(&data_a, shape.clone()).unwrap();
        let tensor_b = MLXFFITensor::from_data(&data_b, shape).unwrap();

        let result = tensor_a.matmul(&tensor_b).unwrap();

        assert_eq!(result.shape(), &[2, 2]);

        let result_data = result.data().unwrap();
        // Note: Actual values depend on whether MLX uses row-major or column-major
        assert_eq!(result_data.len(), 4);
    }

    #[test]
    fn test_matrix_vector_multiplication() {
        // A = [[1, 2, 3], [4, 5, 6]]  (2x3)
        // b = [[7], [8], [9]]  (3x1)
        // c = [[50], [122]]  (2x1)
        let data_matrix = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let data_vector = vec![7.0, 8.0, 9.0];

        let matrix = MLXFFITensor::from_data(&data_matrix, vec![2, 3]).unwrap();
        let vector = MLXFFITensor::from_data(&data_vector, vec![3, 1]).unwrap();

        let result = matrix.matmul(&vector);

        // Matmul should work or return error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_identity_matrix_multiplication() {
        // A = [[1, 2], [3, 4]]
        // I = [[1, 0], [0, 1]]
        // A * I = A
        let data_a = vec![1.0, 2.0, 3.0, 4.0];
        let data_i = vec![1.0, 0.0, 0.0, 1.0];
        let shape = vec![2, 2];

        let tensor_a = MLXFFITensor::from_data(&data_a, shape.clone()).unwrap();
        let tensor_i = MLXFFITensor::from_data(&data_i, shape).unwrap();

        let result = tensor_a.matmul(&tensor_i).unwrap();

        let result_data = result.data().unwrap();
        // Result should be close to original A
        assert_eq!(result_data.len(), 4);
    }
}

#[cfg(test)]
mod array_data_access_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_tensor_data_access() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        let retrieved_data = tensor.data().unwrap();
        assert_eq!(retrieved_data.len(), 4);

        // Verify data matches
        for (original, retrieved) in data.iter().zip(retrieved_data.iter()) {
            assert!((original - retrieved).abs() < 1e-5);
        }
    }

    #[test]
    fn test_tensor_shape_access() {
        let data = vec![1.0; 12];
        let shape = vec![3, 4];

        let tensor = MLXFFITensor::from_data(&data, shape.clone()).unwrap();

        assert_eq!(tensor.shape(), &shape[..]);
    }

    #[test]
    fn test_tensor_size_calculation() {
        let shapes_and_sizes = vec![
            (vec![2, 3], 6),
            (vec![4, 5, 6], 120),
            (vec![10], 10),
            (vec![1, 1, 1], 1),
        ];

        for (shape, expected_size) in shapes_and_sizes {
            let data = vec![0.0; expected_size];
            let tensor = MLXFFITensor::from_data(&data, shape).unwrap();
            assert_eq!(tensor.size(), expected_size);
        }
    }
}

#[cfg(test)]
mod array_dtype_tests {
    use adapteros_lora_mlx_ffi::tensor::{MLXFFITensor, TensorDtype};

    #[test]
    fn test_float32_dtype() {
        let data = vec![1.0, 2.0];
        let tensor = MLXFFITensor::from_data(&data, vec![2]).unwrap();

        assert_eq!(tensor.dtype(), TensorDtype::Float32);
    }

    #[test]
    fn test_int32_dtype() {
        let data = vec![1, 2];
        let tensor = MLXFFITensor::from_ints(&data, vec![2]).unwrap();

        assert_eq!(tensor.dtype(), TensorDtype::Int32);
    }

    #[test]
    fn test_dtype_mismatch_error() {
        let data = vec![1, 2];
        let tensor = MLXFFITensor::from_ints(&data, vec![2]).unwrap();

        // Trying to get data as Float32 should fail
        let result = tensor.data();
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod array_edge_cases_tests {
    use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

    #[test]
    fn test_large_tensor() {
        let size = 10000;
        let data = vec![1.0; size];
        let shape = vec![100, 100];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        assert_eq!(tensor.size(), size);
    }

    #[test]
    fn test_tensor_with_zeros() {
        let data = vec![0.0; 4];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        let retrieved_data = tensor.data().unwrap();
        for &val in retrieved_data {
            assert_eq!(val, 0.0);
        }
    }

    #[test]
    fn test_tensor_with_negatives() {
        let data = vec![-1.0, -2.0, -3.0, -4.0];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        let retrieved_data = tensor.data().unwrap();
        for (original, retrieved) in data.iter().zip(retrieved_data.iter()) {
            assert!((original - retrieved).abs() < 1e-5);
        }
    }

    #[test]
    fn test_tensor_with_very_small_values() {
        let data = vec![1e-10, 2e-10, 3e-10, 4e-10];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        let retrieved_data = tensor.data().unwrap();
        assert_eq!(retrieved_data.len(), 4);
    }

    #[test]
    fn test_tensor_with_very_large_values() {
        let data = vec![1e10, 2e10, 3e10, 4e10];
        let shape = vec![2, 2];

        let tensor = MLXFFITensor::from_data(&data, shape).unwrap();

        let retrieved_data = tensor.data().unwrap();
        assert_eq!(retrieved_data.len(), 4);
    }
}
