//! Core domain adapter trait and types

use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_numerics::noise::{Tensor, EpsilonStats};
use adapteros_trace::Event;
use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{DomainAdapterError, Result};

/// Wrapper for tensor data with metadata
#[derive(Debug, Clone)]
pub struct TensorData {
    /// The actual tensor
    pub tensor: Tensor,
    /// Metadata about this tensor
    pub metadata: TensorMetadata,
}

/// Metadata associated with a tensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorMetadata {
    /// Hash of the tensor data (for deterministic verification)
    pub hash: B3Hash,
    /// Shape of the tensor
    pub shape: Vec<usize>,
    /// Data type (e.g., "f32", "f16", "q8")
    pub dtype: String,
    /// Total number of elements
    pub element_count: usize,
    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

impl TensorData {
    /// Create new tensor data with computed metadata
    pub fn new(tensor: Tensor, dtype: String) -> Self {
        let shape = tensor.shape.clone();
        let element_count = tensor.len();
        
        // Compute hash of tensor data
        let hash = B3Hash::hash(&Self::serialize_tensor_for_hash(&tensor));
        
        Self {
            tensor,
            metadata: TensorMetadata {
                hash,
                shape,
                dtype,
                element_count,
                custom: HashMap::new(),
            },
        }
    }
    
    /// Add custom metadata
    pub fn with_custom_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.custom.insert(key, value);
        self
    }
    
    /// Verify tensor hash
    pub fn verify_hash(&self) -> bool {
        let computed_hash = B3Hash::hash(&Self::serialize_tensor_for_hash(&self.tensor));
        computed_hash == self.metadata.hash
    }
    
    /// Serialize tensor data for hashing (deterministic)
    fn serialize_tensor_for_hash(tensor: &Tensor) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Add shape
        for dim in &tensor.shape {
            bytes.extend_from_slice(&dim.to_le_bytes());
        }
        
        // Add data
        for val in &tensor.data {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        
        bytes
    }
}

/// Metadata about a domain adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetadata {
    /// Unique name of the adapter
    pub name: String,
    /// Version of the adapter
    pub version: String,
    /// Model hash (BLAKE3)
    pub model_hash: B3Hash,
    /// Input format specification
    pub input_format: String,
    /// Output format specification
    pub output_format: String,
    /// Expected epsilon (numerical drift) threshold
    pub epsilon_threshold: f64,
    /// Whether this adapter is deterministic
    pub deterministic: bool,
    /// Additional metadata
    pub custom: HashMap<String, serde_json::Value>,
}

/// Domain adapter trait
///
/// All domain adapters must implement this trait to integrate with
/// the deterministic execution layer. Adapters are responsible for:
/// - Translating domain-specific inputs to tensors
/// - Performing domain-specific transformations
/// - Maintaining deterministic behavior
/// - Reporting numerical drift (epsilon)
/// - Logging operations to the trace
pub trait DomainAdapter: Send + Sync {
    /// Get adapter name
    fn name(&self) -> &str;
    
    /// Get adapter metadata
    fn metadata(&self) -> &AdapterMetadata;
    
    /// Prepare adapter for execution
    ///
    /// This method is called once during initialization to set up the adapter
    /// with the deterministic executor. It should register any required resources,
    /// allocate memory, and prepare for forward passes.
    ///
    /// # Arguments
    /// * `executor` - The deterministic executor instance
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    fn prepare(&mut self, executor: &mut DeterministicExecutor) -> Result<()>;
    
    /// Forward pass through the adapter
    ///
    /// This is the main inference method that transforms input tensors
    /// to output tensors using domain-specific logic. The implementation
    /// MUST be deterministic - identical inputs must produce identical outputs.
    ///
    /// # Arguments
    /// * `input` - Input tensor data
    ///
    /// # Returns
    /// * `Result<TensorData>` - Output tensor data or error
    fn forward(&mut self, input: &TensorData) -> Result<TensorData>;
    
    /// Postprocess output tensor
    ///
    /// This method applies any final transformations to the output tensor
    /// before it is returned to the caller. Common operations include:
    /// - Normalization
    /// - Quantization
    /// - Format conversion
    ///
    /// # Arguments
    /// * `output` - Output tensor from forward pass
    ///
    /// # Returns
    /// * `Result<TensorData>` - Postprocessed tensor data or error
    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData>;
    
    /// Get current epsilon statistics
    ///
    /// Returns the numerical error statistics for the last forward pass.
    /// This is used to track numerical drift and ensure it stays within
    /// acceptable bounds.
    ///
    /// # Returns
    /// * `Option<EpsilonStats>` - Error statistics if available
    fn epsilon_stats(&self) -> Option<EpsilonStats>;
    
    /// Reset adapter state
    ///
    /// Clears any internal state and prepares the adapter for a new
    /// inference session. This is important for maintaining determinism
    /// across multiple runs.
    fn reset(&mut self);
    
    /// Generate trace event for this operation
    ///
    /// Creates a trace event that will be logged to the trace bundle.
    /// This is used for replay and audit purposes.
    ///
    /// # Arguments
    /// * `tick_id` - Current tick counter
    /// * `op_id` - Operation identifier
    /// * `inputs` - Input data
    /// * `outputs` - Output data
    ///
    /// # Returns
    /// * `Event` - Trace event
    fn create_trace_event(
        &self,
        tick_id: u64,
        op_id: String,
        inputs: &HashMap<String, serde_json::Value>,
        outputs: &HashMap<String, serde_json::Value>,
    ) -> Event;
}

/// Helper trait for domain-specific input preprocessing
pub trait InputPreprocessor {
    /// Preprocess raw input data into tensor format
    fn preprocess(&self, raw_input: &[u8]) -> Result<TensorData>;
}

/// Helper trait for domain-specific output postprocessing
pub trait OutputPostprocessor {
    /// Postprocess tensor output into domain-specific format
    fn postprocess(&self, tensor: &TensorData) -> Result<Vec<u8>>;
}

/// Adapter registration system
pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn DomainAdapter>>,
}

impl AdapterRegistry {
    /// Create a new adapter registry
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }
    
    /// Register an adapter
    pub fn register(&mut self, adapter: Box<dyn DomainAdapter>) -> Result<()> {
        let name = adapter.name().to_string();
        
        if self.adapters.contains_key(&name) {
            return Err(DomainAdapterError::InvalidManifest {
                reason: format!("Adapter {} is already registered", name),
            });
        }
        
        self.adapters.insert(name, adapter);
        Ok(())
    }
    
    /// Get an adapter by name
    pub fn get(&self, name: &str) -> Option<&dyn DomainAdapter> {
        self.adapters.get(name).map(|a| a.as_ref())
    }
    
    /// Get a mutable reference to an adapter
    pub fn get_mut(&mut self, name: &str) -> Option<&mut (dyn DomainAdapter + '_)> {
        if let Some(adapter) = self.adapters.get_mut(name) {
            Some(adapter.as_mut())
        } else {
            None
        }
    }
    
    /// List all registered adapters
    pub fn list_adapters(&self) -> Vec<&str> {
        self.adapters.keys().map(|k| k.as_str()).collect()
    }
    
    /// Remove an adapter
    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if self.adapters.remove(name).is_none() {
            return Err(DomainAdapterError::AdapterNotInitialized {
                adapter_name: name.to_string(),
            });
        }
        Ok(())
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tensor_data_creation() {
        let tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
        let tensor_data = TensorData::new(tensor, "f32".to_string());
        
        assert_eq!(tensor_data.metadata.shape, vec![3]);
        assert_eq!(tensor_data.metadata.element_count, 3);
        assert_eq!(tensor_data.metadata.dtype, "f32");
    }
    
    #[test]
    fn test_tensor_data_hash_verification() {
        let tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
        let tensor_data = TensorData::new(tensor, "f32".to_string());
        
        assert!(tensor_data.verify_hash());
    }
    
    #[test]
    fn test_tensor_data_custom_metadata() {
        let tensor = Tensor::new(vec![1.0], vec![1]);
        let tensor_data = TensorData::new(tensor, "f32".to_string())
            .with_custom_metadata("key".to_string(), serde_json::Value::String("value".to_string()));
        
        assert!(tensor_data.metadata.custom.contains_key("key"));
    }
    
    #[test]
    fn test_adapter_registry() {
        let mut registry = AdapterRegistry::new();
        assert_eq!(registry.list_adapters().len(), 0);
    }
}

