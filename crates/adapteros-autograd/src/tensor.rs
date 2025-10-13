//! Autograd tensor implementation

use ndarray::Array2;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a tensor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TensorId(pub u64);

impl fmt::Display for TensorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tensor({})", self.0)
    }
}

/// Autograd tensor with gradient tracking
#[derive(Debug, Clone)]
pub struct AutogradTensor {
    /// Unique identifier
    pub id: TensorId,
    /// Tensor data
    pub data: Array2<f32>,
    /// Whether this tensor requires gradient computation
    pub requires_grad: bool,
    /// Gradient tensor (computed during backward pass)
    pub grad: Option<Array2<f32>>,
    /// Gradient function index (reference to operation that created this tensor)
    pub grad_fn: Option<usize>,
}

impl AutogradTensor {
    /// Create a new autograd tensor
    pub fn new(id: TensorId, data: Array2<f32>, requires_grad: bool) -> Self {
        Self {
            id,
            data,
            requires_grad,
            grad: None,
            grad_fn: None,
        }
    }

    /// Get tensor shape
    pub fn shape(&self) -> &[usize] {
        self.data.shape()
    }

    /// Get tensor size (total elements)
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Check if tensor has gradient
    pub fn has_grad(&self) -> bool {
        self.grad.is_some()
    }

    /// Get gradient tensor
    pub fn grad(&self) -> Option<&Array2<f32>> {
        self.grad.as_ref()
    }

    /// Get mutable gradient tensor
    pub fn grad_mut(&mut self) -> Option<&mut Array2<f32>> {
        self.grad.as_mut()
    }

    /// Set gradient tensor
    pub fn set_grad(&mut self, grad: Array2<f32>) {
        self.grad = Some(grad);
    }

    /// Clear gradient
    pub fn clear_grad(&mut self) {
        self.grad = None;
    }

    /// Detach tensor from computation graph
    pub fn detach(&self) -> Self {
        Self {
            id: self.id,
            data: self.data.clone(),
            requires_grad: false,
            grad: None,
            grad_fn: None,
        }
    }

    /// Get tensor data as view
    pub fn data(&self) -> &Array2<f32> {
        &self.data
    }

    /// Get mutable tensor data
    pub fn data_mut(&mut self) -> &mut Array2<f32> {
        &mut self.data
    }
}

impl fmt::Display for AutogradTensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AutogradTensor(id={}, shape={:?}, requires_grad={}, has_grad={})",
            self.id.0,
            self.shape(),
            self.requires_grad,
            self.has_grad()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_tensor_creation() {
        let data = array![[1.0, 2.0], [3.0, 4.0]];
        let tensor = AutogradTensor::new(TensorId(0), data.clone(), true);

        assert_eq!(tensor.shape(), &[2, 2]);
        assert_eq!(tensor.size(), 4);
        assert!(tensor.requires_grad);
        assert!(!tensor.has_grad());
    }

    #[test]
    fn test_tensor_gradient_operations() {
        let data = array![[1.0, 2.0], [3.0, 4.0]];
        let mut tensor = AutogradTensor::new(TensorId(0), data, true);

        let grad = array![[0.1, 0.2], [0.3, 0.4]];
        tensor.set_grad(grad.clone());

        assert!(tensor.has_grad());
        assert_eq!(tensor.grad(), Some(&grad));

        tensor.clear_grad();
        assert!(!tensor.has_grad());
    }

    #[test]
    fn test_tensor_detach() {
        let data = array![[1.0, 2.0], [3.0, 4.0]];
        let tensor = AutogradTensor::new(TensorId(0), data.clone(), true);

        let detached = tensor.detach();
        assert_eq!(detached.data, data);
        assert!(!detached.requires_grad);
        assert!(!detached.has_grad());
    }

    #[test]
    fn test_tensor_display() {
        let data = array![[1.0, 2.0], [3.0, 4.0]];
        let tensor = AutogradTensor::new(TensorId(42), data, true);

        let display = format!("{}", tensor);
        assert!(display.contains("id=42"));
        assert!(display.contains("shape=[2, 2]"));
        assert!(display.contains("requires_grad=true"));
    }
}
