//! Multi-LoRA routing with K-sparse gating

use crate::lora::LoRAAdapter;
use adapteros_core::{AosError, Result};

/// Apply multiple LoRA adapters with K-sparse gating
///
/// # Arguments
/// * `adapters` - List of LoRA adapters to apply
/// * `gates` - Gate values for each adapter (Q15 quantized, 0-32767)
/// * `module_name` - Target module name
/// * `input` - Input activations
/// * `base_output` - Base model output
///
/// # Returns
/// Combined output with weighted LoRA contributions
pub fn apply_multi_lora(
    adapters: &[&LoRAAdapter],
    gates: &[u16],
    module_name: &str,
    input: &[f32],
    base_output: &[f32],
) -> Result<Vec<f32>> {
    if adapters.len() != gates.len() {
        return Err(AosError::Mlx(format!(
            "Adapter count ({}) != gate count ({})",
            adapters.len(),
            gates.len()
        )));
    }

    if adapters.is_empty() {
        return Ok(base_output.to_vec());
    }

    // Start with base output
    let mut result = base_output.to_vec();

    // Apply each adapter with its gate weight
    for (adapter, &gate) in adapters.iter().zip(gates.iter()) {
        if gate == 0 {
            continue; // Skip adapters with zero gate
        }

        // Convert Q15 gate to float (0-32767 -> 0.0-1.0)
        let gate_weight = gate as f32 / 32767.0;

        // Apply LoRA
        let lora_output = adapter.apply(module_name, input, base_output)?;

        // Weighted combination: result += gate_weight * (lora_output - base_output)
        for i in 0..result.len() {
            let delta = lora_output[i] - base_output[i];
            result[i] += gate_weight * delta;
        }
    }

    Ok(result)
}

/// Select top-K adapters based on gate logits
///
/// # Arguments
/// * `logits` - Raw gate logits for all adapters
/// * `k` - Number of adapters to select
///
/// # Returns
/// (selected_indices, quantized_gates)
pub fn select_top_k(logits: &[f32], k: usize) -> (Vec<usize>, Vec<u16>) {
    let k = k.min(logits.len());

    // Get indices sorted by logit value (descending)
    let mut indexed_logits: Vec<(usize, f32)> =
        logits.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    indexed_logits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top K
    let top_k: Vec<(usize, f32)> = indexed_logits.into_iter().take(k).collect();

    // Apply softmax to top-K logits
    let max_logit = top_k
        .iter()
        .map(|(_, v)| v)
        .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let exp_sum: f32 = top_k.iter().map(|(_, v)| (v - max_logit).exp()).sum();

    let mut indices = Vec::with_capacity(k);
    let mut gates = Vec::with_capacity(k);

    for (idx, logit) in top_k {
        indices.push(idx);

        // Softmax probability
        let prob = ((logit - max_logit).exp()) / exp_sum;

        // Quantize to Q15 (0.0-1.0 -> 0-32767)
        let gate_q15 = (prob * 32767.0).round().clamp(0.0, 32767.0) as u16;
        gates.push(gate_q15);
    }

    (indices, gates)
}

/// Apply entropy floor to prevent single-adapter collapse
///
/// # Arguments
/// * `gates` - Q15 quantized gates
/// * `entropy_floor` - Minimum entropy (0.0-1.0)
///
/// # Returns
/// Adjusted gates with entropy floor applied
pub fn apply_entropy_floor(gates: &[u16], entropy_floor: f32) -> Vec<u16> {
    if gates.is_empty() {
        return gates.to_vec();
    }

    // Convert to probabilities
    let mut probs: Vec<f32> = gates.iter().map(|&g| g as f32 / 32767.0).collect();

    // Calculate current entropy
    let entropy = -probs
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|&p| p * p.ln())
        .sum::<f32>();

    let max_entropy = (gates.len() as f32).ln();
    let normalized_entropy = entropy / max_entropy;

    // If entropy is too low, mix in uniform distribution
    if normalized_entropy < entropy_floor {
        let uniform_weight = (entropy_floor - normalized_entropy) / (1.0 - normalized_entropy);
        let uniform_prob = 1.0 / gates.len() as f32;

        for prob in &mut probs {
            *prob = (1.0 - uniform_weight) * (*prob) + uniform_weight * uniform_prob;
        }

        // Renormalize
        let sum: f32 = probs.iter().sum();
        for prob in &mut probs {
            *prob /= sum;
        }
    }

    // Convert back to Q15
    probs
        .iter()
        .map(|&p| (p * 32767.0).round().clamp(0.0, 32767.0) as u16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_top_k() {
        let logits = vec![1.0, 3.0, 2.0, 0.5, 2.5];
        let (indices, gates) = select_top_k(&logits, 3);

        // Should select indices 1, 4, 2 (logits 3.0, 2.5, 2.0)
        assert_eq!(indices.len(), 3);
        assert_eq!(indices[0], 1); // Highest logit
        assert_eq!(gates.len(), 3);

        // Gates should sum to approximately 32767 (Q15 max)
        let sum: u32 = gates.iter().map(|&g| g as u32).sum();
        assert!((sum as i32 - 32767).abs() < 100); // Allow small rounding error
    }

    #[test]
    fn test_select_top_k_with_k_larger_than_logits() {
        let logits = vec![1.0, 2.0];
        let (indices, gates) = select_top_k(&logits, 5);

        // Should only select 2 adapters
        assert_eq!(indices.len(), 2);
        assert_eq!(gates.len(), 2);
    }

    #[test]
    fn test_apply_entropy_floor() {
        // Highly peaked distribution (low entropy)
        let gates = vec![32000, 500, 267]; // ~0.976, 0.015, 0.008

        let adjusted = apply_entropy_floor(&gates, 0.5);

        // Should be more uniform after adjustment
        let max_gate = *adjusted.iter().max().expect("Should have max gate");
        let min_gate = *adjusted.iter().min().expect("Should have min gate");

        // Gap should be smaller than original
        assert!(max_gate - min_gate < 32000 - 267);
    }

    #[test]
    fn test_entropy_floor_no_change_when_high_entropy() {
        // Already uniform distribution
        let gates = vec![10922, 10922, 10923]; // ~1/3 each

        let adjusted = apply_entropy_floor(&gates, 0.5);

        // Should remain mostly unchanged
        for (orig, adj) in gates.iter().zip(adjusted.iter()) {
            assert!((*orig as i32 - *adj as i32).abs() < 100);
        }
    }
}
