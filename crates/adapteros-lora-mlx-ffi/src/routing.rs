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

        // Get LoRA weights for this module (flattened for contiguous math, cached)
        if let (Some((rank, hidden_dim)), Some((a_flat, b_flat))) = (
            adapter.module_shape(module_name),
            adapter.flatten_module_weights_cached(module_name),
        ) {
            // Apply LoRA transformation: output = input * A^T * B^T
            let lora_output = apply_lora_transform_flat(
                input,
                &a_flat,
                &b_flat,
                rank,
                hidden_dim,
                adapter.config().alpha,
            )?;

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

/// Apply LoRA transformation using contiguous row-major weights.
///
/// - `a_row_major`: rank x hidden_dim, row-major
/// - `b_row_major`: hidden_dim x rank, row-major
fn apply_lora_transform_flat(
    input: &[f32],
    a_row_major: &[f32],
    b_row_major: &[f32],
    rank: usize,
    hidden_dim: usize,
    alpha: f32,
) -> Result<Vec<f32>> {
    if rank == 0 || hidden_dim == 0 || input.is_empty() {
        return Ok(vec![0.0; input.len()]);
    }
    // input length should match hidden_dim; clamp to min for safety
    let len = input.len().min(hidden_dim);

    // First: intermediate[r] = sum_h input[h] * A[r,h]
    let mut intermediate = vec![0.0f32; rank];
    for r in 0..rank {
        let base = r * hidden_dim;
        let mut acc = 0.0f32;
        for h in 0..len {
            acc += input[h] * a_row_major[base + h];
        }
        intermediate[r] = acc;
    }

    // Second: output[h] = sum_r intermediate[r] * B[h,r]
    let mut output = vec![0.0f32; hidden_dim];
    for h in 0..hidden_dim {
        let base = h * rank;
        let mut acc = 0.0f32;
        for r in 0..rank {
            acc += intermediate[r] * b_row_major[base + r];
        }
        output[h] = acc;
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

    fn lcg_next(seed: &mut u64) -> f32 {
        // Simple deterministic LCG PRNG -> [0,1)
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let v = ((*seed >> 33) as u32) as f32 / (u32::MAX as f32);
        v
    }

    fn build_random_lora(
        rank: usize,
        hidden: usize,
        seed: &mut u64,
    ) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        // A: rank x hidden, B: hidden x rank
        let mut a = vec![vec![0.0f32; hidden]; rank];
        for r in 0..rank {
            for h in 0..hidden {
                a[r][h] = lcg_next(seed) * 0.5 - 0.25; // small range around 0
            }
        }
        let mut b = vec![vec![0.0f32; rank]; hidden];
        for h in 0..hidden {
            for r in 0..rank {
                b[h][r] = lcg_next(seed) * 0.5 - 0.25;
            }
        }
        (a, b)
    }

    fn flatten_row_major(a: &[Vec<f32>]) -> Vec<f32> {
        let mut out = Vec::with_capacity(a.len() * a.get(0).map(|r| r.len()).unwrap_or(0));
        for row in a.iter() {
            out.extend_from_slice(row);
        }
        out
    }

    #[test]
    fn test_cpu_flat_parity_across_shapes() {
        let ranks = [1usize, 2, 4];
        let hiddens = [3usize, 8];
        let mut seed = 0xC0FFEEu64;
        let alpha = 11.0f32;

        for &rank in &ranks {
            for &hidden in &hiddens {
                // Build random but deterministic inputs and weights
                let mut input = vec![0.0f32; hidden];
                for h in 0..hidden {
                    input[h] = lcg_next(&mut seed) * 2.0 - 1.0;
                }
                let (a, b) = build_random_lora(rank, hidden, &mut seed);

                let cpu = apply_lora_transform(&input, &a, &b, alpha).unwrap();
                let a_flat = flatten_row_major(&a);
                let b_flat = flatten_row_major(&b);
                let flat = apply_lora_transform_flat(&input, &a_flat, &b_flat, rank, hidden, alpha)
                    .unwrap();

                assert_eq!(cpu.len(), hidden);
                assert_eq!(flat.len(), hidden);
                for i in 0..hidden {
                    let d = (cpu[i] - flat[i]).abs();
                    assert!(
                        d < 1e-6,
                        "mismatch at rank={}, hidden={}, i={} -> cpu={} flat={} Δ={}",
                        rank,
                        hidden,
                        i,
                        cpu[i],
                        flat[i],
                        d
                    );
                }
            }
        }
    }

    #[test]
    fn test_cpu_flat_parity_randomized() {
        // Multiple randomized trials with fixed seed
        let mut seed = 123456789u64;
        let alpha = 7.0f32;
        for _t in 0..5 {
            let rank = 1 + (lcg_next(&mut seed) * 3.0).floor() as usize; // 1..=3
            let hidden = if lcg_next(&mut seed) > 0.5 { 5 } else { 8 };

            let mut input = vec![0.0f32; hidden];
            for h in 0..hidden {
                input[h] = lcg_next(&mut seed) * 2.0 - 1.0;
            }
            let (a, b) = build_random_lora(rank, hidden, &mut seed);
            let cpu = apply_lora_transform(&input, &a, &b, alpha).unwrap();
            let a_flat = flatten_row_major(&a);
            let b_flat = flatten_row_major(&b);
            let flat =
                apply_lora_transform_flat(&input, &a_flat, &b_flat, rank, hidden, alpha).unwrap();
            for i in 0..hidden {
                assert!((cpu[i] - flat[i]).abs() < 1e-6);
            }
        }
    }

    fn manual_lora_apply(input: &[f32], a: &[Vec<f32>], b: &[Vec<f32>], alpha: f32) -> Vec<f32> {
        let rank = a.len();
        let hidden = input.len();
        let mut inter = vec![0.0f32; rank];
        for r in 0..rank {
            for h in 0..hidden {
                inter[r] += input[h] * a[r][h];
            }
        }
        let mut out = vec![0.0f32; hidden];
        for h in 0..hidden {
            for r in 0..rank {
                out[h] += inter[r] * b[h][r];
            }
        }
        let scale = alpha / rank as f32;
        for v in out.iter_mut() {
            *v *= scale;
        }
        out
    }

    #[test]
    fn test_apply_multi_lora_deterministic_exact() {
        // Two small adapters, fixed gates summing to 1.0 exactly (0.5 each)
        let rank = 1usize;
        let hidden = 3usize;
        let alpha = 2.0f32;

        let mut a1 = vec![vec![0.0f32; hidden]; rank];
        let mut b1 = vec![vec![0.0f32; rank]; hidden];
        // Choose simple numbers
        a1[0] = vec![1.0, 0.0, -1.0];
        b1[0][0] = 0.5;
        b1[1][0] = -0.25;
        b1[2][0] = 1.5;

        let mut a2 = vec![vec![0.0f32; hidden]; rank];
        let mut b2 = vec![vec![0.0f32; rank]; hidden];
        a2[0] = vec![0.2, -0.3, 0.4];
        b2[0][0] = -1.0;
        b2[1][0] = 2.0;
        b2[2][0] = 0.25;

        let mut ad1 = LoRAAdapter::new(
            "a1".into(),
            LoRAConfig {
                rank,
                alpha,
                target_modules: vec!["m".into()],
                dropout: 0.0,
            },
        );
        ad1.add_module_weights("m", a1.clone(), b1.clone());

        let mut ad2 = LoRAAdapter::new(
            "a2".into(),
            LoRAConfig {
                rank,
                alpha,
                target_modules: vec!["m".into()],
                dropout: 0.0,
            },
        );
        ad2.add_module_weights("m", a2.clone(), b2.clone());

        let input = vec![0.3f32, -0.2, 0.5];
        let base_output = vec![0.1f32, -0.1, 0.2];
        let adapters = vec![&ad1, &ad2];
        let gates = vec![16384u16, 16384u16]; // 0.5 each with divisor 32768

        // Expected: base + 0.5 * out1 + 0.5 * out2 (no renorm since total=1)
        let out1 = manual_lora_apply(&input, &a1, &b1, alpha);
        let out2 = manual_lora_apply(&input, &a2, &b2, alpha);
        let mut expected = base_output.clone();
        for i in 0..hidden {
            expected[i] += 0.5 * out1[i] + 0.5 * out2[i];
        }

        let got = apply_multi_lora(&adapters, &gates, "m", &input, &base_output).unwrap();
        assert_eq!(got.len(), hidden);
        for i in 0..hidden {
            assert!(
                (got[i] - expected[i]).abs() < 1e-6,
                "i={} got={} expected={}",
                i,
                got[i],
                expected[i]
            );
        }
    }

    #[test]
    fn test_apply_multi_lora_module_missing_noop() {
        // Adapter without the requested module should not change output
        let rank = 2usize;
        let mut ad = LoRAAdapter::new(
            "only_q".into(),
            LoRAConfig {
                rank,
                alpha: 1.0,
                target_modules: vec!["q".into()],
                dropout: 0.0,
            },
        );
        ad.add_module_weights(
            "q",
            vec![vec![1.0, 0.0]; rank],
            vec![vec![0.0, 1.0], vec![1.0, 0.0]],
        );

        let input = vec![0.1f32, 0.2];
        let base_output = vec![0.3f32, -0.4];
        let adapters = vec![&ad];
        let gates = vec![32767u16];
        let got = apply_multi_lora(&adapters, &gates, "absent", &input, &base_output).unwrap();
        assert_eq!(got, base_output);
    }
}
