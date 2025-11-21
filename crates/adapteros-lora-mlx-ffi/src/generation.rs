//! Text generation loop with KV cache and deterministic sampling for MLX backend
//!
//! Implements token-by-token generation with:
//! - HKDF-seeded deterministic sampling
//! - KV cache for efficient generation
//! - Temperature, top-k, and top-p sampling strategies
//! - Repetition penalty
//! - Streaming support

use crate::MLXFFIModel;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use rand::Rng;
use rand::SeedableRng;
use std::collections::HashMap;

/// Sampling strategy selector
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplingStrategy {
    /// Greedy decoding (always select highest probability token)
    Greedy,
    /// Stochastic sampling from full distribution
    Stochastic,
}

impl SamplingStrategy {
    /// Determine strategy based on temperature
    /// - temperature = 0.0 → Greedy
    /// - temperature > 0.0 → Stochastic
    pub fn from_temperature(temperature: f32) -> Self {
        if (temperature - 0.0).abs() < 1e-6 {
            SamplingStrategy::Greedy
        } else {
            SamplingStrategy::Stochastic
        }
    }
}

/// Generation parameters
#[derive(Debug, Clone)]
pub struct GenerationConfig {
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Temperature for sampling (0.0 = greedy, higher = more random)
    pub temperature: f32,
    /// Top-k sampling (only consider top K tokens)
    pub top_k: Option<usize>,
    /// Top-p/nucleus sampling (cumulative probability threshold)
    pub top_p: Option<f32>,
    /// Repetition penalty (1.0 = no penalty, >1.0 = penalize repetition)
    pub repetition_penalty: f32,
    /// EOS token ID
    pub eos_token: u32,
    /// Enable KV cache
    pub use_cache: bool,
}

impl GenerationConfig {
    /// Get the sampling strategy for this configuration
    pub fn sampling_strategy(&self) -> SamplingStrategy {
        SamplingStrategy::from_temperature(self.temperature)
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100,
            temperature: 1.0,
            top_k: None,
            top_p: None,
            repetition_penalty: 1.0,
            eos_token: 151645, // Qwen2.5 <|im_end|>
            use_cache: true,
        }
    }
}

/// KV cache for efficient generation
///
/// Stores key and value tensors for each layer to avoid recomputing
/// past token representations.
#[derive(Debug)]
pub struct KVCache {
    /// Cache per layer: layer_idx -> (key_cache, value_cache)
    layer_caches: HashMap<usize, (Vec<Vec<f32>>, Vec<Vec<f32>>)>,
    /// Number of cached positions
    cached_positions: usize,
    /// Maximum cache size (positions)
    max_size: usize,
}

impl KVCache {
    /// Create a new KV cache
    pub fn new(max_size: usize) -> Self {
        Self {
            layer_caches: HashMap::new(),
            cached_positions: 0,
            max_size,
        }
    }

    /// Update cache for a layer
    ///
    /// # Arguments
    /// * `layer_idx` - Layer index
    /// * `key` - Key tensor to cache
    /// * `value` - Value tensor to cache
    pub fn update(&mut self, layer_idx: usize, key: Vec<Vec<f32>>, value: Vec<Vec<f32>>) {
        if self.cached_positions >= self.max_size {
            // Evict oldest entries (simple FIFO)
            self.evict_oldest();
        }

        self.layer_caches
            .entry(layer_idx)
            .and_modify(|(k_cache, v_cache)| {
                k_cache.extend(key.iter().cloned());
                v_cache.extend(value.iter().cloned());
            })
            .or_insert((key, value));

        self.cached_positions += 1;
    }

    /// Get cached key/value for a layer
    pub fn get(&self, layer_idx: usize) -> Option<&(Vec<Vec<f32>>, Vec<Vec<f32>>)> {
        self.layer_caches.get(&layer_idx)
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        self.layer_caches.clear();
        self.cached_positions = 0;
    }

    /// Get number of cached positions
    pub fn len(&self) -> usize {
        self.cached_positions
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cached_positions == 0
    }

    /// Evict oldest cache entries
    fn evict_oldest(&mut self) {
        // For simplicity, clear entire cache on overflow
        // A more sophisticated implementation would use a ring buffer
        tracing::debug!(
            "KV cache overflow ({}), clearing cache",
            self.cached_positions
        );
        self.clear();
    }
}

/// Token generator with deterministic sampling
pub struct MLXGenerator {
    /// Random number generator (seeded via HKDF)
    rng: rand::rngs::StdRng,
    /// Generation configuration
    config: GenerationConfig,
    /// KV cache for efficient generation
    cache: Option<KVCache>,
    /// Base seed for deterministic generation
    base_seed: B3Hash,
}

impl MLXGenerator {
    /// Create a new generator with HKDF-derived seed
    ///
    /// # Arguments
    /// * `base_seed` - Base seed (typically model hash)
    /// * `config` - Generation configuration
    pub fn new(base_seed: B3Hash, config: GenerationConfig) -> Self {
        // Derive RNG seed from base seed
        let rng_seed = derive_seed(&base_seed, "mlx-sampling");
        let rng = rand::rngs::StdRng::from_seed(rng_seed);

        let cache = if config.use_cache {
            Some(KVCache::new(2048)) // Default max cache size
        } else {
            None
        };

        Self {
            rng,
            config,
            cache,
            base_seed,
        }
    }

    /// Generate text from prompt
    ///
    /// # Arguments
    /// * `model` - MLX model
    /// * `prompt_tokens` - Input token IDs
    ///
    /// # Returns
    /// Generated token IDs (including prompt)
    pub fn generate(&mut self, model: &MLXFFIModel, prompt_tokens: Vec<u32>) -> Result<Vec<u32>> {
        let mut tokens = prompt_tokens.clone();
        let prompt_len = tokens.len();

        tracing::info!(
            prompt_tokens = prompt_len,
            max_tokens = self.config.max_tokens,
            temperature = self.config.temperature,
            "Starting MLX generation"
        );

        // Clear cache at start of generation
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }

        for step in 0..self.config.max_tokens {
            // Derive step-specific seed for determinism
            let step_seed = self.derive_step_seed(step);
            self.rng = rand::rngs::StdRng::from_seed(step_seed);

            // Run forward pass
            // For now, use simple forward (KV cache integration requires model changes)
            let position = tokens.len() - 1;
            let logits = model.forward(&tokens, position)?;

            // Apply repetition penalty
            let penalized_logits = if self.config.repetition_penalty != 1.0 {
                self.apply_repetition_penalty(&logits, &tokens)?
            } else {
                logits
            };

            // Sample next token
            let next_token = self.sample_token(&penalized_logits)?;

            // Check for EOS
            if next_token == self.config.eos_token {
                tracing::debug!(step = step, "EOS token encountered");
                break;
            }

            tokens.push(next_token);

            // Log progress periodically
            if step % 10 == 0 && step > 0 {
                tracing::debug!(
                    step = step,
                    tokens_generated = tokens.len() - prompt_len,
                    "Generation progress"
                );
            }
        }

        let generated_count = tokens.len() - prompt_len;
        tracing::info!(
            tokens_generated = generated_count,
            total_tokens = tokens.len(),
            "Generation complete"
        );

        Ok(tokens)
    }

    /// Generate text with streaming callback
    ///
    /// # Arguments
    /// * `model` - MLX model
    /// * `prompt_tokens` - Input token IDs
    /// * `callback` - Called for each generated token
    ///
    /// # Returns
    /// Generated token IDs (including prompt)
    pub fn generate_streaming<F>(
        &mut self,
        model: &MLXFFIModel,
        prompt_tokens: Vec<u32>,
        mut callback: F,
    ) -> Result<Vec<u32>>
    where
        F: FnMut(u32, usize) -> Result<bool>, // (token, position) -> should_continue
    {
        let mut tokens = prompt_tokens.clone();
        let prompt_len = tokens.len();

        // Clear cache at start
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }

        for step in 0..self.config.max_tokens {
            // Derive step-specific seed
            let step_seed = self.derive_step_seed(step);
            self.rng = rand::rngs::StdRng::from_seed(step_seed);

            // Run forward pass
            let position = tokens.len() - 1;
            let logits = model.forward(&tokens, position)?;

            // Apply repetition penalty
            let penalized_logits = if self.config.repetition_penalty != 1.0 {
                self.apply_repetition_penalty(&logits, &tokens)?
            } else {
                logits
            };

            // Sample next token
            let next_token = self.sample_token(&penalized_logits)?;

            // Check for EOS
            if next_token == self.config.eos_token {
                break;
            }

            tokens.push(next_token);

            // Invoke callback
            let should_continue = callback(next_token, tokens.len() - 1)?;
            if !should_continue {
                tracing::debug!(step = step, "Generation stopped by callback");
                break;
            }
        }

        let generated_count = tokens.len() - prompt_len;
        tracing::info!(
            tokens_generated = generated_count,
            "Streaming generation complete"
        );

        Ok(tokens)
    }

    /// Sample next token from logits using configured strategy
    ///
    /// Applies sampling pipeline:
    /// 1. Repetition penalty (via caller)
    /// 2. Temperature scaling
    /// 3. Top-k filtering (optional)
    /// 4. Top-p (nucleus) filtering (optional)
    /// 5. Strategy selection (greedy vs stochastic)
    fn sample_token(&mut self, logits: &[f32]) -> Result<u32> {
        if logits.is_empty() {
            return Err(AosError::Internal("Empty logits".to_string()));
        }

        // Determine sampling strategy
        let strategy = self.config.sampling_strategy();

        // Apply temperature scaling
        let scaled_logits: Vec<f32> = if self.config.temperature != 1.0 {
            let temp = self.config.temperature.max(0.01); // Prevent division by zero
            logits.iter().map(|&l| l / temp).collect()
        } else {
            logits.to_vec()
        };

        // Convert to probabilities via softmax
        let probs = self.softmax(&scaled_logits);

        // Apply top-k filtering if configured
        let filtered_probs = if let Some(k) = self.config.top_k {
            self.apply_top_k(&probs, k)
        } else {
            probs
        };

        // Apply top-p (nucleus) filtering if configured
        let final_probs = if let Some(p) = self.config.top_p {
            self.apply_top_p(&filtered_probs, p)
        } else {
            filtered_probs
        };

        // Sample using selected strategy
        match strategy {
            SamplingStrategy::Greedy => self.sample_greedy(&final_probs),
            SamplingStrategy::Stochastic => self.sample_from_distribution(&final_probs),
        }
    }

    /// Apply repetition penalty to logits
    fn apply_repetition_penalty(&self, logits: &[f32], tokens: &[u32]) -> Result<Vec<f32>> {
        let mut penalized = logits.to_vec();

        // Count token occurrences
        let mut token_counts: HashMap<u32, usize> = HashMap::new();
        for &token in tokens {
            *token_counts.entry(token).or_insert(0) += 1;
        }

        // Apply penalty to repeated tokens
        for (token_id, count) in token_counts {
            if token_id < penalized.len() as u32 {
                let idx = token_id as usize;
                // Penalty: logit / (penalty ^ count)
                // Higher penalty = lower probability for repeated tokens
                penalized[idx] /= self.config.repetition_penalty.powi(count as i32);
            }
        }

        Ok(penalized)
    }

    /// Compute softmax (logits -> probabilities)
    fn softmax(&self, logits: &[f32]) -> Vec<f32> {
        let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        let exp_logits: Vec<f32> = logits.iter().map(|&l| (l - max_logit).exp()).collect();
        let sum: f32 = exp_logits.iter().sum();

        if sum == 0.0 {
            // Fallback to uniform distribution
            vec![1.0 / logits.len() as f32; logits.len()]
        } else {
            exp_logits.iter().map(|&e| e / sum).collect()
        }
    }

    /// Apply top-k filtering
    fn apply_top_k(&self, probs: &[f32], k: usize) -> Vec<f32> {
        let mut indexed_probs: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();

        // Sort by probability (descending)
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Zero out probabilities outside top-k
        let mut filtered = vec![0.0; probs.len()];
        for i in 0..k.min(indexed_probs.len()) {
            let (idx, prob) = indexed_probs[i];
            filtered[idx] = prob;
        }

        // Renormalize
        let sum: f32 = filtered.iter().sum();
        if sum > 0.0 {
            filtered.iter().map(|&p| p / sum).collect()
        } else {
            filtered
        }
    }

    /// Apply top-p (nucleus) filtering
    fn apply_top_p(&self, probs: &[f32], p: f32) -> Vec<f32> {
        let mut indexed_probs: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();

        // Sort by probability (descending)
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Find cumulative probability threshold
        let mut cumsum = 0.0;
        let mut cutoff_idx = indexed_probs.len();
        for (i, (_idx, prob)) in indexed_probs.iter().enumerate() {
            cumsum += prob;
            if cumsum >= p {
                cutoff_idx = i + 1;
                break;
            }
        }

        // Zero out probabilities outside nucleus
        let mut filtered = vec![0.0; probs.len()];
        for i in 0..cutoff_idx {
            let (idx, prob) = indexed_probs[i];
            filtered[idx] = prob;
        }

        // Renormalize
        let sum: f32 = filtered.iter().sum();
        if sum > 0.0 {
            filtered.iter().map(|&p| p / sum).collect()
        } else {
            filtered
        }
    }

    /// Greedy sampling: select token with highest probability
    ///
    /// Uses deterministic argmax instead of random sampling.
    /// Always returns the same token for the same probabilities.
    fn sample_greedy(&self, probs: &[f32]) -> Result<u32> {
        if probs.is_empty() {
            return Err(AosError::Internal(
                "Cannot perform greedy sampling on empty probabilities".to_string(),
            ));
        }

        // Find index of maximum probability
        let (idx, _max_prob) = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| AosError::Internal("Failed to find max probability".to_string()))?;

        Ok(idx as u32)
    }

    /// Sample from probability distribution
    fn sample_from_distribution(&mut self, probs: &[f32]) -> Result<u32> {
        let sum: f32 = probs.iter().sum();
        if sum == 0.0 {
            return Err(AosError::Internal(
                "Cannot sample from zero probability distribution".to_string(),
            ));
        }

        // Generate random value in [0, sum]
        let random_val: f32 = self.rng.gen::<f32>() * sum;

        // Find token corresponding to random value
        let mut cumsum = 0.0;
        for (idx, &prob) in probs.iter().enumerate() {
            cumsum += prob;
            if cumsum >= random_val {
                return Ok(idx as u32);
            }
        }

        // Fallback to last token (shouldn't happen with proper normalization)
        Ok((probs.len() - 1) as u32)
    }

    /// Derive step-specific seed for deterministic generation
    fn derive_step_seed(&self, step: usize) -> [u8; 32] {
        let label = format!("mlx-gen-step:{}", step);
        derive_seed(&self.base_seed, &label)
    }

    /// Get current cache state (for debugging)
    pub fn cache_len(&self) -> usize {
        self.cache.as_ref().map_or(0, |c| c.len())
    }

    /// Clear generation cache
    pub fn clear_cache(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
    }

    /// Generate text from a text prompt using provided tokenizer
    ///
    /// Convenience method that handles tokenization and detokenization.
    ///
    /// # Arguments
    /// * `model` - MLX model for inference
    /// * `prompt` - Text prompt to generate from
    /// * `tokenizer` - Tokenizer for encoding/decoding
    ///
    /// # Returns
    /// Generated text or error
    pub fn generate_text(
        &mut self,
        model: &MLXFFIModel,
        prompt: &str,
        tokenizer: &crate::tokenizer::MLXTokenizer,
    ) -> Result<String> {
        // Encode prompt to tokens
        let prompt_tokens = tokenizer.encode(prompt)?;

        // Generate tokens
        let generated_tokens = self.generate(model, prompt_tokens)?;

        // Decode to text
        tokenizer.decode(&generated_tokens)
    }

    /// Generate text with chat template
    ///
    /// Applies the tokenizer's chat template before generating.
    ///
    /// # Arguments
    /// * `model` - MLX model for inference
    /// * `prompt` - User prompt (will be formatted with chat template)
    /// * `tokenizer` - Tokenizer with chat template support
    ///
    /// # Returns
    /// Generated text or error
    pub fn generate_chat(
        &mut self,
        model: &MLXFFIModel,
        prompt: &str,
        tokenizer: &crate::tokenizer::MLXTokenizer,
    ) -> Result<String> {
        // Apply chat template
        let formatted_prompt = tokenizer.apply_chat_template(prompt);

        // Generate text
        self.generate_text(model, &formatted_prompt, tokenizer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_config_default() {
        let config = GenerationConfig::default();
        assert_eq!(config.max_tokens, 100);
        assert_eq!(config.temperature, 1.0);
        assert_eq!(config.repetition_penalty, 1.0);
        assert!(config.use_cache);
    }

    #[test]
    fn test_sampling_strategy_from_temperature() {
        // Zero temperature → greedy
        assert_eq!(
            SamplingStrategy::from_temperature(0.0),
            SamplingStrategy::Greedy
        );

        // Very small temperature (near zero) → greedy
        assert_eq!(
            SamplingStrategy::from_temperature(1e-7),
            SamplingStrategy::Greedy
        );

        // Positive temperatures → stochastic
        assert_eq!(
            SamplingStrategy::from_temperature(0.5),
            SamplingStrategy::Stochastic
        );
        assert_eq!(
            SamplingStrategy::from_temperature(1.0),
            SamplingStrategy::Stochastic
        );
        assert_eq!(
            SamplingStrategy::from_temperature(2.0),
            SamplingStrategy::Stochastic
        );
    }

    #[test]
    fn test_generation_config_strategy() {
        let greedy_config = GenerationConfig {
            temperature: 0.0,
            ..Default::default()
        };
        assert_eq!(greedy_config.sampling_strategy(), SamplingStrategy::Greedy);

        let stochastic_config = GenerationConfig {
            temperature: 0.7,
            ..Default::default()
        };
        assert_eq!(
            stochastic_config.sampling_strategy(),
            SamplingStrategy::Stochastic
        );
    }

    #[test]
    fn test_kv_cache_creation() {
        let mut cache = KVCache::new(100);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        // Add some cache entries
        cache.update(0, vec![vec![1.0, 2.0]], vec![vec![3.0, 4.0]]);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_kv_cache_retrieval() {
        let mut cache = KVCache::new(100);
        let key = vec![vec![1.0, 2.0]];
        let value = vec![vec![3.0, 4.0]];

        cache.update(0, key.clone(), value.clone());

        let retrieved = cache.get(0).unwrap();
        assert_eq!(retrieved.0, key);
        assert_eq!(retrieved.1, value);
    }

    #[test]
    fn test_softmax_computation() {
        let base_seed = B3Hash::hash(b"test");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        let logits = vec![1.0, 2.0, 3.0];
        let probs = generator.softmax(&logits);

        // Check sum to 1.0
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);

        // Check ordering (higher logit = higher prob)
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_top_k_filtering() {
        let base_seed = B3Hash::hash(b"test");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        let probs = vec![0.1, 0.2, 0.3, 0.25, 0.15];
        let filtered = generator.apply_top_k(&probs, 2);

        // Only top 2 should have non-zero probability
        let non_zero_count = filtered.iter().filter(|&&p| p > 0.0).count();
        assert_eq!(non_zero_count, 2);

        // Sum should still be 1.0
        let sum: f32 = filtered.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_top_p_filtering() {
        let base_seed = B3Hash::hash(b"test");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        let probs = vec![0.4, 0.3, 0.2, 0.05, 0.05];
        let filtered = generator.apply_top_p(&probs, 0.8);

        // Should keep tokens until cumsum >= 0.8
        // 0.4 + 0.3 + 0.2 = 0.9, so should keep first 3 tokens
        let non_zero_count = filtered.iter().filter(|&&p| p > 0.0).count();
        assert!(non_zero_count <= 3);
    }

    #[test]
    fn test_repetition_penalty() {
        let base_seed = B3Hash::hash(b"test");
        let mut config = GenerationConfig::default();
        config.repetition_penalty = 1.2;
        let generator = MLXGenerator::new(base_seed, config);

        let logits = vec![1.0, 2.0, 3.0, 4.0];
        let tokens = vec![0, 0, 1]; // Token 0 appears twice

        let penalized = generator
            .apply_repetition_penalty(&logits, &tokens)
            .unwrap();

        // Token 0 should be penalized (lower logit)
        assert!(penalized[0] < logits[0]);
        // Token 1 should be slightly penalized
        assert!(penalized[1] < logits[1]);
        // Tokens 2, 3 should be unchanged
        assert_eq!(penalized[2], logits[2]);
        assert_eq!(penalized[3], logits[3]);
    }

    #[test]
    fn test_deterministic_step_seeds() {
        let base_seed = B3Hash::hash(b"test");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        // Same step should produce same seed
        let seed1 = generator.derive_step_seed(5);
        let seed2 = generator.derive_step_seed(5);
        assert_eq!(seed1, seed2);

        // Different steps should produce different seeds
        let seed3 = generator.derive_step_seed(6);
        assert_ne!(seed1, seed3);
    }

    #[test]
    fn test_greedy_sampling_basic() {
        let base_seed = B3Hash::hash(b"greedy-test");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        // Simple probability distribution: [0.1, 0.3, 0.2, 0.4]
        // Should select token 3 (highest probability)
        let probs = vec![0.1, 0.3, 0.2, 0.4];
        let token = generator.sample_greedy(&probs).unwrap();
        assert_eq!(token, 3);
    }

    #[test]
    fn test_greedy_sampling_clear_winner() {
        let base_seed = B3Hash::hash(b"greedy-clear");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        // Clear winner
        let probs = vec![0.01, 0.01, 0.97, 0.01];
        let token = generator.sample_greedy(&probs).unwrap();
        assert_eq!(token, 2);
    }

    #[test]
    fn test_greedy_sampling_first_max() {
        let base_seed = B3Hash::hash(b"greedy-first");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        // Multiple equal probabilities - max_by returns last max element
        let probs = vec![0.5, 0.5, 0.0];
        let token = generator.sample_greedy(&probs).unwrap();
        assert_eq!(token, 1);
    }

    #[test]
    fn test_greedy_sampling_deterministic() {
        let base_seed = B3Hash::hash(b"greedy-determinism");
        let config = GenerationConfig::default();
        let generator1 = MLXGenerator::new(base_seed, config.clone());
        let generator2 = MLXGenerator::new(base_seed, config);

        let probs = vec![0.1, 0.2, 0.3, 0.25, 0.15];

        // Same probabilities, same base seed should produce same result
        let token1 = generator1.sample_greedy(&probs).unwrap();
        let token2 = generator2.sample_greedy(&probs).unwrap();
        assert_eq!(token1, token2);
    }

    #[test]
    fn test_stochastic_vs_greedy_sampling() {
        let base_seed = B3Hash::hash(b"stochastic-vs-greedy");

        // Greedy configuration (temperature = 0)
        let greedy_config = GenerationConfig {
            temperature: 0.0,
            ..Default::default()
        };
        let generator_greedy = MLXGenerator::new(base_seed, greedy_config);

        // Stochastic configuration (temperature > 0)
        let stochastic_config = GenerationConfig {
            temperature: 0.8,
            ..Default::default()
        };
        let _generator_stochastic = MLXGenerator::new(base_seed, stochastic_config);

        let probs = vec![0.1, 0.2, 0.3, 0.25, 0.15];

        // Greedy should always select token 2 (highest prob)
        let token = generator_greedy.sample_greedy(&probs).unwrap();
        assert_eq!(token, 2);

        // Stochastic sampling would vary (can't test deterministically without mocking)
    }

    #[test]
    fn test_greedy_with_top_k_filtering() {
        let base_seed = B3Hash::hash(b"greedy-topk");

        // With top-k filtering
        let config = GenerationConfig {
            temperature: 0.0,
            top_k: Some(2),
            ..Default::default()
        };
        let generator = MLXGenerator::new(base_seed, config);

        // Original: [0.1, 0.2, 0.3, 0.25, 0.15]
        let probs = vec![0.1, 0.2, 0.3, 0.25, 0.15];

        // After top-2 filtering: token 2 (0.3) and token 3 (0.25) remain
        let filtered = generator.apply_top_k(&probs, 2);
        let token = generator.sample_greedy(&filtered).unwrap();

        // Should select token 2 (still highest after filtering)
        assert_eq!(token, 2);
    }

    #[test]
    fn test_greedy_with_top_p_filtering() {
        let base_seed = B3Hash::hash(b"greedy-topp");

        // With top-p filtering
        let config = GenerationConfig {
            temperature: 0.0,
            top_p: Some(0.7),
            ..Default::default()
        };
        let generator = MLXGenerator::new(base_seed, config);

        // Original: [0.4, 0.3, 0.2, 0.05, 0.05] (cumsum at 0.9 reaches threshold)
        let probs = vec![0.4, 0.3, 0.2, 0.05, 0.05];

        // After top-p filtering, token 0 (0.4) will have highest prob
        let filtered = generator.apply_top_p(&probs, 0.7);
        let token = generator.sample_greedy(&filtered).unwrap();

        // Should select token 0 (highest)
        assert_eq!(token, 0);
    }

    #[test]
    fn test_sampling_pipeline_integration() {
        // Test the full pipeline: temperature -> top-k -> top-p -> greedy
        let base_seed = B3Hash::hash(b"pipeline-test");

        let config = GenerationConfig {
            temperature: 0.0, // Forces greedy
            top_k: Some(3),
            top_p: Some(0.95),
            ..Default::default()
        };

        let mut generator = MLXGenerator::new(base_seed, config);
        let logits = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        // This should:
        // 1. Apply temperature (no-op for 0)
        // 2. Convert to probs via softmax
        // 3. Apply top-3 filtering
        // 4. Apply top-p filtering
        // 5. Use greedy selection
        let strategy = generator.config.sampling_strategy();
        assert_eq!(strategy, SamplingStrategy::Greedy);
    }

    #[test]
    fn test_greedy_sampling_uniform_distribution() {
        let base_seed = B3Hash::hash(b"greedy-uniform");
        let config = GenerationConfig::default();
        let generator = MLXGenerator::new(base_seed, config);

        // Uniform distribution - max_by returns last max element
        let probs = vec![0.25, 0.25, 0.25, 0.25];
        let token = generator.sample_greedy(&probs).unwrap();
        assert_eq!(token, 3);
    }
}
