//! Rust autograd system for AdapterOS
//!
//! This crate provides a deterministic autograd implementation for LoRA training,
//! avoiding Python dependencies while maintaining reproducibility.

use adapteros_core::{AosError, Result};
use ndarray::Array2;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

pub mod gradients;
pub mod loss;
pub mod operations;
pub mod tensor;

pub use gradients::{GradientAccumulator, GradientTracker};
pub use loss::{LossFunction, LossType};
pub use operations::{Operation, OperationType};
pub use tensor::{AutogradTensor, TensorId};

/// Autograd context for managing computation graph
#[derive(Debug)]
pub struct AutogradContext {
    /// Global random number generator with deterministic seeding
    rng: Arc<parking_lot::Mutex<ChaCha20Rng>>,
    /// Operation history for backpropagation
    operations: Vec<Operation>,
    /// Tensor registry
    tensors: HashMap<TensorId, AutogradTensor>,
    /// Next tensor ID
    next_tensor_id: u64,
}

impl AutogradContext {
    /// Create a new autograd context with deterministic seeding
    pub fn new(seed: u64) -> Self {
        let rng = Arc::new(parking_lot::Mutex::new(ChaCha20Rng::seed_from_u64(seed)));

        info!("Created autograd context with seed: {}", seed);

        Self {
            rng,
            operations: Vec::new(),
            tensors: HashMap::new(),
            next_tensor_id: 0,
        }
    }

    /// Create a new tensor with automatic gradient tracking
    pub fn tensor(&mut self, data: Array2<f32>, requires_grad: bool) -> TensorId {
        let id = TensorId(self.next_tensor_id);
        self.next_tensor_id += 1;

        let tensor = AutogradTensor {
            id,
            data,
            requires_grad,
            grad: None,
            grad_fn: None,
        };

        self.tensors.insert(id, tensor);
        id
    }

    /// Get tensor by ID
    pub fn get_tensor(&self, id: TensorId) -> Option<&AutogradTensor> {
        self.tensors.get(&id)
    }

    /// Get mutable tensor by ID
    pub fn get_tensor_mut(&mut self, id: TensorId) -> Option<&mut AutogradTensor> {
        self.tensors.get_mut(&id)
    }

    /// Perform matrix multiplication with gradient tracking
    pub fn matmul(&mut self, a: TensorId, b: TensorId) -> Result<TensorId> {
        let tensor_a = self
            .get_tensor(a)
            .ok_or_else(|| AosError::Autograd("Tensor A not found".to_string()))?;
        let tensor_b = self
            .get_tensor(b)
            .ok_or_else(|| AosError::Autograd("Tensor B not found".to_string()))?;

        // Perform forward pass
        let result_data = tensor_a.data.dot(&tensor_b.data);
        let requires_grad = tensor_a.requires_grad || tensor_b.requires_grad;

        let result_id = self.tensor(result_data, requires_grad);

        // Record operation for backpropagation
        let operation = Operation {
            op_type: OperationType::MatMul,
            inputs: vec![a, b],
            output: result_id,
            backward_fn: Some(Box::new(move |ctx, grad_output| {
                Self::matmul_backward(ctx, a, b, result_id, grad_output)
            })),
        };

        self.operations.push(operation);

        // Set gradient function
        let op_index = self.operations.len() - 1;
        if let Some(tensor) = self.get_tensor_mut(result_id) {
            tensor.grad_fn = Some(op_index);
        }

        debug!("MatMul operation: {} x {} -> {}", a.0, b.0, result_id.0);
        Ok(result_id)
    }

    /// Perform element-wise addition with gradient tracking
    pub fn add(&mut self, a: TensorId, b: TensorId) -> Result<TensorId> {
        let tensor_a = self
            .get_tensor(a)
            .ok_or_else(|| AosError::Autograd("Tensor A not found".to_string()))?;
        let tensor_b = self
            .get_tensor(b)
            .ok_or_else(|| AosError::Autograd("Tensor B not found".to_string()))?;

        // Perform forward pass
        let result_data = &tensor_a.data + &tensor_b.data;
        let requires_grad = tensor_a.requires_grad || tensor_b.requires_grad;

        let result_id = self.tensor(result_data, requires_grad);

        // Record operation for backpropagation
        let operation = Operation {
            op_type: OperationType::Add,
            inputs: vec![a, b],
            output: result_id,
            backward_fn: Some(Box::new(move |ctx, grad_output| {
                Self::add_backward(ctx, a, b, result_id, grad_output)
            })),
        };

        self.operations.push(operation);

        // Set gradient function
        let op_index = self.operations.len() - 1;
        if let Some(tensor) = self.get_tensor_mut(result_id) {
            tensor.grad_fn = Some(op_index);
        }

        debug!("Add operation: {} + {} -> {}", a.0, b.0, result_id.0);
        Ok(result_id)
    }

    /// Perform element-wise multiplication with gradient tracking
    pub fn mul(&mut self, a: TensorId, b: TensorId) -> Result<TensorId> {
        let tensor_a = self
            .get_tensor(a)
            .ok_or_else(|| AosError::Autograd("Tensor A not found".to_string()))?;
        let tensor_b = self
            .get_tensor(b)
            .ok_or_else(|| AosError::Autograd("Tensor B not found".to_string()))?;

        // Perform forward pass
        let result_data = &tensor_a.data * &tensor_b.data;
        let requires_grad = tensor_a.requires_grad || tensor_b.requires_grad;

        let result_id = self.tensor(result_data, requires_grad);

        // Record operation for backpropagation
        let operation = Operation {
            op_type: OperationType::Mul,
            inputs: vec![a, b],
            output: result_id,
            backward_fn: Some(Box::new(move |ctx, grad_output| {
                Self::mul_backward(ctx, a, b, result_id, grad_output)
            })),
        };

        self.operations.push(operation);

        // Set gradient function
        let op_index = self.operations.len() - 1;
        if let Some(tensor) = self.get_tensor_mut(result_id) {
            tensor.grad_fn = Some(op_index);
        }

        debug!("Mul operation: {} * {} -> {}", a.0, b.0, result_id.0);
        Ok(result_id)
    }

    /// Compute gradients for a tensor
    pub fn backward(&mut self, tensor_id: TensorId) -> Result<()> {
        // Initialize gradient for output tensor
        if let Some(tensor) = self.get_tensor_mut(tensor_id) {
            if tensor.requires_grad {
                let shape = tensor.data.shape();
                tensor.grad = Some(Array2::ones((shape[0], shape[1])) * 1.0f32);
            }
        }

        // Backpropagate through operations in reverse order
        // Process in reverse to avoid borrow checker issues - collect operation indices
        let num_ops = self.operations.len();
        for i in (0..num_ops).rev() {
            let should_process = {
                let op = &self.operations[i];
                (op.output == tensor_id || self.has_gradient(op.output)) && op.backward_fn.is_some()
            };

            if should_process {
                let output_id = self.operations[i].output;
                let grad = self.get_gradient(output_id)?;

                // Extract the backward function temporarily to avoid double borrow
                let backward_fn_opt = self.operations[i].backward_fn.take();
                if let Some(backward_fn) = backward_fn_opt {
                    backward_fn(self, grad)?;
                    // Restore the backward function
                    self.operations[i].backward_fn = Some(backward_fn);
                }
            }
        }

        debug!("Backward pass completed for tensor {}", tensor_id.0);
        Ok(())
    }

    /// Check if tensor has gradient
    fn has_gradient(&self, tensor_id: TensorId) -> bool {
        self.tensors
            .get(&tensor_id)
            .and_then(|t| t.grad.as_ref())
            .is_some()
    }

    /// Get gradient for tensor
    fn get_gradient(&self, tensor_id: TensorId) -> Result<Array2<f32>> {
        self.tensors
            .get(&tensor_id)
            .and_then(|t| t.grad.as_ref())
            .cloned()
            .ok_or_else(|| AosError::Autograd(format!("No gradient for tensor {}", tensor_id.0)))
    }

    /// Matrix multiplication backward pass
    fn matmul_backward(
        ctx: &mut AutogradContext,
        a: TensorId,
        b: TensorId,
        _output: TensorId,
        grad_output: Array2<f32>,
    ) -> Result<()> {
        let tensor_a = ctx.get_tensor(a).unwrap();
        let tensor_b = ctx.get_tensor(b).unwrap();
        
        let a_requires_grad = tensor_a.requires_grad;
        let b_requires_grad = tensor_b.requires_grad;
        let a_data = tensor_a.data.clone();
        let b_data = tensor_b.data.clone();

        // dA = grad_output * B^T
        if a_requires_grad {
            let grad_a = grad_output.dot(&b_data.t());
            ctx.accumulate_gradient(a, grad_a)?;
        }
        
        // dB = A^T * grad_output
        if b_requires_grad {
            let grad_b = a_data.t().dot(&grad_output);
            ctx.accumulate_gradient(b, grad_b)?;
        }

        Ok(())
    }

    /// Addition backward pass
    fn add_backward(
        ctx: &mut AutogradContext,
        a: TensorId,
        b: TensorId,
        _output: TensorId,
        grad_output: Array2<f32>,
    ) -> Result<()> {
        let tensor_a = ctx.get_tensor(a).unwrap();
        let tensor_b = ctx.get_tensor(b).unwrap();
        
        let a_requires_grad = tensor_a.requires_grad;
        let b_requires_grad = tensor_b.requires_grad;

        // dA = grad_output
        if a_requires_grad {
            ctx.accumulate_gradient(a, grad_output.clone())?;
        }
        
        // dB = grad_output
        if b_requires_grad {
            ctx.accumulate_gradient(b, grad_output)?;
        }

        Ok(())
    }

    /// Multiplication backward pass
    fn mul_backward(
        ctx: &mut AutogradContext,
        a: TensorId,
        b: TensorId,
        _output: TensorId,
        grad_output: Array2<f32>,
    ) -> Result<()> {
        let tensor_a = ctx.get_tensor(a).unwrap();
        let tensor_b = ctx.get_tensor(b).unwrap();
        
        let a_requires_grad = tensor_a.requires_grad;
        let b_requires_grad = tensor_b.requires_grad;
        let a_data = tensor_a.data.clone();
        let b_data = tensor_b.data.clone();

        // dA = grad_output * B
        if a_requires_grad {
            let grad_a = &grad_output * &b_data;
            ctx.accumulate_gradient(a, grad_a)?;
        }
        
        // dB = grad_output * A
        if b_requires_grad {
            let grad_b = &grad_output * &a_data;
            ctx.accumulate_gradient(b, grad_b)?;
        }

        Ok(())
    }

    /// Accumulate gradient for a tensor
    fn accumulate_gradient(&mut self, tensor_id: TensorId, grad: Array2<f32>) -> Result<()> {
        if let Some(tensor) = self.get_tensor_mut(tensor_id) {
            if tensor.requires_grad {
                match &mut tensor.grad {
                    Some(existing_grad) => {
                        *existing_grad += &grad;
                    }
                    None => {
                        tensor.grad = Some(grad);
                    }
                }
            }
        }
        Ok(())
    }

    /// Get deterministic random number generator
    pub fn rng(&self) -> Arc<parking_lot::Mutex<ChaCha20Rng>> {
        self.rng.clone()
    }

    /// Clear gradients
    pub fn zero_grad(&mut self) {
        for tensor in self.tensors.values_mut() {
            tensor.grad = None;
        }
        debug!("Cleared all gradients");
    }

    /// Get operation count
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Get tensor count
    pub fn tensor_count(&self) -> usize {
        self.tensors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_autograd_context_creation() {
        let mut ctx = AutogradContext::new(42);
        assert_eq!(ctx.tensor_count(), 0);
        assert_eq!(ctx.operation_count(), 0);
    }

    #[test]
    fn test_tensor_creation() {
        let mut ctx = AutogradContext::new(42);
        let data = array![[1.0, 2.0], [3.0, 4.0]];
        let tensor_id = ctx.tensor(data.clone(), true);

        assert_eq!(ctx.tensor_count(), 1);
        let tensor = ctx.get_tensor(tensor_id).unwrap();
        assert_eq!(tensor.data, data);
        assert!(tensor.requires_grad);
    }

    #[test]
    fn test_matmul_forward() {
        let mut ctx = AutogradContext::new(42);
        let a_data = array![[1.0, 2.0], [3.0, 4.0]];
        let b_data = array![[5.0, 6.0], [7.0, 8.0]];

        let a = ctx.tensor(a_data, true);
        let b = ctx.tensor(b_data, true);
        let c = ctx.matmul(a, b).unwrap();

        assert_eq!(ctx.tensor_count(), 3);
        assert_eq!(ctx.operation_count(), 1);

        let result = ctx.get_tensor(c).unwrap();
        let expected = array![[19.0, 22.0], [43.0, 50.0]];
        assert_eq!(result.data, expected);
    }

    #[test]
    fn test_backward_pass() {
        let mut ctx = AutogradContext::new(42);
        let a_data = array![[1.0, 2.0], [3.0, 4.0]];
        let b_data = array![[5.0, 6.0], [7.0, 8.0]];

        let a = ctx.tensor(a_data, true);
        let b = ctx.tensor(b_data, true);
        let c = ctx.matmul(a, b).unwrap();

        ctx.backward(c).unwrap();

        // Check that gradients were computed
        let a_tensor = ctx.get_tensor(a).unwrap();
        assert!(a_tensor.grad.is_some());

        let b_tensor = ctx.get_tensor(b).unwrap();
        assert!(b_tensor.grad.is_some());
    }
}
