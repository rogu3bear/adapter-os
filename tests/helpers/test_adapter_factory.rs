//! Test Adapter Factory
//!
//! Utilities for creating small test adapters for kernel integration tests.
//! Provides both realistic adapters (via training pipeline) and synthetic
//! adapters (with specific weight patterns).

use adapteros_core::{AosError, Result};
use safetensors::tensor::{SafeTensors, TensorView};
use safetensors::Dtype;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a minimal test adapter using the training pipeline
///
/// This creates a realistic LoRA adapter by training on minimal data.
/// The adapter will have valid SafeTensors format with all required metadata.
///
/// # Arguments
/// * `rank` - LoRA rank (typically 4-16 for tests)
/// * `alpha` - LoRA alpha scaling factor (typically 2*rank)
///
/// # Returns
/// Raw bytes of the .aos adapter file
pub async fn create_minimal_test_adapter(rank: usize, alpha: f32) -> Result<Vec<u8>> {
    use adapteros_lora_worker::training::{
        AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
    };

    // Create minimal training examples
    let examples = vec![
        TrainingExample::new(vec![1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10]),
        TrainingExample::new(vec![11, 12, 13, 14, 15], vec![16, 17, 18, 19, 20]),
    ];

    // Configure tiny training run
    let config = TrainingConfig {
        rank,
        alpha,
        learning_rate: 1e-3,
        batch_size: 1,
        epochs: 1,
        hidden_dim: 64,
        ..Default::default()
    };

    // Train tiny adapter
    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    let result = trainer.train(&examples, "test_adapter").await?;

    // Package adapter
    let temp_dir = TempDir::with_prefix("aos-test-").map_err(|e| AosError::Io(e.to_string()))?;
    let packager = AdapterPackager::new(temp_dir.path().to_path_buf());

    // Quantize and package
    let quantizer = LoRAQuantizer::new("q15".to_string(), 128, 15);
    let quantized = quantizer.quantize(&result.weights)?;

    let adapter_id = format!("test_adapter_r{}_a{}", rank, alpha as u32);

    let packaged = packager
        .package(
            "default",
            &adapter_id,
            &quantized,
            &config,
            "test_model",
        )
        .await?;

    // Read the packaged .aos file
    let adapter_path = temp_dir.path().join(&adapter_id).join(format!("{}.aos", adapter_id));
    tokio::fs::read(&adapter_path)
        .await
        .map_err(|e| AosError::Io(format!("Failed to read adapter: {}", e)))
}

/// Weight pattern type for synthetic adapters
#[derive(Debug, Clone, Copy)]
pub enum WeightPattern {
    /// All weights are 0.0
    Zeros,
    /// All weights are 1.0
    Ones,
    /// Weights are sequential: 0, 1, 2, 3, ...
    Sequential,
    /// Weights are constant value
    Constant(f32),
    /// Weights are random (seeded for determinism)
    Random(u64),
}

/// Create a synthetic test adapter with specific weight pattern
///
/// This creates an adapter with controlled weight values for testing.
/// Useful for verifying buffer access and basic computations.
///
/// # Arguments
/// * `rank` - LoRA rank (must be ≥ 1)
/// * `alpha` - LoRA alpha scaling factor
/// * `pattern` - Weight pattern to use
///
/// # Returns
/// Raw SafeTensors bytes (not .aos format, just weights)
pub fn create_synthetic_adapter(
    rank: usize,
    alpha: f32,
    pattern: WeightPattern,
) -> Result<Vec<u8>> {
    if rank == 0 {
        return Err(AosError::Validation("rank must be ≥ 1".to_string()));
    }

    // Generate weight value function based on pattern
    let weight_fn: Box<dyn Fn(usize, usize) -> f32> = match pattern {
        WeightPattern::Zeros => Box::new(|_, _| 0.0),
        WeightPattern::Ones => Box::new(|_, _| 1.0),
        WeightPattern::Sequential => Box::new(|i, j| (i * 1000 + j) as f32),
        WeightPattern::Constant(val) => Box::new(move |_, _| val),
        WeightPattern::Random(seed) => {
            use rand::{Rng, SeedableRng};
            use rand_chacha::ChaCha20Rng;
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            Box::new(move |_, _| rng.gen_range(-1.0..1.0))
        }
    };

    // Standard hidden dimension for testing
    const HIDDEN_DIM: usize = 64;

    // Create tensors for each target module
    let mut tensors: Vec<(String, TensorData)> = Vec::new();

    // Target modules: q_proj, k_proj, v_proj, mlp.down_proj, mlp.up_proj
    for module in &[
        "q_proj",
        "k_proj",
        "v_proj",
        "mlp.down_proj",
        "mlp.up_proj",
    ] {
        // LoRA A matrix: [rank, hidden_dim] - downsample
        let mut a_data = Vec::with_capacity(rank * HIDDEN_DIM);
        for i in 0..rank {
            for j in 0..HIDDEN_DIM {
                a_data.push(weight_fn(i, j));
            }
        }

        tensors.push((
            format!("{}.lora_A.weight", module),
            TensorData {
                data: a_data,
                shape: vec![rank, HIDDEN_DIM],
                dtype: Dtype::F32,
            },
        ));

        // LoRA B matrix: [hidden_dim, rank] - upsample
        let mut b_data = Vec::with_capacity(HIDDEN_DIM * rank);
        for i in 0..HIDDEN_DIM {
            for j in 0..rank {
                b_data.push(weight_fn(i, j));
            }
        }

        tensors.push((
            format!("{}.lora_B.weight", module),
            TensorData {
                data: b_data,
                shape: vec![HIDDEN_DIM, rank],
                dtype: Dtype::F32,
            },
        ));
    }

    // Add metadata
    let mut metadata = HashMap::new();
    metadata.insert("lora_rank".to_string(), rank.to_string());
    metadata.insert("lora_alpha".to_string(), alpha.to_string());
    metadata.insert("base_model".to_string(), "test_model".to_string());

    // Serialize to SafeTensors format
    serialize_safetensors_with_metadata(&tensors, metadata)
}

/// Tensor data container for serialization
struct TensorData {
    data: Vec<f32>,
    shape: Vec<usize>,
    dtype: Dtype,
}

/// Serialize tensors to SafeTensors format with metadata
fn serialize_safetensors_with_metadata(
    tensors: &[(String, TensorData)],
    metadata: HashMap<String, String>,
) -> Result<Vec<u8>> {
    use safetensors::tensor::Tensor;

    // Convert to SafeTensors format
    let tensors_map: HashMap<String, Tensor<'_>> = tensors
        .iter()
        .map(|(name, tensor_data)| {
            // Convert f32 data to bytes
            let data_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    tensor_data.data.as_ptr() as *const u8,
                    tensor_data.data.len() * std::mem::size_of::<f32>(),
                )
            };

            let tensor = Tensor::new(tensor_data.dtype, tensor_data.shape.clone(), data_bytes);

            (name.clone(), tensor)
        })
        .collect();

    // Serialize
    safetensors::tensor::serialize(&tensors_map, &Some(metadata))
        .map_err(|e| AosError::Serialization(format!("SafeTensors serialization failed: {}", e)))
}

/// Create an adapter with constant weight values
///
/// Convenience wrapper around `create_synthetic_adapter` for constant patterns.
pub fn create_adapter_with_constant_weights(rank: usize, value: f32) -> Result<Vec<u8>> {
    create_synthetic_adapter(rank, value * 2.0, WeightPattern::Constant(value))
}

/// Sample values from a Metal buffer (for testing)
///
/// # Safety
/// This function performs unsafe pointer operations to read Metal buffer contents.
/// It assumes the buffer contains valid f32 values.
pub unsafe fn sample_buffer(buffer: &metal::Buffer, count: usize) -> Result<Vec<f32>> {
    let contents = buffer.contents();
    let buffer_len = buffer.length() as usize;
    let f32_count = buffer_len / std::mem::size_of::<f32>();

    if count > f32_count {
        return Err(AosError::Validation(format!(
            "Requested {} samples but buffer only has {} f32 values",
            count, f32_count
        )));
    }

    let data_ptr = contents as *const f32;
    let slice = std::slice::from_raw_parts(data_ptr, count);

    Ok(slice.to_vec())
}

/// Compute L2 distance between two vectors
pub fn compute_l2_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        panic!("Vectors must have same length");
    }

    let sum_sq_diff: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum();

    sum_sq_diff.sqrt()
}

/// Assert approximate equality with epsilon
#[macro_export]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, epsilon: $epsilon:expr) => {
        let distance = $crate::helpers::test_adapter_factory::compute_l2_distance($left, $right);
        assert!(
            distance < $epsilon,
            "Vectors not approximately equal:\n  L2 distance: {}\n  epsilon: {}",
            distance,
            $epsilon
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_adapter_zeros() -> Result<()> {
        let adapter_bytes = create_synthetic_adapter(4, 8.0, WeightPattern::Zeros)?;
        assert!(!adapter_bytes.is_empty());

        // Verify SafeTensors can be loaded
        let tensors = SafeTensors::deserialize(&adapter_bytes)
            .map_err(|e| AosError::Serialization(e.to_string()))?;

        // Check q_proj.lora_A exists
        let tensor = tensors
            .tensor("q_proj.lora_A.weight")
            .map_err(|e| AosError::Serialization(e.to_string()))?;

        assert_eq!(tensor.shape(), &[4, 64]);

        // Check all values are zero
        let data = tensor.data();
        let values: &[f32] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };

        assert!(values.iter().all(|&v| v == 0.0));

        Ok(())
    }

    #[test]
    fn test_synthetic_adapter_ones() -> Result<()> {
        let adapter_bytes = create_synthetic_adapter(4, 8.0, WeightPattern::Ones)?;
        let tensors = SafeTensors::deserialize(&adapter_bytes)
            .map_err(|e| AosError::Serialization(e.to_string()))?;

        let tensor = tensors
            .tensor("k_proj.lora_B.weight")
            .map_err(|e| AosError::Serialization(e.to_string()))?;

        let data = tensor.data();
        let values: &[f32] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };

        assert!(values.iter().all(|&v| v == 1.0));

        Ok(())
    }

    #[test]
    fn test_compute_l2_distance() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(compute_l2_distance(&a, &b), 0.0);

        let c = vec![0.0, 0.0, 0.0];
        let d = vec![3.0, 4.0, 0.0];
        assert_eq!(compute_l2_distance(&c, &d), 5.0); // 3-4-5 triangle
    }
}
