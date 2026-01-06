//! Tokenizer validation command.

use crate::output::OutputWriter;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

/// Validate a tokenizer.json file.
#[derive(Debug, Clone, Args)]
pub struct CheckTokenizerArgs {
    /// Path to tokenizer.json
    pub path: PathBuf,

    /// Output results in JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct CheckTokenizerResult {
    path: String,
    valid: bool,
    error: Option<String>,
    model_type: Option<String>,
    vocab_size: Option<usize>,
}

impl CheckTokenizerArgs {
    pub async fn execute(&self, output: &OutputWriter) -> Result<()> {
        let path = &self.path;
        if !path.exists() {
            let msg = format!("Tokenizer file not found: {}", path.display());
            if output.is_json() {
                output.json(&CheckTokenizerResult {
                    path: path.display().to_string(),
                    valid: false,
                    error: Some(msg.clone()),
                    model_type: None,
                    vocab_size: None,
                })?;
                return Ok(());
            }
            return Err(anyhow!(msg));
        }

        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read tokenizer file: {}", path.display()))?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("Invalid tokenizer JSON: {}", path.display()))?;

        let model_type = value
            .get("model")
            .and_then(|m| m.get("type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let vocab_size = value
            .get("model")
            .and_then(|m| m.get("vocab_size"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .or_else(|| {
                value
                    .get("vocab")
                    .and_then(|v| v.as_object())
                    .map(|o| o.len())
            });

        if output.is_json() {
            output.json(&CheckTokenizerResult {
                path: path.display().to_string(),
                valid: true,
                error: None,
                model_type,
                vocab_size,
            })?;
            return Ok(());
        }

        output.success("Tokenizer JSON is valid");
        output.kv("Path", &path.display().to_string());
        if let Some(mt) = model_type {
            output.kv("Model", &mt);
        }
        if let Some(size) = vocab_size {
            output.kv("Vocab size", &size.to_string());
        }

        Ok(())
    }
}
