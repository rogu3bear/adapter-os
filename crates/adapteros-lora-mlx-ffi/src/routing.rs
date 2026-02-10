//! Multi-LoRA routing implementation for MLX FFI

use crate::lora::LoRAAdapter;
use adapteros_core::{Result, Q15_GATE_DENOMINATOR};
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
/// Epsilon for gate normalization checks
const GATE_NORMALIZATION_EPSILON: f32 = 0.01;

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

    // Convert Q15 gates to float and compute total for normalization
    let mut gate_weights: Vec<f32> = gates
        .iter()
        .map(|&g| g as f32 / Q15_GATE_DENOMINATOR)
        .collect();

    let total_weight: f32 = gate_weights.iter().sum();

    // Normalize gates BEFORE application if they don't sum to ~1.0
    // Gates are Q15 quantized and should sum to 1.0 by design, but we
    // normalize defensively to handle edge cases (e.g., some adapters
    // missing the target module, or quantization rounding errors)
    if total_weight > 0.0 {
        if (total_weight - 1.0).abs() > GATE_NORMALIZATION_EPSILON {
            debug!(
                "Gate sum {:.4} deviates from 1.0 by more than epsilon, normalizing gates",
                total_weight
            );
            for weight in &mut gate_weights {
                *weight /= total_weight;
            }
        }
        assert!(
            (gate_weights.iter().sum::<f32>() - 1.0).abs() <= GATE_NORMALIZATION_EPSILON + 1e-6,
            "Gates must sum to approximately 1.0 after normalization, got {}",
            gate_weights.iter().sum::<f32>()
        );
    }

    let mut result = base_output.to_vec();

    // Apply each adapter with its normalized gate weight
    for (adapter, &gate_weight) in adapters.iter().zip(gate_weights.iter()) {
        if !adapter.has_module(module_name) {
            continue;
        }

        // Get LoRA weights for this module
        if let Some((lora_a, lora_b)) = adapter.get_module_weights(module_name) {
            // Apply LoRA transformation: output = input * A^T * B^T
            let lora_output = apply_lora_transform(input, lora_a, lora_b, adapter.config().alpha)?;

            // Weighted combination with base output using normalized gate
            for (i, &lora_val) in lora_output.iter().enumerate() {
                if i < result.len() {
                    result[i] += lora_val * gate_weight;
                }
            }
        }
    }

    debug!(
        "LoRA routing complete: {} adapters, original_gate_sum={:.3}, output_len={}",
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

/// Feature vector dimensions for input_features:
/// - [0..8]: Language one-hot (Rust, Python, TypeScript, Go, Java, C++, JavaScript, Other)
/// - [8..11]: Framework scores (normalized 0.0-1.0)
/// - [11]: Symbol hits normalized
/// - [12]: Path tokens normalized
/// - [13+]: Optional additional features
pub const FEATURE_LANGUAGE_START: usize = 0;
pub const FEATURE_LANGUAGE_END: usize = 8;
pub const FEATURE_FRAMEWORK_START: usize = 8;
pub const FEATURE_FRAMEWORK_END: usize = 11;
pub const FEATURE_SYMBOL_HITS: usize = 11;
pub const FEATURE_PATH_TOKENS: usize = 12;
pub const MIN_FEATURE_DIM: usize = 13;

/// Compute LoRA adapter score for routing based on input features
///
/// # Feature-Aware Scoring
/// The score combines:
/// - Base score from adapter rank/alpha (30% weight)
/// - Language affinity score (40% weight)
/// - Framework match score (20% weight)
/// - Tier boost (10% weight)
///
/// # Arguments
/// * `adapter` - The LoRA adapter to score
/// * `input_features` - Feature vector (see FEATURE_* constants for layout)
/// * `module_name` - Target module name
///
/// # Returns
/// Score in range [0.0, 1.0], higher is better
pub fn compute_adapter_score(
    adapter: &LoRAAdapter,
    input_features: &[f32],
    module_name: &str,
) -> f32 {
    if !adapter.has_module(module_name) {
        return 0.0;
    }

    let config = adapter.config();

    // Base score from rank/alpha (normalized)
    let rank_score = (config.rank as f32 / 16.0).min(1.0);
    let alpha_score = (config.alpha / 32.0).min(1.0);
    let base_score = (rank_score + alpha_score) / 2.0;

    // If no features provided, return base score only
    if input_features.len() < MIN_FEATURE_DIM {
        return base_score;
    }

    // Language affinity score
    let language_score = compute_language_affinity(config, input_features);

    // Framework match score
    let framework_score = compute_framework_match(config, input_features);

    // Tier boost (persistent > ephemeral > experimental)
    let tier_boost = match config.tier.as_deref() {
        Some("persistent") => 1.0,
        Some("ephemeral") => 0.7,
        Some("experimental") => 0.4,
        _ => 0.5, // Default tier
    };

    // Weighted combination
    let final_score =
        base_score * 0.3 + language_score * 0.4 + framework_score * 0.2 + tier_boost * 0.1;

    debug!(
        adapter_id = %adapter.id(),
        base_score,
        language_score,
        framework_score,
        tier_boost,
        final_score,
        "Computed adapter score"
    );

    final_score.clamp(0.0, 1.0)
}

/// Compute language affinity between adapter and input features
fn compute_language_affinity(config: &crate::lora::LoRAConfig, input_features: &[f32]) -> f32 {
    if config.language_affinities.is_empty() {
        // No language preference - return neutral score
        return 0.5;
    }

    // Get language one-hot from features [0..8]
    let language_features =
        &input_features[FEATURE_LANGUAGE_START..FEATURE_LANGUAGE_END.min(input_features.len())];

    // Find max activation in input (detected language)
    let (detected_lang, max_activation) = language_features
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((7, &0.0)); // Default to "Other" (index 7)

    // Check if adapter supports detected language
    if config.language_affinities.contains(&detected_lang) {
        // Strong match - boost by activation strength
        0.8 + (max_activation * 0.2)
    } else if config.language_affinities.is_empty() {
        // Adapter is language-agnostic
        0.5
    } else {
        // Language mismatch - penalize
        0.2
    }
}

/// Compute framework match score
fn compute_framework_match(config: &crate::lora::LoRAConfig, input_features: &[f32]) -> f32 {
    // Framework features at indices [8..11]
    let framework_features = if input_features.len() > FEATURE_FRAMEWORK_END {
        &input_features[FEATURE_FRAMEWORK_START..FEATURE_FRAMEWORK_END]
    } else if input_features.len() > FEATURE_FRAMEWORK_START {
        &input_features[FEATURE_FRAMEWORK_START..]
    } else {
        return 0.5; // No framework features
    };

    // Sum of framework activations indicates framework relevance
    let framework_relevance: f32 = framework_features.iter().sum();

    match &config.framework {
        Some(_framework) => {
            // Adapter has framework specialization
            // Higher relevance = better match (assumes framework features are aligned)
            (0.5 + framework_relevance * 0.5).min(1.0)
        }
        None => {
            // No framework specialization - neutral
            0.5
        }
    }
}

/// Select top-K adapters based on scores.
///
/// Ordering: score descending, index ascending for deterministic ties.
/// NaN values are treated as lowest priority (sorted last).
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

    // Sort by score (descending), then index ascending for deterministic ties.
    // NaN values always go to the end.
    indexed_scores.sort_by(|a, b| {
        match (a.1.is_nan(), b.1.is_nan()) {
            (true, true) => a.0.cmp(&b.0),
            (true, false) => std::cmp::Ordering::Greater, // NaN goes to end
            (false, true) => std::cmp::Ordering::Less,    // Non-NaN comes first
            (false, false) => b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)),
        }
    });

    // Take top-K
    indexed_scores.truncate(k);
    indexed_scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::{LoRAAdapter, LoRAConfig};

    fn create_test_adapter(id: &str, rank: usize) -> LoRAAdapter {
        let config = LoRAConfig {
            rank,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.1,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let mut adapter = LoRAAdapter::new(id.to_string(), config);

        // Add dummy weights
        let lora_a = vec![vec![1.0, 2.0]; rank];
        let lora_b = vec![vec![3.0, 4.0]; 2]; // hidden_dim = 2

        adapter.add_module_weights("q_proj", lora_a, lora_b);
        adapter
    }

    fn create_adapter_with_metadata(
        id: &str,
        rank: usize,
        languages: Vec<usize>,
        framework: Option<&str>,
        tier: Option<&str>,
    ) -> LoRAAdapter {
        let config = LoRAConfig {
            rank,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.1,
            language_affinities: languages,
            framework: framework.map(|s| s.to_string()),
            tier: tier.map(|s| s.to_string()),
        };

        let mut adapter = LoRAAdapter::new(id.to_string(), config);
        let lora_a = vec![vec![1.0, 2.0]; rank];
        let lora_b = vec![vec![3.0, 4.0]; 2];
        adapter.add_module_weights("q_proj", lora_a, lora_b);
        adapter
    }

    /// Create a full feature vector for testing (13+ dimensions)
    fn create_feature_vector(language_idx: usize, framework_scores: [f32; 3]) -> Vec<f32> {
        let mut features = vec![0.0; 13];
        // Set language one-hot
        if language_idx < 8 {
            features[language_idx] = 1.0;
        }
        // Set framework scores [8..11]
        features[8] = framework_scores[0];
        features[9] = framework_scores[1];
        features[10] = framework_scores[2];
        // Symbol hits and path tokens
        features[11] = 0.5;
        features[12] = 0.3;
        features
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
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_apply_multi_lora_preserves_base_output() {
        let adapter = create_test_adapter("adapter-preserve", 1);
        let adapters = vec![&adapter];
        let gates = vec![32767u16]; // full weight

        let input = vec![1.0, 1.0]; // non-cancelling input to ensure delta is produced
        let base_output = vec![1.0, -2.0];
        let base_clone = base_output.clone();

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        assert_eq!(
            base_output, base_clone,
            "base output must remain immutable during routing"
        );
        assert_ne!(
            result, base_output,
            "LoRA routing should produce an adapted output without mutating the base buffer"
        );
    }

    #[test]
    fn test_apply_lora_transform() {
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        let input = vec![1.0, 2.0];

        let result = apply_lora_transform(&input, &lora_a, &lora_b, 16.0).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_compute_adapter_score_basic() {
        let adapter = create_test_adapter("test", 4);
        let input_features = vec![1.0, 2.0, 3.0, 4.0]; // Short features - base score only

        let score = compute_adapter_score(&adapter, &input_features, "q_proj");
        assert!(score > 0.0);

        let score_invalid = compute_adapter_score(&adapter, &input_features, "invalid_module");
        assert_eq!(score_invalid, 0.0);
    }

    #[test]
    fn test_compute_adapter_score_with_language_match() {
        // Adapter specialized for Rust (index 0)
        let rust_adapter = create_adapter_with_metadata(
            "rust-adapter",
            4,
            vec![0], // Rust
            None,
            Some("persistent"),
        );

        // Adapter specialized for Python (index 1)
        let python_adapter = create_adapter_with_metadata(
            "python-adapter",
            4,
            vec![1], // Python
            None,
            Some("persistent"),
        );

        // Feature vector with Rust detected (index 0 = 1.0)
        let rust_features = create_feature_vector(0, [0.0, 0.0, 0.0]);

        let rust_score = compute_adapter_score(&rust_adapter, &rust_features, "q_proj");
        let python_score = compute_adapter_score(&python_adapter, &rust_features, "q_proj");

        // Rust adapter should score higher for Rust code
        assert!(
            rust_score > python_score,
            "Rust adapter ({}) should score higher than Python adapter ({}) for Rust code",
            rust_score,
            python_score
        );
    }

    #[test]
    fn test_compute_adapter_score_tier_boost() {
        let persistent =
            create_adapter_with_metadata("persistent", 4, vec![], None, Some("persistent"));
        let ephemeral =
            create_adapter_with_metadata("ephemeral", 4, vec![], None, Some("ephemeral"));
        let experimental =
            create_adapter_with_metadata("experimental", 4, vec![], None, Some("experimental"));

        let features = create_feature_vector(7, [0.0, 0.0, 0.0]); // "Other" language

        let persistent_score = compute_adapter_score(&persistent, &features, "q_proj");
        let ephemeral_score = compute_adapter_score(&ephemeral, &features, "q_proj");
        let experimental_score = compute_adapter_score(&experimental, &features, "q_proj");

        // Tier ordering: persistent > ephemeral > experimental
        assert!(
            persistent_score > ephemeral_score,
            "Persistent ({}) > Ephemeral ({})",
            persistent_score,
            ephemeral_score
        );
        assert!(
            ephemeral_score > experimental_score,
            "Ephemeral ({}) > Experimental ({})",
            ephemeral_score,
            experimental_score
        );
    }

    #[test]
    fn test_compute_adapter_score_framework_boost() {
        let framework_adapter = create_adapter_with_metadata(
            "django",
            4,
            vec![1], // Python
            Some("django"),
            None,
        );

        // Features with high framework relevance
        let high_framework = create_feature_vector(1, [0.8, 0.5, 0.3]);
        // Features with low framework relevance
        let low_framework = create_feature_vector(1, [0.0, 0.0, 0.0]);

        let fw_high = compute_adapter_score(&framework_adapter, &high_framework, "q_proj");
        let fw_low = compute_adapter_score(&framework_adapter, &low_framework, "q_proj");

        // Framework adapter should score higher with framework-relevant features
        assert!(
            fw_high > fw_low,
            "Framework adapter should score higher with framework features: {} > {}",
            fw_high,
            fw_low
        );
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
        assert_eq!(top_k[0].0, 1); // adapter2 (score 0.7)
        assert_eq!(top_k[1].0, 2); // adapter3 (score 0.5)
    }

    #[test]
    fn test_select_top_k_adapters_tie_breaks_by_index() {
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 4);
        let adapter3 = create_test_adapter("adapter3", 6);

        let adapters = vec![&adapter1, &adapter2, &adapter3];
        let scores = vec![0.5, 0.5, 0.4];

        let top_k = select_top_k_adapters(&adapters, &scores, 2);

        assert_eq!(top_k.len(), 2);
        // Tied scores should prefer lower indices.
        assert_eq!(top_k[0].0, 0);
        assert_eq!(top_k[1].0, 1);
    }

    #[test]
    fn test_q15_encode_decode_precision() {
        let values = [0.0_f32, 0.5, 0.99, 0.123456];
        for v in values {
            let encoded = (v * Q15_GATE_DENOMINATOR).round() as u16;
            let decoded = encoded as f32 / Q15_GATE_DENOMINATOR;
            assert!(
                (v - decoded).abs() < 1e-4,
                "Precision loss for {}: encoded={}, decoded={}",
                v,
                encoded,
                decoded
            );
        }
    }

    #[test]
    fn test_q15_gate_normalization() {
        // Gates should approximately sum to 1.0 after dequantization
        let gates_q15: Vec<u16> = vec![16384, 8192, 8191]; // ~0.5, ~0.25, ~0.25
        let sum: f32 = gates_q15
            .iter()
            .map(|g| *g as f32 / Q15_GATE_DENOMINATOR)
            .sum();
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Gate sum should be ~1.0, got {}",
            sum
        );
    }

    #[test]
    fn test_gate_normalization_before_application() {
        // Test that gates are normalized BEFORE applying to LoRA outputs,
        // not normalizing the final result (which would incorrectly scale base_output)
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 2);

        let adapters = vec![&adapter1, &adapter2];

        // Gates that don't sum to 1.0 (e.g., 0.3 + 0.4 = 0.7)
        let gates_unnormalized = vec![
            (0.3 * Q15_GATE_DENOMINATOR) as u16,
            (0.4 * Q15_GATE_DENOMINATOR) as u16,
        ];

        // Gates that sum to 1.0 after normalization (0.3/0.7 + 0.4/0.7 = 1.0)
        let gates_normalized = vec![
            ((0.3 / 0.7) * Q15_GATE_DENOMINATOR) as u16,
            ((0.4 / 0.7) * Q15_GATE_DENOMINATOR) as u16,
        ];

        let input = vec![1.0, 2.0];
        let base_output = vec![10.0, 20.0]; // Non-zero base to verify it's not being scaled

        let result_unnorm = apply_multi_lora(
            &adapters,
            &gates_unnormalized,
            "q_proj",
            &input,
            &base_output,
        )
        .unwrap();
        let result_norm =
            apply_multi_lora(&adapters, &gates_normalized, "q_proj", &input, &base_output).unwrap();

        // Both should produce similar results since gates are normalized before application
        for (i, (&unnorm, &norm)) in result_unnorm.iter().zip(result_norm.iter()).enumerate() {
            assert!(
                (unnorm - norm).abs() < 0.1,
                "Results should be similar at index {}: unnorm={}, norm={}",
                i,
                unnorm,
                norm
            );
        }
    }

    #[test]
    fn test_base_output_not_scaled_by_gate_sum() {
        // Critical test: base_output should not be divided by gate sum
        // This was the bug in the original code
        let adapter = create_test_adapter("adapter1", 2);
        let adapters = vec![&adapter];

        // Gate that is less than 1.0 (0.5)
        let gates = vec![(0.5 * Q15_GATE_DENOMINATOR) as u16];

        let input = vec![0.0, 0.0]; // Zero input means LoRA contribution is zero
        let base_output = vec![10.0, 20.0];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        // With zero input, LoRA contribution is zero, so result should equal base_output
        // (with the old buggy code, this would be base_output / 0.5 = [20.0, 40.0])
        for (i, (&res, &base)) in result.iter().zip(base_output.iter()).enumerate() {
            assert!(
                (res - base).abs() < 0.01,
                "Base output should not be scaled at index {}: result={}, expected={}",
                i,
                res,
                base
            );
        }
    }
}
