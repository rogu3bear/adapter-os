//! LoRA adapter loading and application

use adapteros_core::{AosError, B3Hash, Result};
// PyO3 will be used in future for MLX integration
// use pyo3::prelude::*;
use safetensors::SafeTensors;
use std::collections::HashMap;
use std::path::Path;
use zeroize::Zeroize;

/// LoRA adapter configuration
#[derive(Debug, Clone)]
pub struct LoRAConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha (scaling factor)
    pub alpha: f32,
    /// Target modules (e.g., "q_proj", "v_proj")
    pub target_modules: Vec<String>,
    /// Dropout probability
    pub dropout: f32,
}

impl Default for LoRAConfig {
    fn default() -> Self {
        Self {
            rank: 16,
            alpha: 32.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
            dropout: 0.0,
        }
    }
}

/// LoRA adapter with A and B matrices
pub struct LoRAAdapter {
    /// Adapter ID
    pub id: String,
    /// Configuration
    pub config: LoRAConfig,
    /// LoRA A matrices (down-projection): {module_name: tensor}
    pub lora_a: HashMap<String, Vec<f32>>,
    /// LoRA B matrices (up-projection): {module_name: tensor}
    pub lora_b: HashMap<String, Vec<f32>>,
    /// Shapes for each module: {module_name: (in_features, out_features)}
    pub shapes: HashMap<String, (usize, usize)>,
    /// Content hash
    pub hash: B3Hash,
}

impl LoRAAdapter {
    /// Load LoRA adapter from safetensors file
    ///
    /// # Arguments
    /// * `path` - Path to .safetensors file
    /// * `id` - Adapter identifier
    /// * `config` - LoRA configuration
    ///
    /// # Returns
    /// Loaded LoRA adapter
    pub fn load<P: AsRef<Path>>(path: P, id: String, config: LoRAConfig) -> Result<Self> {
        let path = path.as_ref();

        // Read safetensors file
        let data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read LoRA file: {}", e)))?;

        // Compute hash
        let hash = B3Hash::hash(&data);

        // Parse safetensors
        let tensors = SafeTensors::deserialize(&data)
            .map_err(|e| AosError::Parse(format!("Failed to parse safetensors: {}", e)))?;

        let mut lora_a = HashMap::new();
        let mut lora_b = HashMap::new();
        let mut shapes = HashMap::new();

        // Extract LoRA weights for each target module
        for module in &config.target_modules {
            // Look for lora_A and lora_B tensors
            let a_key = format!("{}.lora_A", module);
            let b_key = format!("{}.lora_B", module);

            if let Ok(a_tensor) = tensors.tensor(&a_key) {
                let a_shape = a_tensor.shape();
                let a_data = a_tensor.data();

                // Convert bytes to f32
                let a_vec = Self::bytes_to_f32(a_data)?;

                if let Ok(b_tensor) = tensors.tensor(&b_key) {
                    let b_shape = b_tensor.shape();
                    let b_data = b_tensor.data();
                    let b_vec = Self::bytes_to_f32(b_data)?;

                    // Validate shapes: A should be [rank, in_features], B should be [out_features, rank]
                    if a_shape.len() == 2 && b_shape.len() == 2 {
                        let rank = a_shape[0];
                        let in_features = a_shape[1];
                        let out_features = b_shape[0];
                        let b_rank = b_shape[1];

                        if rank == b_rank && rank == config.rank {
                            lora_a.insert(module.clone(), a_vec);
                            lora_b.insert(module.clone(), b_vec);
                            shapes.insert(module.clone(), (in_features, out_features));

                            tracing::info!(
                                "Loaded LoRA weights for {}: A=[{}, {}], B=[{}, {}]",
                                module,
                                rank,
                                in_features,
                                out_features,
                                rank
                            );
                        } else {
                            tracing::warn!(
                                "Rank mismatch for {}: expected {}, got A={}, B={}",
                                module,
                                config.rank,
                                rank,
                                b_rank
                            );
                        }
                    }
                }
            }
        }

        if lora_a.is_empty() {
            return Err(AosError::Parse(
                "No LoRA weights found in safetensors file".to_string(),
            ));
        }

        Ok(Self {
            id,
            config,
            lora_a,
            lora_b,
            shapes,
            hash,
        })
    }

    /// Convert bytes to f32 vector (assuming little-endian f32)
    fn bytes_to_f32(bytes: &[u8]) -> Result<Vec<f32>> {
        if bytes.len() % 4 != 0 {
            return Err(AosError::Parse(
                "Byte length not divisible by 4 for f32 conversion".to_string(),
            ));
        }

        let mut result = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            result.push(value);
        }

        Ok(result)
    }

    /// Apply LoRA to a base model output
    ///
    /// Formula: output = base_output + (alpha / rank) * B @ A @ input
    ///
    /// # Arguments
    /// * `module_name` - Name of the module (e.g., "q_proj")
    /// * `input` - Input activations
    /// * `base_output` - Output from base model
    ///
    /// # Returns
    /// Modified output with LoRA applied
    pub fn apply(&self, module_name: &str, input: &[f32], base_output: &[f32]) -> Result<Vec<f32>> {
        // Get LoRA matrices for this module
        let lora_a = self
            .lora_a
            .get(module_name)
            .ok_or_else(|| AosError::Mlx(format!("No LoRA A for module {}", module_name)))?;
        let lora_b = self
            .lora_b
            .get(module_name)
            .ok_or_else(|| AosError::Mlx(format!("No LoRA B for module {}", module_name)))?;
        let (in_features, out_features) = self
            .shapes
            .get(module_name)
            .ok_or_else(|| AosError::Mlx(format!("No shape info for module {}", module_name)))?;

        // Validate input size
        if input.len() != *in_features {
            return Err(AosError::Mlx(format!(
                "Input size mismatch: expected {}, got {}",
                in_features,
                input.len()
            )));
        }

        // Validate output size
        if base_output.len() != *out_features {
            return Err(AosError::Mlx(format!(
                "Output size mismatch: expected {}, got {}",
                out_features,
                base_output.len()
            )));
        }

        let rank = self.config.rank;
        let scale = self.config.alpha / rank as f32;

        // Compute A @ input (down-projection)
        let mut a_out = vec![0.0; rank];
        for i in 0..rank {
            let mut sum = 0.0;
            for j in 0..*in_features {
                sum += lora_a[i * in_features + j] * input[j];
            }
            a_out[i] = sum;
        }

        // Compute B @ (A @ input) (up-projection)
        let mut lora_out = vec![0.0; *out_features];
        for i in 0..*out_features {
            let mut sum = 0.0;
            for j in 0..rank {
                sum += lora_b[i * rank + j] * a_out[j];
            }
            lora_out[i] = sum * scale;
        }

        // Add to base output
        let mut result = base_output.to_vec();
        for i in 0..*out_features {
            result[i] += lora_out[i];
        }

        Ok(result)
    }

    /// Get adapter ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get adapter configuration
    pub fn config(&self) -> &LoRAConfig {
        &self.config
    }

    /// Get content hash
    pub fn hash(&self) -> &B3Hash {
        &self.hash
    }

    /// Get number of target modules
    pub fn num_modules(&self) -> usize {
        self.lora_a.len()
    }
}

impl Drop for LoRAAdapter {
    fn drop(&mut self) {
        // Zeroize LoRA weight vectors
        let mut total_bytes = 0;
        for (_, weights) in &mut self.lora_a {
            total_bytes += weights.len() * 4; // f32 = 4 bytes
            weights.zeroize();
        }
        for (_, weights) in &mut self.lora_b {
            total_bytes += weights.len() * 4;
            weights.zeroize();
        }

        // Emit telemetry via tracing
        tracing::info!(
            adapter_id = %self.id,
            bytes = total_bytes,
            "Adapter weights zeroized"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lora_config_default() {
        let config = LoRAConfig::default();
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.target_modules.len(), 4);
    }

    #[test]
    fn test_lora_apply() {
        let config = LoRAConfig {
            rank: 2,
            alpha: 4.0,
            target_modules: vec!["test".to_string()],
            dropout: 0.0,
        };

        let mut adapter = LoRAAdapter {
            id: "test".to_string(),
            config,
            lora_a: HashMap::new(),
            lora_b: HashMap::new(),
            shapes: HashMap::new(),
            hash: B3Hash::hash(b"test"),
        };

        // Create simple test matrices
        // A: [2, 3] = [[1, 0, 0], [0, 1, 0]]
        let lora_a = vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        // B: [4, 2] = [[1, 0], [0, 1], [1, 1], [0, 0]]
        let lora_b = vec![1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0];

        adapter.lora_a.insert("test".to_string(), lora_a);
        adapter.lora_b.insert("test".to_string(), lora_b);
        adapter.shapes.insert("test".to_string(), (3, 4));

        // Test input and base output
        let input = vec![1.0, 2.0, 3.0];
        let base_output = vec![0.0, 0.0, 0.0, 0.0];

        let result = adapter
            .apply("test", &input, &base_output)
            .expect("Test adapter apply should succeed");

        // With scale = 4.0 / 2 = 2.0
        // A @ input = [1, 2]
        // B @ [1, 2] = [1, 2, 3, 0]
        // Scaled: [2, 4, 6, 0]
        assert_eq!(result.len(), 4);
        assert!((result[0] - 2.0).abs() < 1e-5);
        assert!((result[1] - 4.0).abs() < 1e-5);
        assert!((result[2] - 6.0).abs() < 1e-5);
        assert!((result[3] - 0.0).abs() < 1e-5);
    }
}
