//! Token generation loop with sampling strategies

use adapteros_core::{AosError, Result};
use rand::Rng;
use rand::SeedableRng;

/// Token generator with configurable sampling
pub struct Generator {
    rng: rand::rngs::StdRng,
    temperature: f32,
    top_k: Option<usize>,
    top_p: Option<f32>,
}

impl Generator {
    /// Create a new generator with seed
    pub fn new(seed: [u8; 32]) -> Self {
        Self {
            rng: rand::rngs::StdRng::from_seed(seed),
            temperature: 1.0,
            top_k: None,
            top_p: None,
        }
    }

    /// Create a new generator with deterministic HKDF-derived seed
    ///
    /// # Arguments
    /// * `seed_global` - Global seed bytes
    /// * `context` - Context string for domain separation
    ///
    /// # Returns
    /// Generator with deterministically derived seed
    pub fn new_deterministic(seed_global: &[u8], context: &str) -> Self {
        use hkdf::Hkdf;
        use sha2::Sha256;

        let hk = Hkdf::<Sha256>::new(None, seed_global);
        let mut seed = [0u8; 32];
        hk.expand(context.as_bytes(), &mut seed)
            .expect("HKDF expand failed");

        Self {
            rng: rand::rngs::StdRng::from_seed(seed),
            temperature: 1.0,
            top_k: None,
            top_p: None,
        }
    }

    /// Set temperature for sampling (default: 1.0)
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.max(0.01); // Prevent division by zero
        self
    }

    /// Set top-k sampling (only consider top K tokens)
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = Some(k);
        self
    }

    /// Set nucleus/top-p sampling (cumulative probability threshold)
    pub fn with_top_p(mut self, p: f32) -> Self {
        self.top_p = Some(p.clamp(0.0, 1.0));
        self
    }

    /// Generate tokens autoregressively
    ///
    /// # Arguments
    /// * `backend` - Inference backend
    /// * `router` - K-sparse router
    /// * `initial_tokens` - Starting token sequence
    /// * `max_tokens` - Maximum tokens to generate
    /// * `vocab_size` - Vocabulary size for buffer allocation
    /// * `eos_token` - End-of-sequence token ID
    ///
    /// # Returns
    /// Complete token sequence including initial tokens
    pub fn generate_tokens(
        &mut self,
        backend: &mut dyn adapteros_lora_kernel_api::FusedKernels,
        router: &mut adapteros_lora_router::Router,
        initial_tokens: Vec<u32>,
        max_tokens: usize,
        vocab_size: usize,
        eos_token: u32,
    ) -> Result<Vec<u32>> {
        let initial_len = initial_tokens.len();
        let mut tokens = initial_tokens;
        let mut io = adapteros_lora_kernel_api::IoBuffers::new(vocab_size);

        for step in 0..max_tokens {
            // Set input to last token
            let last_token = tokens.last().ok_or_else(|| {
                AosError::Internal("Token sequence cannot be empty during generation".to_string())
            })?;
            io.input_ids = vec![*last_token];
            io.position = tokens.len() - 1;

            // Get router decision
            // For now, use dummy features and uniform priors
            // In a full implementation, features would come from token embeddings
            let num_adapters = 8; // TODO: Get from manifest
            let features = vec![0.0f32; 16]; // Dummy features
            let priors = vec![1.0f32 / num_adapters as f32; num_adapters]; // Uniform priors
            let decision = router.route(&features, &priors);

            // Run inference step
            // Convert Decision to RouterRing
            let ring = adapteros_lora_kernel_api::RouterRing {
                indices: decision.indices.to_vec(),
                gates_q15: decision.gates_q15.to_vec(),
                position: io.position,
            };
            backend.run_step(&ring, &mut io)?;

            // Sample next token
            let next_token = self.next_token(&io.output_logits)?;

            // Check for EOS
            if next_token == eos_token {
                tracing::debug!("EOS token encountered at step {}", step);
                break;
            }

            tokens.push(next_token);
        }

        tracing::info!(
            "Generated {} tokens (initial: {}, new: {})",
            tokens.len(),
            initial_len,
            tokens.len() - initial_len
        );

        Ok(tokens)
    }

    /// Generate next token from logits
    pub fn next_token(&mut self, logits: &[f32]) -> Result<u32> {
        if logits.is_empty() {
            return Err(adapteros_core::AosError::Worker(
                "Empty logits provided".to_string(),
            ));
        }

        // Apply temperature
        let scaled_logits: Vec<f32> = if self.temperature != 1.0 {
            logits.iter().map(|&l| l / self.temperature).collect()
        } else {
            logits.to_vec()
        };

        // Convert logits to probabilities (softmax)
        let probs = self.softmax(&scaled_logits);

        // Apply top-k filtering if configured
        let filtered_probs = if let Some(k) = self.top_k {
            self.apply_top_k(&probs, k)
        } else {
            probs
        };

        // Apply nucleus (top-p) filtering if configured
        let final_probs = if let Some(p) = self.top_p {
            self.apply_top_p(&filtered_probs, p)
        } else {
            filtered_probs
        };

        // Sample from the distribution
        self.sample_from_distribution(&final_probs)
    }

    /// Softmax function to convert logits to probabilities
    fn softmax(&self, logits: &[f32]) -> Vec<f32> {
        // Find max for numerical stability
        let max_logit = logits
            .iter()
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);

        // Compute exp(logit - max)
        let exp_logits: Vec<f32> = logits.iter().map(|&l| (l - max_logit).exp()).collect();

        // Normalize
        let sum: f32 = exp_logits.iter().sum();
        exp_logits.iter().map(|&e| e / sum).collect()
    }

    /// Apply top-k filtering: zero out all but top k probabilities
    fn apply_top_k(&self, probs: &[f32], k: usize) -> Vec<f32> {
        let mut indexed_probs: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();

        // Sort by probability descending
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Keep only top k
        let mut result = vec![0.0; probs.len()];
        for (idx, prob) in indexed_probs.iter().take(k) {
            result[*idx] = *prob;
        }

        // Renormalize
        let sum: f32 = result.iter().sum();
        if sum > 0.0 {
            result.iter().map(|&p| p / sum).collect()
        } else {
            result
        }
    }

    /// Apply nucleus (top-p) sampling: keep tokens with cumulative prob <= p
    fn apply_top_p(&self, probs: &[f32], p: f32) -> Vec<f32> {
        let mut indexed_probs: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();

        // Sort by probability descending
        indexed_probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Find cutoff where cumulative prob exceeds p
        let mut cumsum = 0.0;
        let mut cutoff_idx = indexed_probs.len();
        for (i, (_, prob)) in indexed_probs.iter().enumerate() {
            cumsum += prob;
            if cumsum >= p {
                cutoff_idx = i + 1;
                break;
            }
        }

        // Keep only tokens up to cutoff
        let mut result = vec![0.0; probs.len()];
        for (idx, prob) in indexed_probs.iter().take(cutoff_idx) {
            result[*idx] = *prob;
        }

        // Renormalize
        let sum: f32 = result.iter().sum();
        if sum > 0.0 {
            result.iter().map(|&p| p / sum).collect()
        } else {
            result
        }
    }

    /// Sample token index from probability distribution
    fn sample_from_distribution(&mut self, probs: &[f32]) -> Result<u32> {
        let sum: f32 = probs.iter().sum();
        if sum <= 0.0 {
            // Fallback to argmax if distribution is degenerate
            return Ok(probs
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i as u32)
                .unwrap_or(0));
        }

        // Sample using random number in [0, 1)
        let rng_val: f32 = self.rng.gen();
        let mut cumsum = 0.0;

        for (i, &prob) in probs.iter().enumerate() {
            cumsum += prob;
            if rng_val <= cumsum {
                return Ok(i as u32);
            }
        }

        // Fallback (shouldn't reach here with proper normalization)
        Ok((probs.len() - 1) as u32)
    }

    /// Greedy sampling (always pick highest probability)
    pub fn greedy(&self, logits: &[f32]) -> Result<u32> {
        logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as u32)
            .ok_or_else(|| adapteros_core::AosError::Worker("Empty logits".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greedy_sampling() {
        let generator = Generator::new([0u8; 32]);
        let logits = vec![0.1, 0.5, 0.3, 0.8, 0.2];
        let token = generator
            .greedy(&logits)
            .expect("Test greedy sampling should succeed");
        assert_eq!(token, 3); // Index of max value
    }

    #[test]
    fn test_softmax() {
        let generator = Generator::new([0u8; 32]);
        let logits = vec![1.0, 2.0, 3.0];
        let probs = generator.softmax(&logits);

        // Check sum to 1
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 0.0001);

        // Check ordering preserved
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_deterministic_with_seed() {
        let mut gen1 = Generator::new([42u8; 32]);
        let mut gen2 = Generator::new([42u8; 32]);

        let logits = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        let token1 = gen1
            .next_token(&logits)
            .expect("Test token generation should succeed");
        let token2 = gen2
            .next_token(&logits)
            .expect("Test token generation should succeed");

        assert_eq!(token1, token2); // Same seed = same result
    }
}
