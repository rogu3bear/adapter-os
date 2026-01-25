//! LoRA (Low-Rank Adaptation) for embedding models
//!
//! Provides lightweight fine-tuning by learning low-rank updates
//! to embedding projection layers. This enables adapting pre-trained
//! embedding models to specific domains with minimal parameter overhead.
//!
//! # Architecture
//!
//! LoRA decomposes weight updates as: W' = W + BA
//! where B is (input_dim x rank) and A is (rank x input_dim).
//!
//! The forward pass computes: output = input + (input @ A @ B) * (alpha / rank)
//!
//! # Determinism
//!
//! All operations use BLAKE3 hashing for deterministic adapter identification
//! and reproducible training checkpoints.

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// LoRA configuration for embedding adaptation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingLoraConfig {
    /// LoRA rank (bottleneck dimension)
    /// Lower rank = fewer parameters, higher rank = more expressiveness
    pub rank: usize,
    /// LoRA alpha (scaling factor)
    /// The output is scaled by alpha/rank
    pub alpha: f32,
    /// Dropout probability (0.0 to disable)
    /// Applied during training to prevent overfitting
    pub dropout: f32,
    /// Target layers to adapt (empty = all projection layers)
    pub target_layers: Vec<String>,
}

impl Default for EmbeddingLoraConfig {
    fn default() -> Self {
        Self {
            rank: 8,
            alpha: 16.0,
            dropout: 0.0,
            target_layers: vec![],
        }
    }
}

impl EmbeddingLoraConfig {
    /// Create a new config with specified rank and alpha
    pub fn new(rank: usize, alpha: f32) -> Self {
        assert!(rank > 0, "rank must be positive");
        assert!(alpha.is_finite(), "alpha must be finite");
        Self {
            rank,
            alpha,
            ..Default::default()
        }
    }

    /// Set dropout probability
    pub fn with_dropout(mut self, dropout: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&dropout),
            "dropout must be in range 0.0..=1.0"
        );
        self.dropout = dropout;
        self
    }

    /// Set target layers
    pub fn with_target_layers(mut self, layers: Vec<String>) -> Self {
        self.target_layers = layers;
        self
    }

    /// Compute the scaling factor (alpha / rank)
    pub fn scaling_factor(&self) -> f32 {
        self.alpha / self.rank as f32
    }
}

/// LoRA adapter weights for a single layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingLoraAdapter {
    /// Configuration
    pub config: EmbeddingLoraConfig,
    /// Input dimension (embedding dimension)
    pub input_dim: usize,
    /// A matrix (input_dim x rank) - initialized to small random values
    /// Stored as row-major: lora_a[row][col] where row < input_dim, col < rank
    pub lora_a: Vec<Vec<f32>>,
    /// B matrix (rank x input_dim) - initialized to zeros
    /// Stored as row-major: lora_b[row][col] where row < rank, col < input_dim
    pub lora_b: Vec<Vec<f32>>,
    /// Adapter hash for tracking (computed lazily)
    #[serde(skip)]
    adapter_hash: Option<B3Hash>,
}

impl EmbeddingLoraAdapter {
    /// Create new adapter with given dimension and config
    ///
    /// Initialization strategy (following standard LoRA):
    /// - A matrix: Kaiming uniform initialization scaled by 1/sqrt(rank)
    /// - B matrix: Zero initialization
    ///
    /// This ensures the initial LoRA contribution is zero: BA = 0
    pub fn new(input_dim: usize, config: EmbeddingLoraConfig) -> Self {
        let rank = config.rank;

        // Initialize A with small values (scaled Kaiming uniform approximation)
        // Using deterministic initialization based on dimensions
        let scale = 1.0 / (rank as f32).sqrt();
        let lora_a: Vec<Vec<f32>> = (0..input_dim)
            .map(|i| {
                (0..rank)
                    .map(|j| {
                        // Deterministic pseudo-random initialization
                        // This creates a reproducible pattern based on indices
                        let idx = i * rank + j;
                        let val = ((idx as f32 * 0.618) % 1.0 - 0.5) * 2.0;
                        val * scale
                    })
                    .collect()
            })
            .collect();

        // Initialize B to zeros (standard LoRA initialization)
        let lora_b: Vec<Vec<f32>> = (0..rank).map(|_| vec![0.0; input_dim]).collect();

        Self {
            config,
            input_dim,
            lora_a,
            lora_b,
            adapter_hash: None,
        }
    }

    /// Create adapter with custom initialization (for loading or testing)
    pub fn with_weights(
        input_dim: usize,
        config: EmbeddingLoraConfig,
        lora_a: Vec<Vec<f32>>,
        lora_b: Vec<Vec<f32>>,
    ) -> Self {
        assert!(input_dim > 0, "input_dim must be positive");
        assert_eq!(lora_a.len(), input_dim, "lora_a rows must match input_dim");
        assert!(
            lora_a.iter().all(|row| row.len() == config.rank),
            "lora_a cols must match rank"
        );
        assert_eq!(lora_b.len(), config.rank, "lora_b rows must match rank");
        assert!(
            lora_b.iter().all(|row| row.len() == input_dim),
            "lora_b cols must match input_dim"
        );
        Self {
            config,
            input_dim,
            lora_a,
            lora_b,
            adapter_hash: None,
        }
    }

    /// Forward pass: output = input + (input @ A @ B) * (alpha / rank)
    ///
    /// Matrix multiplication order:
    /// 1. tmp = input (1 x input_dim) @ A (input_dim x rank) = (1 x rank)
    /// 2. lora_out = tmp (1 x rank) @ B (rank x input_dim) = (1 x input_dim)
    /// 3. output = input + lora_out * scaling_factor
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        assert_eq!(
            input.len(),
            self.input_dim,
            "Input dimension mismatch: expected {}, got {}",
            self.input_dim,
            input.len()
        );

        let rank = self.config.rank;
        let scaling = self.config.scaling_factor();

        // Step 1: input @ A (input_dim -> rank)
        let mut tmp = vec![0.0; rank];
        for (i, row) in self.lora_a.iter().enumerate() {
            let input_val = input[i];
            for (j, &val) in row.iter().enumerate() {
                tmp[j] += input_val * val;
            }
        }

        // Step 2: tmp @ B (rank -> input_dim)
        let mut lora_out = vec![0.0; self.input_dim];
        for (j, out_val) in lora_out.iter_mut().enumerate() {
            for (i, &tmp_val) in tmp.iter().enumerate() {
                *out_val += tmp_val * self.lora_b[i][j];
            }
        }

        // Step 3: input + lora_out * scaling
        input
            .iter()
            .zip(lora_out.iter())
            .map(|(x, delta)| x + delta * scaling)
            .collect()
    }

    /// Forward pass for a batch of inputs
    pub fn forward_batch(&self, inputs: &[Vec<f32>]) -> Vec<Vec<f32>> {
        inputs.iter().map(|input| self.forward(input)).collect()
    }

    /// Compute deterministic hash of adapter weights using BLAKE3
    ///
    /// The hash includes:
    /// - Configuration (rank, alpha, dropout, target_layers)
    /// - Input dimension
    /// - All weights from lora_a and lora_b matrices
    pub fn compute_hash(&self) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash configuration
        hasher.update(&self.config.rank.to_le_bytes());
        hasher.update(&self.config.alpha.to_le_bytes());
        hasher.update(&self.config.dropout.to_le_bytes());
        for layer in &self.config.target_layers {
            hasher.update(layer.as_bytes());
        }

        // Hash dimension
        hasher.update(&self.input_dim.to_le_bytes());

        // Hash lora_a weights
        for row in &self.lora_a {
            for val in row {
                hasher.update(&val.to_le_bytes());
            }
        }

        // Hash lora_b weights
        for row in &self.lora_b {
            for val in row {
                hasher.update(&val.to_le_bytes());
            }
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Get cached or compute adapter hash
    pub fn adapter_hash(&mut self) -> B3Hash {
        if self.adapter_hash.is_none() {
            self.adapter_hash = Some(self.compute_hash());
        }
        self.adapter_hash.unwrap()
    }

    /// Invalidate cached hash (call after modifying weights)
    pub fn invalidate_hash(&mut self) {
        self.adapter_hash = None;
    }

    /// Save adapter to JSON file
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load adapter from JSON file
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Get the total number of trainable parameters
    pub fn num_parameters(&self) -> usize {
        let rank = self.config.rank;
        // A: input_dim x rank, B: rank x input_dim
        2 * self.input_dim * rank
    }

    /// Get parameter efficiency ratio compared to full fine-tuning
    /// Returns the ratio of LoRA params to full weight matrix params
    pub fn parameter_efficiency(&self) -> f32 {
        let full_params = self.input_dim * self.input_dim;
        self.num_parameters() as f32 / full_params as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lora_config_default() {
        let config = EmbeddingLoraConfig::default();
        assert_eq!(config.rank, 8);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(config.dropout, 0.0);
        assert!(config.target_layers.is_empty());
    }

    #[test]
    fn test_lora_config_new() {
        let config = EmbeddingLoraConfig::new(16, 32.0);
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
    }

    #[test]
    fn test_lora_config_builder() {
        let config = EmbeddingLoraConfig::new(4, 8.0)
            .with_dropout(0.1)
            .with_target_layers(vec!["query".to_string(), "key".to_string()]);
        assert_eq!(config.rank, 4);
        assert_eq!(config.alpha, 8.0);
        assert_eq!(config.dropout, 0.1);
        assert_eq!(config.target_layers.len(), 2);
    }

    #[test]
    fn test_scaling_factor() {
        let config = EmbeddingLoraConfig::new(8, 16.0);
        assert!((config.scaling_factor() - 2.0).abs() < 1e-6);

        let config2 = EmbeddingLoraConfig::new(4, 4.0);
        assert!((config2.scaling_factor() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_lora_adapter_creation() {
        let config = EmbeddingLoraConfig::default();
        let adapter = EmbeddingLoraAdapter::new(384, config);

        assert_eq!(adapter.input_dim, 384);
        assert_eq!(adapter.lora_a.len(), 384); // input_dim rows
        assert_eq!(adapter.lora_a[0].len(), 8); // rank columns
        assert_eq!(adapter.lora_b.len(), 8); // rank rows
        assert_eq!(adapter.lora_b[0].len(), 384); // input_dim columns
    }

    #[test]
    fn test_lora_b_initialized_to_zero() {
        let config = EmbeddingLoraConfig::default();
        let adapter = EmbeddingLoraAdapter::new(64, config);

        // B should be all zeros
        for row in &adapter.lora_b {
            for val in row {
                assert_eq!(*val, 0.0);
            }
        }
    }

    #[test]
    fn test_lora_forward_pass_identity_when_b_zero() {
        // When B is zero, output should equal input (since BA = 0)
        let config = EmbeddingLoraConfig::new(4, 4.0);
        let adapter = EmbeddingLoraAdapter::new(4, config);

        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = adapter.forward(&input);

        // Since B is initialized to zeros, lora contribution is zero
        for (i, o) in input.iter().zip(output.iter()) {
            assert!((i - o).abs() < 1e-6, "Expected {} but got {}", i, o);
        }
    }

    #[test]
    fn test_lora_forward_pass_with_weights() {
        let config = EmbeddingLoraConfig {
            rank: 2,
            alpha: 2.0, // scaling = 1.0
            dropout: 0.0,
            target_layers: vec![],
        };

        // Create a simple 3x2 A matrix and 2x3 B matrix
        let lora_a = vec![
            vec![1.0, 0.0], // row 0
            vec![0.0, 1.0], // row 1
            vec![1.0, 1.0], // row 2
        ];
        let lora_b = vec![
            vec![1.0, 0.0, 0.0], // row 0
            vec![0.0, 1.0, 0.0], // row 1
        ];

        let adapter = EmbeddingLoraAdapter::with_weights(3, config, lora_a, lora_b);

        let input = vec![1.0, 0.0, 0.0];
        let output = adapter.forward(&input);

        // tmp = input @ A = [1.0, 0.0] @ A = [1.0, 0.0]
        // lora_out = tmp @ B = [1.0, 0.0] @ B = [1.0, 0.0, 0.0]
        // output = input + lora_out * 1.0 = [2.0, 0.0, 0.0]
        assert_eq!(output.len(), 3);
        assert!((output[0] - 2.0).abs() < 1e-6);
        assert!((output[1] - 0.0).abs() < 1e-6);
        assert!((output[2] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_lora_forward_batch() {
        let config = EmbeddingLoraConfig::new(4, 4.0);
        let adapter = EmbeddingLoraAdapter::new(4, config);

        let inputs = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.5, 0.5, 0.0, 0.0],
        ];

        let outputs = adapter.forward_batch(&inputs);
        assert_eq!(outputs.len(), 3);
        for output in &outputs {
            assert_eq!(output.len(), 4);
        }
    }

    #[test]
    fn test_lora_adapter_hash_deterministic() {
        let config = EmbeddingLoraConfig::default();
        let adapter1 = EmbeddingLoraAdapter::new(384, config.clone());
        let adapter2 = EmbeddingLoraAdapter::new(384, config);

        // Same config and initialization should produce same hash
        let hash1 = adapter1.compute_hash();
        let hash2 = adapter2.compute_hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_lora_adapter_hash_differs_with_config() {
        let config1 = EmbeddingLoraConfig::new(8, 16.0);
        let config2 = EmbeddingLoraConfig::new(16, 16.0);

        let adapter1 = EmbeddingLoraAdapter::new(64, config1);
        let adapter2 = EmbeddingLoraAdapter::new(64, config2);

        let hash1 = adapter1.compute_hash();
        let hash2 = adapter2.compute_hash();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_lora_adapter_hash_differs_with_weights() {
        let config = EmbeddingLoraConfig::default();
        let mut adapter = EmbeddingLoraAdapter::new(64, config);

        let hash_before = adapter.compute_hash();

        // Modify a weight
        adapter.lora_b[0][0] = 1.0;

        let hash_after = adapter.compute_hash();
        assert_ne!(hash_before, hash_after);
    }

    #[test]
    fn test_lora_adapter_hash_caching() {
        let config = EmbeddingLoraConfig::default();
        let mut adapter = EmbeddingLoraAdapter::new(64, config);

        let hash1 = adapter.adapter_hash();
        let hash2 = adapter.adapter_hash();
        assert_eq!(hash1, hash2);

        // Modify and invalidate
        adapter.lora_b[0][0] = 1.0;
        adapter.invalidate_hash();

        let hash3 = adapter.adapter_hash();
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_lora_serialization() {
        let config = EmbeddingLoraConfig::new(4, 8.0).with_dropout(0.1);
        let adapter = EmbeddingLoraAdapter::new(16, config);

        let json = serde_json::to_string(&adapter).unwrap();
        let parsed: EmbeddingLoraAdapter = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.config.rank, adapter.config.rank);
        assert_eq!(parsed.config.alpha, adapter.config.alpha);
        assert_eq!(parsed.config.dropout, adapter.config.dropout);
        assert_eq!(parsed.input_dim, adapter.input_dim);
        assert_eq!(parsed.lora_a.len(), adapter.lora_a.len());
        assert_eq!(parsed.lora_b.len(), adapter.lora_b.len());

        // Verify weights match
        for (row1, row2) in parsed.lora_a.iter().zip(adapter.lora_a.iter()) {
            for (v1, v2) in row1.iter().zip(row2.iter()) {
                assert!((v1 - v2).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn test_lora_save_load() {
        let config = EmbeddingLoraConfig::new(4, 8.0);
        let adapter = EmbeddingLoraAdapter::new(32, config);
        let hash_before = adapter.compute_hash();

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("adapter.json");

        adapter.save(&path).unwrap();
        let loaded = EmbeddingLoraAdapter::load(&path).unwrap();

        let hash_after = loaded.compute_hash();
        assert_eq!(hash_before, hash_after);
    }

    #[test]
    fn test_num_parameters() {
        let config = EmbeddingLoraConfig::new(8, 16.0);
        let adapter = EmbeddingLoraAdapter::new(384, config);

        // A: 384 x 8 = 3072
        // B: 8 x 384 = 3072
        // Total: 6144
        assert_eq!(adapter.num_parameters(), 6144);
    }

    #[test]
    fn test_parameter_efficiency() {
        let config = EmbeddingLoraConfig::new(8, 16.0);
        let adapter = EmbeddingLoraAdapter::new(384, config);

        // Full params: 384 * 384 = 147456
        // LoRA params: 6144
        // Ratio: 6144 / 147456 = 0.0416...
        let efficiency = adapter.parameter_efficiency();
        assert!(efficiency < 0.05);
        assert!(efficiency > 0.04);
    }

    #[test]
    #[should_panic(expected = "Input dimension mismatch")]
    fn test_lora_forward_dimension_mismatch() {
        let config = EmbeddingLoraConfig::new(4, 4.0);
        let adapter = EmbeddingLoraAdapter::new(8, config);

        let wrong_input = vec![1.0, 2.0, 3.0]; // Wrong size
        adapter.forward(&wrong_input);
    }

    #[test]
    fn test_lora_config_serialization() {
        let config = EmbeddingLoraConfig::new(16, 32.0)
            .with_dropout(0.05)
            .with_target_layers(vec!["proj".to_string()]);

        let json = serde_json::to_string(&config).unwrap();
        let parsed: EmbeddingLoraConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, parsed);
    }

    #[test]
    #[should_panic(expected = "rank must be positive")]
    fn test_lora_config_zero_rank() {
        EmbeddingLoraConfig::new(0, 16.0);
    }

    #[test]
    #[should_panic(expected = "alpha must be finite")]
    fn test_lora_config_infinite_alpha() {
        EmbeddingLoraConfig::new(8, f32::INFINITY);
    }

    #[test]
    #[should_panic(expected = "dropout must be in range")]
    fn test_lora_config_invalid_dropout() {
        EmbeddingLoraConfig::new(8, 16.0).with_dropout(1.5);
    }

    #[test]
    #[should_panic(expected = "dropout must be in range")]
    fn test_lora_config_negative_dropout() {
        EmbeddingLoraConfig::new(8, 16.0).with_dropout(-0.1);
    }

    #[test]
    #[should_panic(expected = "input_dim must be positive")]
    fn test_with_weights_zero_input_dim() {
        let config = EmbeddingLoraConfig::new(2, 4.0);
        EmbeddingLoraAdapter::with_weights(0, config, vec![], vec![vec![], vec![]]);
    }

    #[test]
    #[should_panic(expected = "lora_a rows must match input_dim")]
    fn test_with_weights_lora_a_wrong_rows() {
        let config = EmbeddingLoraConfig::new(2, 4.0);
        let lora_a = vec![vec![1.0, 0.0]]; // Only 1 row, but input_dim is 3
        let lora_b = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        EmbeddingLoraAdapter::with_weights(3, config, lora_a, lora_b);
    }

    #[test]
    #[should_panic(expected = "lora_a cols must match rank")]
    fn test_with_weights_lora_a_wrong_cols() {
        let config = EmbeddingLoraConfig::new(2, 4.0);
        let lora_a = vec![
            vec![1.0], // Only 1 col, but rank is 2
            vec![0.0],
            vec![1.0],
        ];
        let lora_b = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        EmbeddingLoraAdapter::with_weights(3, config, lora_a, lora_b);
    }

    #[test]
    #[should_panic(expected = "lora_b rows must match rank")]
    fn test_with_weights_lora_b_wrong_rows() {
        let config = EmbeddingLoraConfig::new(2, 4.0);
        let lora_a = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 1.0]];
        let lora_b = vec![vec![1.0, 0.0, 0.0]]; // Only 1 row, but rank is 2
        EmbeddingLoraAdapter::with_weights(3, config, lora_a, lora_b);
    }

    #[test]
    #[should_panic(expected = "lora_b cols must match input_dim")]
    fn test_with_weights_lora_b_wrong_cols() {
        let config = EmbeddingLoraConfig::new(2, 4.0);
        let lora_a = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 1.0]];
        let lora_b = vec![
            vec![1.0, 0.0], // Only 2 cols, but input_dim is 3
            vec![0.0, 1.0],
        ];
        EmbeddingLoraAdapter::with_weights(3, config, lora_a, lora_b);
    }
}
