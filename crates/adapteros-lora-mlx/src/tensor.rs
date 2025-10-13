//! MLX tensor wrapper and conversion utilities

use adapteros_core::{AosError, Result};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

/// Wrapper for MLX array with shape and dtype information
pub struct MLXTensor {
    /// Python MLX array object
    array: PyObject,
    /// Shape of the tensor
    shape: Vec<usize>,
    /// Data type (float32, float16, etc.)
    dtype: String,
}

impl MLXTensor {
    /// Create MLXTensor from Python object
    pub fn from_py_object(py: Python, obj: PyObject) -> Result<Self> {
        // Get shape
        let shape_obj = obj
            .getattr(py, "shape")
            .map_err(|e| AosError::Mlx(format!("Failed to get shape: {}", e)))?;
        let shape_bound = shape_obj.bind(py);
        let shape_tuple = shape_bound
            .downcast::<PyTuple>()
            .map_err(|e| AosError::Mlx(format!("Failed to downcast shape to tuple: {}", e)))?;

        let shape: Vec<usize> = shape_tuple
            .iter()
            .map(|item| item.extract::<usize>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AosError::Mlx(format!("Failed to extract shape: {}", e)))?;

        // Get dtype
        let dtype_obj = obj
            .getattr(py, "dtype")
            .map_err(|e| AosError::Mlx(format!("Failed to get dtype: {}", e)))?;
        let dtype_bound = dtype_obj.bind(py);
        let dtype: String = dtype_bound
            .str()
            .map_err(|e| AosError::Mlx(format!("Failed to get dtype string: {}", e)))?
            .to_string();

        Ok(Self {
            array: obj,
            shape,
            dtype,
        })
    }

    /// Create MLXTensor from Rust Vec<f32>
    pub fn from_vec(py: Python, data: Vec<f32>, shape: Vec<usize>) -> Result<Self> {
        let mlx = py
            .import_bound("mlx.core")
            .map_err(|e| AosError::Mlx(format!("Failed to import mlx: {}", e)))?;

        // Create Python list from data
        let py_list = PyList::new_bound(py, &data);

        // Create MLX array
        let array = mlx
            .call_method1("array", (py_list,))
            .map_err(|e| AosError::Mlx(format!("Failed to create array: {}", e)))?;

        // Reshape if needed
        let array = if shape.len() > 1 {
            let shape_tuple = PyTuple::new_bound(py, &shape);
            array
                .call_method1("reshape", (shape_tuple,))
                .map_err(|e| AosError::Mlx(format!("Failed to reshape: {}", e)))?
        } else {
            array
        };

        Ok(Self {
            array: array.unbind(),
            shape,
            dtype: "float32".to_string(),
        })
    }

    /// Convert MLXTensor to Rust Vec<f32>
    pub fn to_vec(&self, py: Python) -> Result<Vec<f32>> {
        // Call .tolist() on the MLX array
        let list = self
            .array
            .call_method0(py, "tolist")
            .map_err(|e| AosError::Mlx(format!("Failed to convert to list: {}", e)))?;

        // Flatten and extract
        let flat = self.flatten_python_list(py, list)?;
        Ok(flat)
    }

    /// Recursively flatten nested Python lists
    fn flatten_python_list(&self, py: Python, obj: PyObject) -> Result<Vec<f32>> {
        let obj_bound = obj.bind(py);

        // Try to extract as float
        if let Ok(val) = obj_bound.extract::<f32>() {
            return Ok(vec![val]);
        }

        // Try to extract as list
        if let Ok(list) = obj_bound.downcast::<PyList>() {
            let mut result = Vec::new();
            for item in list.iter() {
                let item_obj = item.unbind();
                let mut flat = self.flatten_python_list(py, item_obj)?;
                result.append(&mut flat);
            }
            return Ok(result);
        }

        Err(AosError::Mlx("Cannot flatten non-list object".to_string()))
    }

    /// Get tensor shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get tensor dtype
    pub fn dtype(&self) -> &str {
        &self.dtype
    }

    /// Get underlying PyObject
    pub fn as_py_object(&self) -> &PyObject {
        &self.array
    }

    /// Validate shape matches expected dimensions
    pub fn validate_shape(&self, expected: &[usize]) -> Result<()> {
        if self.shape != expected {
            return Err(AosError::Mlx(format!(
                "Shape mismatch: expected {:?}, got {:?}",
                expected, self.shape
            )));
        }
        Ok(())
    }

    /// Get total number of elements
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_from_vec() {
        Python::with_gil(|py| {
            let data = vec![1.0, 2.0, 3.0, 4.0];
            let shape = vec![2, 2];

            let tensor = MLXTensor::from_vec(py, data.clone(), shape.clone())
                .expect("Test tensor creation should succeed");

            assert_eq!(tensor.shape(), &[2, 2]);
            assert_eq!(tensor.numel(), 4);
        });
    }

    #[test]
    fn test_tensor_roundtrip() {
        Python::with_gil(|py| {
            let original = vec![1.0, 2.0, 3.0, 4.0];
            let shape = vec![4];

            let tensor = MLXTensor::from_vec(py, original.clone(), shape)
                .expect("Test tensor creation should succeed");
            let recovered = tensor
                .to_vec(py)
                .expect("Test tensor conversion should succeed");

            assert_eq!(original, recovered);
        });
    }
}
