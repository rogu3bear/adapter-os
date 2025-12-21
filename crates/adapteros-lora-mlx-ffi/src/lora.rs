//! LoRA adapter implementation for MLX FFI

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LoRA adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Target modules for LoRA adaptation
    pub target_modules: Vec<String>,
    /// Dropout rate
    pub dropout: f32,
    /// Language affinity indices (0=Rust, 1=Python, 2=TypeScript, 3=Go, 4=Java, 5=C++, 6=JavaScript, 7=Other)
    #[serde(default)]
    pub language_affinities: Vec<usize>,
    /// Framework specialization (e.g., "django", "fastapi", "react", "axum")
    #[serde(default)]
    pub framework: Option<String>,
    /// Adapter tier for scoring (e.g., "persistent", "ephemeral", "experimental")
    #[serde(default)]
    pub tier: Option<String>,
}

impl Default for LoRAConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
            dropout: 0.1,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        }
    }
}

/// LoRA adapter with weights
#[derive(Debug, Clone)]
pub struct LoRAAdapter {
    /// Adapter identifier
    pub id: String,
    /// LoRA configuration
    pub config: LoRAConfig,
    /// LoRA A matrices (down-projection) by module name
    pub lora_a: HashMap<String, Vec<Vec<f32>>>,
    /// LoRA B matrices (up-projection) by module name
    pub lora_b: HashMap<String, Vec<Vec<f32>>>,
    /// Weight shapes by module name
    pub shapes: HashMap<String, (usize, usize)>,
    /// Adapter hash for integrity checking
    pub hash: B3Hash,
}

impl LoRAAdapter {
    /// Create a new LoRA adapter
    pub fn new(id: String, config: LoRAConfig) -> Self {
        let hash = B3Hash::hash(id.as_bytes());
        Self {
            id,
            config,
            lora_a: HashMap::new(),
            lora_b: HashMap::new(),
            shapes: HashMap::new(),
            hash,
        }
    }

    /// Add LoRA weights for a module
    pub fn add_module_weights(
        &mut self,
        module_name: &str,
        lora_a: Vec<Vec<f32>>,
        lora_b: Vec<Vec<f32>>,
    ) {
        self.lora_a.insert(module_name.to_string(), lora_a);
        self.lora_b.insert(module_name.to_string(), lora_b);

        // Store shape information
        if let Some(a_matrix) = self.lora_a.get(module_name) {
            if !a_matrix.is_empty() && !a_matrix[0].is_empty() {
                let rows = a_matrix.len();
                let cols = a_matrix[0].len();
                self.shapes.insert(module_name.to_string(), (rows, cols));
            }
        }
    }

    /// Get LoRA weights for a module
    #[allow(clippy::type_complexity)]
    pub fn get_module_weights(
        &self,
        module_name: &str,
    ) -> Option<(&Vec<Vec<f32>>, &Vec<Vec<f32>>)> {
        let lora_a = self.lora_a.get(module_name)?;
        let lora_b = self.lora_b.get(module_name)?;
        Some((lora_a, lora_b))
    }

    /// Get adapter identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get adapter configuration
    pub fn config(&self) -> &LoRAConfig {
        &self.config
    }

    /// Get adapter hash
    pub fn hash(&self) -> B3Hash {
        self.hash
    }

    /// Check if adapter has weights for a module
    pub fn has_module(&self, module_name: &str) -> bool {
        self.lora_a.contains_key(module_name) && self.lora_b.contains_key(module_name)
    }

    /// Get total parameter count
    pub fn parameter_count(&self) -> usize {
        let mut total = 0;

        // Count parameters from both lora_a and lora_b matrices
        for matrix in self.lora_a.values() {
            for row in matrix {
                total += row.len();
            }
        }

        for matrix in self.lora_b.values() {
            for row in matrix {
                total += row.len();
            }
        }

        total
    }

    /// Get memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        self.parameter_count() * 4 // f32 = 4 bytes
    }

    /// Load a LoRA adapter from file
    pub fn load<P: AsRef<std::path::Path>>(
        path: P,
        id: String,
        config: LoRAConfig,
    ) -> Result<Self> {
        let mut adapter = Self::new(id, config);
        let path = path.as_ref();

        // Load safetensors file
        let data = std::fs::read(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read LoRA file {}: {}",
                path.display(),
                e
            ))
        })?;

        let tensors = safetensors::SafeTensors::deserialize(&data)
            .map_err(|e| AosError::Validation(format!("Failed to parse safetensors: {}", e)))?;

        // Extract LoRA weights for each target module
        for module_name in &adapter.config().target_modules.clone() {
            let lora_a_key = format!("{}.lora_A", module_name);
            let lora_b_key = format!("{}.lora_B", module_name);

            match (tensors.tensor(&lora_a_key), tensors.tensor(&lora_b_key)) {
                (Ok(lora_a_tensor), Ok(lora_b_tensor)) => {
                    // Convert tensors to Vec<Vec<f32>>
                    let lora_a = tensor_to_nested_vec(&lora_a_tensor)?;
                    let lora_b = tensor_to_nested_vec(&lora_b_tensor)?;

                    adapter.add_module_weights(module_name, lora_a, lora_b);

                    tracing::debug!(
                        "Loaded LoRA weights for {}: A shape {:?}, B shape {:?}",
                        module_name,
                        lora_a_tensor.shape(),
                        lora_b_tensor.shape()
                    );
                }
                _ => {
                    tracing::warn!(
                        "LoRA weights not found for module {}: expected keys {}.lora_A, {}.lora_B",
                        module_name,
                        module_name,
                        module_name
                    );
                }
            }
        }

        Ok(adapter)
    }
}

/// Convert safetensors tensor view to nested Vec<Vec<f32>>
fn tensor_to_nested_vec(tensor: &safetensors::tensor::TensorView) -> Result<Vec<Vec<f32>>> {
    let shape = tensor.shape();
    let data = tensor.data();

    if shape.len() != 2 {
        return Err(AosError::Validation(format!(
            "Expected 2D tensor, got shape {:?}",
            shape
        )));
    }

    let rows = shape[0];
    let cols = shape[1];

    // Convert bytes to f32
    let float_data: &[f32] = unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const f32,
            data.len() / std::mem::size_of::<f32>(),
        )
    };

    if float_data.len() != rows * cols {
        return Err(AosError::Validation(format!(
            "Data size mismatch: expected {} elements, got {}",
            rows * cols,
            float_data.len()
        )));
    }

    // Convert to nested vec
    let mut result = Vec::with_capacity(rows);
    for i in 0..rows {
        let start = i * cols;
        let end = start + cols;
        result.push(float_data[start..end].to_vec());
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lora_config_default() {
        let config = LoRAConfig::default();
        assert_eq!(config.rank, 4);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(config.target_modules.len(), 4);
        assert_eq!(config.dropout, 0.1);
    }

    #[test]
    fn test_lora_adapter_creation() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        assert_eq!(adapter.id(), "test_adapter");
        assert_eq!(adapter.config().rank, 4);
        assert_eq!(adapter.parameter_count(), 0);
    }

    #[test]
    fn test_lora_adapter_weights() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        // Add weights for a module
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        assert!(adapter.has_module("q_proj"));
        assert_eq!(adapter.parameter_count(), 8); // 2x2 + 2x2 = 8 parameters
        assert_eq!(adapter.memory_usage(), 32); // 8 * 4 bytes
    }

    #[test]
    fn test_lora_adapter_serialization() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        // Test serialization
        let serialized = serde_json::to_string(&adapter.config()).unwrap();
        let deserialized: LoRAConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(adapter.config().rank, deserialized.rank);
        assert_eq!(adapter.config().alpha, deserialized.alpha);
    }
}
