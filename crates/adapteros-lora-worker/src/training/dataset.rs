//! Dataset generation from patches for micro-LoRA training
//!
//! Converts code patches into training examples with tokenization and context windows.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1, TrainingExampleV1};
type TrainingExample = TrainingExampleV1;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{debug, info};

/// Dataset generator for creating training examples from patches
#[derive(Debug, Clone)]
pub struct DatasetGenerator {
    context_window: usize,
    min_examples: usize,
    pad_token_id: u32,
}

/// Summary of a deterministic train/validation split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSplitSummary {
    /// Clamped split ratio used (0.0-0.5).
    pub split_ratio: f32,
    /// Total examples considered for splitting.
    pub total_examples: usize,
    /// Number of training examples.
    pub train_count: usize,
    /// Number of validation examples.
    pub validation_count: usize,
    /// BLAKE3 hash of the ordered example hashes for split integrity.
    pub split_hash_b3: String,
}

/// File patch for training
#[derive(Debug, Clone)]
pub struct FilePatch {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub change_type: ChangeType,
}

/// Type of change in patch
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Add,
    Modify,
    Delete,
}

impl Default for DatasetGenerator {
    fn default() -> Self {
        Self {
            context_window: 512,
            min_examples: 10,
            pad_token_id: 0,
        }
    }
}

impl DatasetGenerator {
    /// Create a new dataset generator
    pub fn new(context_window: usize, min_examples: usize) -> Self {
        Self {
            context_window,
            min_examples,
            pad_token_id: 0,
        }
    }

    /// Set pad token id for attention mask generation.
    pub fn with_pad_token_id(mut self, pad_token_id: u32) -> Self {
        self.pad_token_id = pad_token_id;
        self
    }

    /// Generate training examples from patches
    pub fn generate_from_patches(&self, patches: &[FilePatch]) -> Result<Vec<TrainingExampleV1>> {
        info!(
            "Generating training examples from {} patches",
            patches.len()
        );

        let mut examples = Vec::new();

        for patch in patches {
            let patch_examples = self.generate_from_patch(patch)?;
            examples.extend(patch_examples);
        }

        if examples.len() < self.min_examples {
            return Err(AosError::Training(format!(
                "Not enough training examples: {} < {}",
                examples.len(),
                self.min_examples
            )));
        }

        info!("Generated {} training examples", examples.len());
        Ok(examples)
    }

    /// Generate examples from a single patch
    fn generate_from_patch(&self, patch: &FilePatch) -> Result<Vec<TrainingExampleV1>> {
        let mut examples = Vec::new();

        match patch.change_type {
            ChangeType::Add | ChangeType::Modify => {
                // For additions and modifications, create input-target pairs
                // Input: old content + context
                // Target: new content

                let old_tokens = self.tokenize(&patch.old_content);
                let new_tokens = self.tokenize(&patch.new_content);

                // Create sliding window pairs
                let pairs = self.create_pairs(&old_tokens, &new_tokens);

                for (pair_idx, (input, target)) in pairs.into_iter().enumerate() {
                    let mut provenance = BTreeMap::new();
                    provenance.insert(
                        "file_path".to_string(),
                        serde_json::Value::String(patch.file_path.clone()),
                    );
                    provenance.insert(
                        "change_type".to_string(),
                        serde_json::Value::String(format!("{:?}", patch.change_type)),
                    );
                    provenance.insert(
                        "file_extension".to_string(),
                        serde_json::Value::String(Self::extract_extension(&patch.file_path)),
                    );

                    let metadata = ExampleMetadataV1::new(
                        patch.file_path.clone(),
                        pair_idx as u64,
                        B3Hash::hash_multi(&[
                            patch.old_content.as_bytes(),
                            b"\0",
                            patch.new_content.as_bytes(),
                        ])
                        .to_hex(),
                        provenance_from_map(&provenance)
                            .map_err(|e| AosError::Training(format!("Metadata error: {}", e)))?,
                        0,
                    );
                    let attention_mask =
                        TrainingExampleV1::attention_mask_from_tokens(&input, self.pad_token_id);
                    examples.push(TrainingExampleV1::new(
                        input,
                        target,
                        attention_mask,
                        metadata,
                    ));
                }
            }
            ChangeType::Delete => {
                // For deletions, we can optionally create examples that learn
                // to predict empty output, but for now skip them
                debug!("Skipping deletion patch for {}", patch.file_path);
            }
        }

        Ok(examples)
    }

    /// Tokenize text into token IDs
    ///
    /// This is a simple whitespace-based tokenizer for demonstration.
    /// In production, this would use the actual model tokenizer.
    pub fn tokenize(&self, text: &str) -> Vec<u32> {
        let mut tokens = Vec::new();

        // Simple character-level tokenization for now
        // In production, use proper BPE/SentencePiece tokenizer
        for ch in text.chars() {
            let token_id = ch as u32;
            tokens.push(token_id);
        }

        tokens
    }

    /// Create input-target pairs from token sequences
    pub fn create_pairs(
        &self,
        old_tokens: &[u32],
        new_tokens: &[u32],
    ) -> Vec<(Vec<u32>, Vec<u32>)> {
        let mut pairs = Vec::new();

        // Strategy: Create sliding windows of context_window size
        // Input: chunk of old_tokens
        // Target: corresponding chunk of new_tokens

        let max_len = old_tokens.len().max(new_tokens.len());
        let mut offset = 0;

        while offset < max_len {
            let input_end = (offset + self.context_window).min(old_tokens.len());
            let target_end = (offset + self.context_window).min(new_tokens.len());

            let input = old_tokens[offset..input_end].to_vec();
            let target = new_tokens[offset..target_end].to_vec();

            // Only add if both have content
            if !input.is_empty() && !target.is_empty() {
                pairs.push((input, target));
            }

            offset += self.context_window / 2; // 50% overlap
        }

        pairs
    }

    /// Extract file extension
    fn extract_extension(file_path: &str) -> String {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Validate training examples
    pub fn validate_examples(&self, examples: &[TrainingExampleV1]) -> Result<()> {
        if examples.is_empty() {
            return Err(AosError::Training("No training examples".to_string()));
        }

        for (idx, example) in examples.iter().enumerate() {
            if example.input_tokens.is_empty() {
                return Err(AosError::Training(format!(
                    "Example {} has empty input",
                    idx
                )));
            }
            if example.target_tokens.is_empty() {
                return Err(AosError::Training(format!(
                    "Example {} has empty target",
                    idx
                )));
            }
            if example.attention_mask.len() != example.input_tokens.len() {
                return Err(AosError::Training(format!(
                    "Example {} attention_mask length mismatch",
                    idx
                )));
            }
            if example.input_tokens.len() > self.context_window {
                return Err(AosError::Training(format!(
                    "Example {} input exceeds context window: {} > {}",
                    idx,
                    example.input_tokens.len(),
                    self.context_window
                )));
            }
            if example.target_tokens.len() > self.context_window {
                return Err(AosError::Training(format!(
                    "Example {} target exceeds context window: {} > {}",
                    idx,
                    example.target_tokens.len(),
                    self.context_window
                )));
            }
        }

        Ok(())
    }
}

pub fn example_hash_for_tokens(
    seed: u64,
    input_tokens: &[u32],
    target_tokens: &[u32],
    attention_mask: &[u8],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(
        input_tokens.len() * 4 + target_tokens.len() * 4 + attention_mask.len() + 8,
    );
    buf.extend_from_slice(&seed.to_le_bytes());
    for token in input_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }
    for token in target_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }
    buf.extend_from_slice(attention_mask);
    let hash = blake3::hash(&buf);
    *hash.as_bytes()
}

pub fn example_hash_for_seed(seed: u64, example: &TrainingExampleV1) -> [u8; 32] {
    example_hash_for_tokens(
        seed,
        &example.input_tokens,
        &example.target_tokens,
        &example.attention_mask,
    )
}

fn split_hash_from_hashes(
    hashes: impl Iterator<Item = [u8; 32]>,
    seed: u64,
    split_ratio: f32,
    total: usize,
    train_len: usize,
) -> String {
    let mut hasher = Hasher::new();
    hasher.update(&seed.to_le_bytes());
    hasher.update(&split_ratio.to_le_bytes());
    hasher.update(&(total as u64).to_le_bytes());
    hasher.update(&(train_len as u64).to_le_bytes());
    for hash in hashes {
        hasher.update(&hash);
    }
    hasher.finalize().to_hex().to_string()
}

/// Compute a deterministic split hash for pre-split training examples.
///
/// This is useful when train/validation sets are provided explicitly while
/// still emitting a comparable split hash for reporting and audits.
pub fn compute_split_hash_for_sets(
    train_examples: &[TrainingExampleV1],
    validation_examples: &[TrainingExampleV1],
    seed: u64,
    split_ratio: f32,
) -> String {
    let mut train_hashes: Vec<[u8; 32]> = train_examples
        .iter()
        .map(|ex| example_hash_for_seed(seed, ex))
        .collect();
    let mut validation_hashes: Vec<[u8; 32]> = validation_examples
        .iter()
        .map(|ex| example_hash_for_seed(seed, ex))
        .collect();

    train_hashes.sort();
    validation_hashes.sort();

    let mut combined = Vec::with_capacity(train_hashes.len() + validation_hashes.len());
    combined.extend(train_hashes);
    combined.extend(validation_hashes);

    let total = combined.len();
    let train_len = train_examples.len();
    let split = split_ratio.clamp(0.0, 0.5);

    split_hash_from_hashes(combined.into_iter(), seed, split, total, train_len)
}

/// Deterministically split examples into train and validation sets.
///
/// The split is stable for a given seed + dataset contents, producing the same
/// ordering as the trainer's internal split logic.
pub fn split_examples_for_validation(
    examples: &[TrainingExampleV1],
    split_ratio: f32,
    seed: u64,
) -> (
    Vec<TrainingExampleV1>,
    Vec<TrainingExampleV1>,
    ValidationSplitSummary,
) {
    let split = split_ratio.clamp(0.0, 0.5);
    let total = examples.len();

    if split <= 0.0 || total <= 1 {
        let hashes = examples.iter().map(|ex| example_hash_for_seed(seed, ex));
        let split_hash_b3 = split_hash_from_hashes(hashes, seed, split, total, total);
        return (
            examples.to_vec(),
            Vec::new(),
            ValidationSplitSummary {
                split_ratio: split,
                total_examples: total,
                train_count: total,
                validation_count: 0,
                split_hash_b3,
            },
        );
    }

    let mut hashed: Vec<([u8; 32], TrainingExampleV1)> = examples
        .iter()
        .cloned()
        .map(|ex| (example_hash_for_seed(seed, &ex), ex))
        .collect();

    hashed.sort_by_key(|(hash, _)| *hash);

    let mut train_len = ((total as f32) * (1.0 - split)).floor() as usize;
    if train_len >= total {
        train_len = total.saturating_sub(1);
    }

    let split_hash_b3 = split_hash_from_hashes(
        hashed.iter().map(|(hash, _)| *hash),
        seed,
        split,
        total,
        train_len,
    );

    let validation_pairs = hashed.split_off(train_len);
    let train_examples: Vec<TrainingExampleV1> = hashed.into_iter().map(|(_, ex)| ex).collect();
    let validation_examples: Vec<TrainingExampleV1> =
        validation_pairs.into_iter().map(|(_, ex)| ex).collect();

    (
        train_examples,
        validation_examples,
        ValidationSplitSummary {
            split_ratio: split,
            total_examples: total,
            train_count: train_len,
            validation_count: total.saturating_sub(train_len),
            split_hash_b3,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_types::training::ExampleMetadataV1;

    fn make_example(
        input_tokens: Vec<u32>,
        target_tokens: Vec<u32>,
        row_id: u64,
    ) -> TrainingExampleV1 {
        let metadata = ExampleMetadataV1::new("test", row_id, "row-hash", "{}", 0);
        let attention_mask = TrainingExampleV1::attention_mask_from_tokens(&input_tokens, 0);
        TrainingExampleV1::new(input_tokens, target_tokens, attention_mask, metadata)
    }

    #[test]
    fn test_tokenize() {
        let gen = DatasetGenerator::default();
        let tokens = gen.tokenize("hello");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0], 'h' as u32);
    }

    #[test]
    fn test_create_pairs() {
        let gen = DatasetGenerator::new(10, 1);
        let old_tokens: Vec<u32> = (0..20).collect();
        let new_tokens: Vec<u32> = (0..20).collect();

        let pairs = gen.create_pairs(&old_tokens, &new_tokens);
        assert!(!pairs.is_empty());
        assert!(pairs[0].0.len() <= 10);
    }

    #[test]
    fn test_extract_extension() {
        assert_eq!(DatasetGenerator::extract_extension("file.rs"), "rs");
        assert_eq!(
            DatasetGenerator::extract_extension("path/to/file.toml"),
            "toml"
        );
        assert_eq!(DatasetGenerator::extract_extension("no_ext"), "unknown");
    }

    #[test]
    fn test_generate_from_patch() {
        let gen = DatasetGenerator::new(50, 1);
        let patch = FilePatch {
            file_path: "test.rs".to_string(),
            old_content: "fn old() {}".to_string(),
            new_content: "fn new() {}".to_string(),
            change_type: ChangeType::Modify,
        };

        let examples = gen.generate_from_patch(&patch).unwrap();
        assert!(!examples.is_empty());
        let provenance: serde_json::Value =
            serde_json::from_str(&examples[0].metadata.provenance).unwrap();
        assert_eq!(
            provenance.get("file_path").and_then(|v| v.as_str()),
            Some("test.rs")
        );
    }

    #[test]
    fn test_validate_examples() {
        let gen = DatasetGenerator::default();
        let examples = vec![make_example(vec![1, 2, 3], vec![4, 5, 6], 1)];

        assert!(gen.validate_examples(&examples).is_ok());
    }

    #[test]
    fn test_validate_empty_examples() {
        let gen = DatasetGenerator::default();
        let examples: Vec<TrainingExample> = vec![];

        assert!(gen.validate_examples(&examples).is_err());
    }

    #[test]
    fn test_validate_target_over_context() {
        let gen = DatasetGenerator::new(4, 1);
        let examples = vec![make_example(vec![1, 2], vec![1, 2, 3, 4, 5], 1)];

        assert!(gen.validate_examples(&examples).is_err());
    }

    #[test]
    fn split_examples_is_deterministic() {
        let examples = vec![
            make_example(vec![1, 2], vec![3, 4], 1),
            make_example(vec![5, 6], vec![7, 8], 2),
            make_example(vec![9], vec![10], 3),
        ];

        let (train_a, val_a, summary_a) = split_examples_for_validation(&examples, 0.25, 42);
        let (train_b, val_b, summary_b) = split_examples_for_validation(&examples, 0.25, 42);

        assert_eq!(summary_a.split_hash_b3, summary_b.split_hash_b3);
        assert_eq!(train_a.len(), train_b.len());
        assert_eq!(val_a.len(), val_b.len());
        assert_eq!(
            summary_a.train_count + summary_a.validation_count,
            examples.len()
        );
    }
}
