//! Tokenizer validation command for aosctl.
//!
//! Provides:
//! - `aosctl models check-tokenizer <path>` - Validate a tokenizer.json file

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tokenizers::Tokenizer;

use crate::output::OutputWriter;

/// Arguments for the check-tokenizer command.
#[derive(Debug, Clone, Parser)]
pub struct CheckTokenizerArgs {
    /// Path to tokenizer.json file
    #[arg(required = true)]
    pub path: PathBuf,
}

impl CheckTokenizerArgs {
    /// Execute the check-tokenizer command.
    pub async fn execute(&self, output: &OutputWriter) -> Result<()> {
        output.section("Tokenizer Validation");

        // 1. Validate file exists
        if !self.path.exists() {
            output.error(format!("File not found: {}", self.path.display()));
            anyhow::bail!("Tokenizer file does not exist: {}", self.path.display());
        }

        if !self.path.is_file() {
            output.error(format!("Path is not a file: {}", self.path.display()));
            anyhow::bail!("Path is not a file: {}", self.path.display());
        }

        output.kv("Path", &self.path.display().to_string());

        // 2. Load tokenizer
        let tokenizer = Tokenizer::from_file(&self.path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to load tokenizer from {}: {}",
                self.path.display(),
                e
            )
        })?;

        output.success("Tokenizer loaded successfully");

        // 3. Get vocab size
        let vocab_size = tokenizer.get_vocab_size(true);
        output.kv("Vocab size", &vocab_size.to_string());

        // 4. Check for special tokens
        output.blank();
        output.info("Special tokens:");

        // Get the full vocabulary including special tokens
        let vocab = tokenizer.get_vocab(true);

        // Helper to find a token matching any of the patterns
        fn find_special_token<'a>(
            vocab: &'a std::collections::HashMap<String, u32>,
            patterns: &[&str],
        ) -> Option<(&'a String, u32)> {
            vocab.iter().find_map(|(token, &id)| {
                let token_lower = token.to_lowercase();
                for pattern in patterns {
                    if token_lower.contains(pattern) || token == *pattern {
                        return Some((token, id));
                    }
                }
                None
            })
        }

        // Check BOS token
        let bos_token = find_special_token(
            &vocab,
            &["bos", "<s>", "<|begin_of_text|>", "<|startoftext|>"],
        );

        if let Some((token, id)) = bos_token {
            output.kv("  BOS token", &format!("{} (id: {})", token, id));
        } else {
            output.warning("  BOS token: not found");
        }

        // Check EOS token
        let eos_token = find_special_token(
            &vocab,
            &[
                "eos",
                "</s>",
                "<|end_of_text|>",
                "<|endoftext|>",
                "<|im_end|>",
            ],
        );

        if let Some((token, id)) = eos_token {
            output.kv("  EOS token", &format!("{} (id: {})", token, id));
        } else {
            output.warning("  EOS token: not found");
        }

        // Check PAD token
        let pad_token = find_special_token(&vocab, &["pad", "<pad>", "<|pad|>"]);

        if let Some((token, id)) = pad_token {
            output.kv("  PAD token", &format!("{} (id: {})", token, id));
        } else {
            output.info("  PAD token: not found (optional)");
        }

        // Check UNK token
        let unk_token = find_special_token(&vocab, &["unk", "<unk>", "<|unk|>"]);

        if let Some((token, id)) = unk_token {
            output.kv("  UNK token", &format!("{} (id: {})", token, id));
        } else {
            output.info("  UNK token: not found (optional)");
        }

        // 5. Report model type (inspect the tokenizer model)
        output.blank();

        // Try to determine tokenizer type by encoding a sample
        let sample_text = "Hello, world!";
        let encoding = tokenizer
            .encode(sample_text, false)
            .map_err(|e| anyhow::anyhow!("Failed to encode sample text: {}", e))?;

        output.kv(
            "Sample encoding",
            &format!("\"{}\" -> {} tokens", sample_text, encoding.len()),
        );

        // 6. Summary
        output.blank();
        let has_required_tokens = bos_token.is_some() || eos_token.is_some();

        if has_required_tokens {
            output.success("Tokenizer is valid and ready for use");
        } else {
            output.warning("Tokenizer may be missing required special tokens (BOS/EOS)");
            output.info("Some models may still work without explicit BOS/EOS tokens");
        }

        // JSON output if requested
        if output.is_json() {
            let json_data = serde_json::json!({
                "path": self.path.display().to_string(),
                "vocab_size": vocab_size,
                "special_tokens": {
                    "bos": bos_token.map(|(t, id)| serde_json::json!({"token": t, "id": id})),
                    "eos": eos_token.map(|(t, id)| serde_json::json!({"token": t, "id": id})),
                    "pad": pad_token.map(|(t, id)| serde_json::json!({"token": t, "id": id})),
                    "unk": unk_token.map(|(t, id)| serde_json::json!({"token": t, "id": id})),
                },
                "sample_encoding": {
                    "input": sample_text,
                    "token_count": encoding.len(),
                },
                "valid": has_required_tokens,
            });
            output.json(&json_data)?;
        }

        Ok(())
    }
}
