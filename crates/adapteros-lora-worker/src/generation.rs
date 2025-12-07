//! Token generation loop with sampling strategies

use adapteros_core::{AosError, Result};
use adapteros_lora_router::AdapterInfo;
use blake3::Hasher;
use rand::Rng;
use rand::SeedableRng;

/// Token generator with configurable sampling
pub struct Generator {
    rng: rand::rngs::StdRng,
    temperature: f32,
    top_k: Option<usize>,
    top_p: Option<f32>,
    /// Base seed for HKDF derivation (deterministic mode only)
    base_seed: [u8; 32],
    /// Current step counter for re-seeding
    step_counter: usize,
    /// Whether this generator is in deterministic mode
    deterministic_mode: bool,
}

impl Generator {
    /// Create a new generator with seed
    pub fn new(seed: [u8; 32]) -> Self {
        Self {
            rng: rand::rngs::StdRng::from_seed(seed),
            temperature: 1.0,
            top_k: None,
            top_p: None,
            base_seed: seed,
            step_counter: 0,
            deterministic_mode: true,
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
            base_seed: seed,
            step_counter: 0,
            deterministic_mode: true,
        }
    }

    /// Derive a step-specific seed using the canonical sampling rule.
    ///
    /// H("sample" || request_seed || step)
    fn derive_step_seed(&self, step: usize) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(b"sample");
        hasher.update(&self.base_seed);
        hasher.update(&(step as u64).to_le_bytes());
        hasher.finalize().as_bytes().to_owned().try_into().unwrap()
    }

    /// Re-seed the RNG for a specific generation step
    ///
    /// In deterministic mode, this ensures each step produces
    /// reproducible outputs regardless of prior operations.
    pub fn reseed_for_step(&mut self, step: usize) {
        if self.deterministic_mode {
            let step_seed = self.derive_step_seed(step);
            self.rng = rand::rngs::StdRng::from_seed(step_seed);
            self.step_counter = step;
        }
    }

    /// Get the current step counter
    pub fn current_step(&self) -> usize {
        self.step_counter
    }

    /// Check if this generator is in deterministic mode
    pub fn is_deterministic(&self) -> bool {
        self.deterministic_mode
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

    /// Enable deterministic mode for step-level reproducibility
    ///
    /// When enabled, `reseed_for_step()` will derive step-specific seeds
    /// using HKDF, enabling exact replay of generation sequences.
    pub fn with_deterministic(mut self) -> Self {
        self.deterministic_mode = true;
        self
    }

    // =========================================================================
    // Setter methods for per-request sampling params (PRD-02: replay support)
    // =========================================================================

    /// Set temperature for sampling (runtime setter)
    ///
    /// Used to apply per-request sampling parameters during replay.
    pub fn set_temperature(&mut self, temperature: f32) {
        self.temperature = temperature.max(0.01); // Prevent division by zero
    }

    /// Set top-k sampling (runtime setter)
    pub fn set_top_k(&mut self, k: Option<usize>) {
        self.top_k = k;
    }

    /// Set top-p sampling (runtime setter)
    pub fn set_top_p(&mut self, p: Option<f32>) {
        self.top_p = p.map(|v| v.clamp(0.0, 1.0));
    }

    /// Reset generator with a new seed (for replay determinism)
    ///
    /// This replaces the base seed and reseeds the RNG, enabling
    /// exact replay of previous inference runs when the same seed
    /// is used.
    ///
    /// # Arguments
    /// * `seed` - 64-bit seed value (will be expanded to 32 bytes)
    pub fn set_seed(&mut self, seed: u64) {
        // Expand u64 seed to [u8; 32] deterministically
        let mut seed_bytes = [0u8; 32];
        seed_bytes[..8].copy_from_slice(&seed.to_le_bytes());
        // Fill remaining bytes with HKDF expansion for entropy
        use hkdf::Hkdf;
        use sha2::Sha256;
        let hk = Hkdf::<Sha256>::new(None, &seed_bytes[..8]);
        hk.expand(b"replay-seed-expand", &mut seed_bytes)
            .expect("HKDF expand failed");

        self.base_seed = seed_bytes;
        self.rng = rand::rngs::StdRng::from_seed(seed_bytes);
        self.step_counter = 0;
        self.deterministic_mode = true;
    }

    /// Apply sampling parameters from an inference request (PRD-02)
    ///
    /// Updates temperature, top_k, top_p, and seed if provided in the request.
    /// This enables deterministic replay when the same parameters are used.
    pub fn set_seed_bytes(&mut self, seed: [u8; 32]) {
        self.base_seed = seed;
        self.rng = rand::rngs::StdRng::from_seed(seed);
        self.step_counter = 0;
        self.deterministic_mode = true;
    }

    pub fn apply_request_params(
        &mut self,
        temperature: Option<f32>,
        top_k: Option<usize>,
        top_p: Option<f32>,
        seed: Option<u64>,
    ) {
        if let Some(t) = temperature {
            self.set_temperature(t);
        }
        if top_k.is_some() {
            self.set_top_k(top_k);
        }
        if top_p.is_some() {
            self.set_top_p(top_p);
        }
        if let Some(s) = seed {
            self.set_seed(s);
        }
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
            // Re-seed RNG for deterministic step-level reproducibility
            self.reseed_for_step(step);

            // Set input to last token
            let last_token = tokens.last().ok_or_else(|| {
                AosError::Internal("Token sequence cannot be empty during generation".to_string())
            })?;
            io.input_ids = vec![*last_token];
            io.position = tokens.len() - 1;

            // Get router decision
            // For now, use dummy features and uniform priors sized to adapter_count
            let num_adapters = 8; // Default adapter count for dummy routing
            let features = vec![0.0f32; 16]; // Dummy features
            let priors = vec![1.0f32 / num_adapters as f32; num_adapters]; // Uniform priors
                                                                           // Create dummy adapter info for route_with_adapter_info
            let adapter_info: Vec<AdapterInfo> = (0..num_adapters)
                .map(|i| AdapterInfo {
                    id: format!("adapter_{}", i),
                    framework: None,
                    languages: vec![0], // Default language
                    tier: "persistent".to_string(),
                })
                .collect();
            let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);

            // Run inference step
            // Convert Decision to RouterRing
            let mut ring = adapteros_lora_kernel_api::RouterRing::from(&decision);
            ring.position = io.position;
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

    /// Apply temperature scaling and filtering to logits
    ///
    /// Shared logic for temperature, top-k, and top-p filtering.
    /// Used by both `next_token` and `max_prob`.
    fn process_logits(&self, logits: &[f32]) -> Vec<f32> {
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
        if let Some(p) = self.top_p {
            self.apply_top_p(&filtered_probs, p)
        } else {
            filtered_probs
        }
    }

    /// Generate next token from logits
    pub fn next_token(&mut self, logits: &[f32]) -> Result<u32> {
        if logits.is_empty() {
            return Err(adapteros_core::AosError::Worker(
                "Empty logits provided".to_string(),
            ));
        }

        let final_probs = self.process_logits(logits);

        // Sample from the distribution
        self.sample_from_distribution(&final_probs)
    }

    /// Compute maximum selection probability for current logits
    ///
    /// Applies temperature scaling and the same filtering (top-k/top-p)
    /// as `next_token`, then returns the maximum probability. This can be
    /// used as a conservative proxy for model confidence during generation.
    pub fn max_prob(&self, logits: &[f32]) -> f32 {
        if logits.is_empty() {
            return 0.0;
        }

        let final_probs = self.process_logits(logits);

        final_probs
            .iter()
            .copied()
            .fold(0.0f32, |m, v| if v > m { v } else { m })
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

    #[test]
    fn test_deterministic_step_level_seeding() {
        let seed = b"test-deterministic-generation!!";
        let logits = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        // Create two generators with same base seed
        let mut gen1 = Generator::new_deterministic(seed, "inference");
        let mut gen2 = Generator::new_deterministic(seed, "inference");

        assert!(gen1.is_deterministic());
        assert!(gen2.is_deterministic());

        // Simulate generation loop - each step should produce same result
        for step in 0..5 {
            gen1.reseed_for_step(step);
            gen2.reseed_for_step(step);

            let token1 = gen1
                .next_token(&logits)
                .expect("Test token generation should succeed");
            let token2 = gen2
                .next_token(&logits)
                .expect("Test token generation should succeed");

            assert_eq!(
                token1, token2,
                "Step {} should produce identical tokens",
                step
            );
        }
    }

    #[test]
    fn test_step_seeding_produces_different_results_per_step() {
        let seed = b"test-deterministic-generation!!";
        let logits = vec![1.0, 1.0, 1.0, 1.0, 1.0]; // Uniform to ensure sampling variety

        let mut gen = Generator::new_deterministic(seed, "inference");

        // Collect tokens from different steps
        let mut tokens = Vec::new();
        for step in 0..10 {
            gen.reseed_for_step(step);
            let token = gen
                .next_token(&logits)
                .expect("Test token generation should succeed");
            tokens.push(token);
        }

        // Not all tokens should be the same (with very high probability)
        let first = tokens[0];
        let all_same = tokens.iter().all(|&t| t == first);
        // With uniform logits and 10 samples, the probability of all being the same is (1/5)^9 ≈ 5e-7
        assert!(
            !all_same,
            "Step-level seeding should produce varying results across steps"
        );
    }

    #[test]
    fn test_non_deterministic_mode_skips_reseeding() {
        let mut gen = Generator::new([42u8; 32]);
        assert!(!gen.is_deterministic());

        let logits = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        // Even with reseeding calls, non-deterministic mode should maintain normal behavior
        let token1 = gen
            .next_token(&logits)
            .expect("Test token generation should succeed");
        gen.reseed_for_step(0);
        let token2 = gen
            .next_token(&logits)
            .expect("Test token generation should succeed");

        // These may or may not be equal, but the test ensures no crash
        // In non-deterministic mode, reseed_for_step is a no-op
        let _ = (token1, token2);
    }
}
