//! Autograd operations and computation graph

use crate::tensor::TensorId;
use adapteros_core::Result;
use ndarray::Array2;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Type of operation in the computation graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OperationType {
    /// Matrix multiplication
    MatMul,
    /// Element-wise addition
    Add,
    /// Element-wise multiplication
    Mul,
    /// Element-wise division
    Div,
    /// ReLU activation
    ReLU,
    /// Sigmoid activation
    Sigmoid,
    /// Tanh activation
    Tanh,
    /// Softmax activation
    Softmax,
    /// Cross-entropy loss
    CrossEntropy,
    /// Mean squared error loss
    MSE,
}

impl fmt::Display for OperationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OperationType::MatMul => write!(f, "MatMul"),
            OperationType::Add => write!(f, "Add"),
            OperationType::Mul => write!(f, "Mul"),
            OperationType::Div => write!(f, "Div"),
            OperationType::ReLU => write!(f, "ReLU"),
            OperationType::Sigmoid => write!(f, "Sigmoid"),
            OperationType::Tanh => write!(f, "Tanh"),
            OperationType::Softmax => write!(f, "Softmax"),
            OperationType::CrossEntropy => write!(f, "CrossEntropy"),
            OperationType::MSE => write!(f, "MSE"),
        }
    }
}

/// Backward function type
pub type BackwardFn =
    Box<dyn Fn(&mut crate::AutogradContext, Array2<f32>) -> Result<()> + Send + Sync>;

/// Operation in the computation graph
pub struct Operation {
    /// Type of operation
    pub op_type: OperationType,
    /// Input tensor IDs
    pub inputs: Vec<TensorId>,
    /// Output tensor ID
    pub output: TensorId,
    /// Backward function for gradient computation
    pub backward_fn: Option<BackwardFn>,
}

impl std::fmt::Debug for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation")
            .field("op_type", &self.op_type)
            .field("inputs", &self.inputs)
            .field("output", &self.output)
            .field(
                "backward_fn",
                &self.backward_fn.as_ref().map(|_| "<function>"),
            )
            .finish()
    }
}

impl Operation {
    /// Create a new operation
    pub fn new(
        op_type: OperationType,
        inputs: Vec<TensorId>,
        output: TensorId,
        backward_fn: Option<BackwardFn>,
    ) -> Self {
        Self {
            op_type,
            inputs,
            output,
            backward_fn,
        }
    }

    /// Get operation type
    pub fn op_type(&self) -> &OperationType {
        &self.op_type
    }

    /// Get input tensor IDs
    pub fn inputs(&self) -> &[TensorId] {
        &self.inputs
    }

    /// Get output tensor ID
    pub fn output(&self) -> TensorId {
        self.output
    }

    /// Check if operation has backward function
    pub fn has_backward(&self) -> bool {
        self.backward_fn.is_some()
    }

    /// Execute backward pass
    pub fn backward(
        &self,
        ctx: &mut crate::AutogradContext,
        grad_output: Array2<f32>,
    ) -> Result<()> {
        if let Some(ref backward_fn) = self.backward_fn {
            backward_fn(ctx, grad_output)
        } else {
            Ok(())
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inputs_str = self
            .inputs
            .iter()
            .map(|id| format!("{}", id))
            .collect::<Vec<_>>()
            .join(", ");

        write!(
            f,
            "Operation({}, inputs=[{}], output={}, has_backward={})",
            self.op_type,
            inputs_str,
            self.output,
            self.has_backward()
        )
    }
}

/// Computation graph for managing operations
#[derive(Debug)]
pub struct ComputationGraph {
    /// List of operations in execution order
    operations: Vec<Operation>,
    /// Operation count
    op_count: usize,
}

impl ComputationGraph {
    /// Create a new computation graph
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            op_count: 0,
        }
    }

    /// Add an operation to the graph
    pub fn add_operation(&mut self, operation: Operation) {
        self.operations.push(operation);
        self.op_count += 1;
    }

    /// Get operation by index
    pub fn get_operation(&self, index: usize) -> Option<&Operation> {
        self.operations.get(index)
    }

    /// Get operation count
    pub fn operation_count(&self) -> usize {
        self.op_count
    }

    /// Get all operations
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Clear all operations
    pub fn clear(&mut self) {
        self.operations.clear();
        self.op_count = 0;
    }

    /// Get operations in reverse order (for backpropagation)
    pub fn operations_reverse(&self) -> impl Iterator<Item = &Operation> {
        self.operations.iter().rev()
    }

    /// Find operations that produce a given tensor
    pub fn find_producers(&self, tensor_id: TensorId) -> Vec<usize> {
        self.operations
            .iter()
            .enumerate()
            .filter(|(_, op)| op.output == tensor_id)
            .map(|(i, _)| i)
            .collect()
    }

    /// Find operations that consume a given tensor
    pub fn find_consumers(&self, tensor_id: TensorId) -> Vec<usize> {
        self.operations
            .iter()
            .enumerate()
            .filter(|(_, op)| op.inputs.contains(&tensor_id))
            .map(|(i, _)| i)
            .collect()
    }
}

impl Default for ComputationGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::TensorId;

    #[test]
    fn test_operation_creation() {
        let inputs = vec![TensorId(0), TensorId(1)];
        let output = TensorId(2);
        let operation = Operation::new(OperationType::MatMul, inputs.clone(), output, None);

        assert_eq!(operation.op_type(), &OperationType::MatMul);
        assert_eq!(operation.inputs(), &inputs);
        assert_eq!(operation.output(), output);
        assert!(!operation.has_backward());
    }

    #[test]
    fn test_operation_display() {
        let inputs = vec![TensorId(0), TensorId(1)];
        let output = TensorId(2);
        let operation = Operation::new(OperationType::Add, inputs, output, None);

        let display = format!("{}", operation);
        assert!(display.contains("Add"));
        assert!(display.contains("Tensor(0)"));
        assert!(display.contains("Tensor(1)"));
        assert!(display.contains("Tensor(2)"));
    }

    #[test]
    fn test_computation_graph() {
        let mut graph = ComputationGraph::new();
        assert_eq!(graph.operation_count(), 0);

        let operation = Operation::new(OperationType::MatMul, vec![TensorId(0)], TensorId(1), None);
        graph.add_operation(operation);

        assert_eq!(graph.operation_count(), 1);
        assert_eq!(graph.operations().len(), 1);
    }

    #[test]
    fn test_computation_graph_producers_consumers() {
        let mut graph = ComputationGraph::new();

        let op1 = Operation::new(OperationType::MatMul, vec![TensorId(0)], TensorId(1), None);
        let op2 = Operation::new(OperationType::Add, vec![TensorId(1)], TensorId(2), None);

        graph.add_operation(op1);
        graph.add_operation(op2);

        let producers = graph.find_producers(TensorId(1));
        assert_eq!(producers.len(), 1);
        assert_eq!(producers[0], 0);

        let consumers = graph.find_consumers(TensorId(1));
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0], 1);
    }
}
