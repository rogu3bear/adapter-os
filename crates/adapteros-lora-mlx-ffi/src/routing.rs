//! Multi-LoRA routing implementation for MLX FFI

use crate::lora::LoRAAdapter;
use adapteros_core::Result;
use tracing::debug;

/// Apply multiple LoRA adapters with weighted routing
///
/// # Arguments
/// * `adapters` - List of active LoRA adapters
/// * `gates` - Q15 quantized gate weights (0-32767)
/// * `module_name` - Target module name
/// * `input` - Input tensor data
/// * `base_output` - Base model output
///
/// # Returns
/// Adapted output with LoRA modifications
pub fn apply_multi_lora(
    adapters: &[&LoRAAdapter],
    gates: &[u16],
    module_name: &str,
    input: &[f32],
    base_output: &[f32],
) -> Result<Vec<f32>> {
    if adapters.is_empty() {
        return Ok(base_output.to_vec());
    }

    debug!(
        "Applying {} LoRA adapters to module {} with gates: {:?}",
        adapters.len(),
        module_name,
        gates
    );

    let mut result = base_output.to_vec();
    let mut total_weight = 0.0;

    // Apply each adapter with its gate weight
    for (adapter, &gate) in adapters.iter().zip(gates.iter()) {
        if !adapter.has_module(module_name) {
            continue;
        }

        let gate_weight = gate as f32 / 32768.0; // Convert Q15 to float
        total_weight += gate_weight;

        // Get LoRA weights for this module
        if let Some((lora_a, lora_b)) = adapter.get_module_weights(module_name) {
            // Apply LoRA transformation: output = input * A^T * B^T
            let lora_output = apply_lora_transform(input, lora_a, lora_b, adapter.config().alpha)?;

            // Weighted combination with base output
            for (i, &lora_val) in lora_output.iter().enumerate() {
                if i < result.len() {
                    result[i] += lora_val * gate_weight;
                }
            }
        }
    }

    // Normalize by total weight if needed
    if total_weight > 0.0 && total_weight != 1.0 {
        for val in &mut result {
            *val /= total_weight;
        }
    }

    debug!(
        "LoRA routing complete: {} adapters, total_weight={:.3}, output_len={}",
        adapters.len(),
        total_weight,
        result.len()
    );

    Ok(result)
}

/// Apply LoRA transformation to input
fn apply_lora_transform(
    input: &[f32],
    lora_a: &[Vec<f32>],
    lora_b: &[Vec<f32>],
    alpha: f32,
) -> Result<Vec<f32>> {
    if lora_a.is_empty() || lora_b.is_empty() {
        return Ok(vec![0.0; input.len()]);
    }

    let rank = lora_a.len();
    let hidden_dim = input.len();

    // First: input * A^T = intermediate (size: rank)
    let mut intermediate = vec![0.0; rank];
    for r in 0..rank {
        for (h_idx, &input_val) in input.iter().enumerate() {
            if h_idx < lora_a[r].len() {
                intermediate[r] += input_val * lora_a[r][h_idx];
            }
        }
    }

    // Second: intermediate * B^T = output (size: hidden_dim)
    let mut output = vec![0.0; hidden_dim];
    for h_idx in 0..hidden_dim {
        if h_idx < lora_b.len() {
            for (r, &inter_val) in intermediate.iter().enumerate() {
                if r < lora_b[h_idx].len() {
                    output[h_idx] += inter_val * lora_b[h_idx][r];
                }
            }
        }
    }

    // Apply alpha scaling
    let scaling = alpha / rank as f32;
    for val in &mut output {
        *val *= scaling;
    }

    Ok(output)
}

/// Compute LoRA adapter score for routing
pub fn compute_adapter_score(
    adapter: &LoRAAdapter,
    _input_features: &[f32],
    module_name: &str,
) -> f32 {
    if !adapter.has_module(module_name) {
        return 0.0;
    }

    // Simple scoring based on adapter rank and alpha
    let rank_score = adapter.config().rank as f32 / 16.0; // Normalize by max rank
    let alpha_score = adapter.config().alpha / 32.0; // Normalize by max alpha

    // Combine scores (simplified)
    (rank_score + alpha_score) / 2.0
}

/// Select top-K adapters based on scores
pub fn select_top_k_adapters(
    _adapters: &[&LoRAAdapter],
    scores: &[f32],
    k: usize,
) -> Vec<(usize, f32)> {
    let mut indexed_scores: Vec<(usize, f32)> = scores
        .iter()
        .enumerate()
        .map(|(i, &score)| (i, score))
        .collect();

    // Sort by score (descending)
    indexed_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Take top-K
    indexed_scores.truncate(k);
    indexed_scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::{LoRAAdapter, LoRAConfig};
    // use std::collections::HashMap; // unused

    fn create_test_adapter(id: &str, rank: usize) -> LoRAAdapter {
        let config = LoRAConfig {
            rank,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.1,
        };

        let mut adapter = LoRAAdapter::new(id.to_string(), config);

        // Add dummy weights
        let lora_a = vec![vec![1.0, 2.0]; rank];
        let lora_b = vec![vec![3.0, 4.0]; 2]; // hidden_dim = 2

        adapter.add_module_weights("q_proj", lora_a, lora_b);
        adapter
    }

    #[test]
    fn test_apply_multi_lora() {
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 2);

        let adapters = vec![&adapter1, &adapter2];
        let gates = vec![16384, 16384]; // 0.5 weight each

        let input = vec![1.0, 2.0];
        let base_output = vec![0.0, 0.0];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        assert_eq!(result.len(), 2);
        // Result should be non-zero due to LoRA application
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_apply_lora_transform() {
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        let input = vec![1.0, 2.0];

        let result = apply_lora_transform(&input, &lora_a, &lora_b, 16.0).unwrap();

        assert_eq!(result.len(), 2);
        // Result should be non-zero
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_compute_adapter_score() {
        let adapter = create_test_adapter("test", 4);
        let input_features = vec![1.0, 2.0, 3.0, 4.0];

        let score = compute_adapter_score(&adapter, &input_features, "q_proj");
        assert!(score > 0.0);

        let score_invalid = compute_adapter_score(&adapter, &input_features, "invalid_module");
        assert_eq!(score_invalid, 0.0);
    }

    #[test]
    fn test_select_top_k_adapters() {
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 4);
        let adapter3 = create_test_adapter("adapter3", 6);

        let adapters = vec![&adapter1, &adapter2, &adapter3];
        let scores = vec![0.3, 0.7, 0.5];

        let top_k = select_top_k_adapters(&adapters, &scores, 2);

        assert_eq!(top_k.len(), 2);
        // Should be sorted by score (descending)
        assert_eq!(top_k[0].0, 1); // adapter2 (score 0.7)
        assert_eq!(top_k[1].0, 2); // adapter3 (score 0.5)
    }
}
