//! Dataset generation from patches for micro-LoRA training
//!
//! Converts code patches into training examples with tokenization and context windows.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Dataset generator for creating training examples from patches
#[derive(Debug, Clone)]
pub struct DatasetGenerator {
    context_window: usize,
    min_examples: usize,
}

/// Single training example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Input token IDs
    pub input: Vec<u32>,
    /// Target token IDs
    pub target: Vec<u32>,
    /// Example metadata
    pub metadata: HashMap<String, String>,
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
        }
    }
}

impl DatasetGenerator {
    /// Create a new dataset generator
    pub fn new(context_window: usize, min_examples: usize) -> Self {
        Self {
            context_window,
            min_examples,
        }
    }

    /// Generate training examples from patches
    pub fn generate_from_patches(&self, patches: &[FilePatch]) -> Result<Vec<TrainingExample>> {
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
    fn generate_from_patch(&self, patch: &FilePatch) -> Result<Vec<TrainingExample>> {
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

                for (input, target) in pairs {
                    let mut metadata = HashMap::new();
                    metadata.insert("file_path".to_string(), patch.file_path.clone());
                    metadata.insert(
                        "change_type".to_string(),
                        format!("{:?}", patch.change_type),
                    );
                    metadata.insert(
                        "file_extension".to_string(),
                        Self::extract_extension(&patch.file_path),
                    );

                    examples.push(TrainingExample {
                        input,
                        target,
                        metadata,
                    });
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
    pub fn validate_examples(&self, examples: &[TrainingExample]) -> Result<()> {
        if examples.is_empty() {
            return Err(AosError::Training("No training examples".to_string()));
        }

        for (idx, example) in examples.iter().enumerate() {
            if example.input.is_empty() {
                return Err(AosError::Training(format!(
                    "Example {} has empty input",
                    idx
                )));
            }
            if example.target.is_empty() {
                return Err(AosError::Training(format!(
                    "Example {} has empty target",
                    idx
                )));
            }
            if example.input.len() > self.context_window {
                return Err(AosError::Training(format!(
                    "Example {} input exceeds context window: {} > {}",
                    idx,
                    example.input.len(),
                    self.context_window
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(examples[0].metadata.get("file_path").unwrap(), "test.rs");
    }

    #[test]
    fn test_validate_examples() {
        let gen = DatasetGenerator::default();
        let examples = vec![TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
        }];

        assert!(gen.validate_examples(&examples).is_ok());
    }

    #[test]
    fn test_validate_empty_examples() {
        let gen = DatasetGenerator::default();
        let examples: Vec<TrainingExample> = vec![];

        assert!(gen.validate_examples(&examples).is_err());
    }
}
