//! Train base adapter command implementation for CLI
//!
//! This module is temporarily stubbed pending reimplementation.
//!
//! Loads the curated dataset manifest, runs the deterministic Micro-LoRA trainer,
//! and packages quantized weights into `adapters/<adapter_id>/`.

use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_core::{AosError, Result};
// Removed: use adapteros_lora_worker::tokenizer::QwenTokenizer;
// Removed: use adapteros_lora_worker::training::{...};
// Removed: use adapteros_single_file_adapter::{...};

use clap::Args;
use std::path::PathBuf;

/// Train base adapter arguments
#[derive(Args, Debug, Clone)]
pub struct TrainBaseAdapterArgs {
    /// Dataset manifest describing positive/negative samples
    #[arg(
        long,
        default_value = "training/datasets/base/code/adapteros/manifest.json"
    )]
    pub manifest: PathBuf,

    /// Output adapters directory
    #[arg(long, default_value = "adapters")]
    pub output_dir: PathBuf,

    /// Output format: directory or aos
    #[arg(long, default_value = "directory")]
    pub output_format: String,

    /// Adapter ID (used for packaged directory name)
    #[arg(long, default_value = "code_lang_v1")]
    pub adapter_id: String,

    /// Tokenizer configuration
    #[command(flatten)]
    pub tokenizer_arg: TokenizerArg,

    /// Common training hyperparameters
    #[command(flatten)]
    pub common: CommonTrainingArgs,
}

impl TrainBaseAdapterArgs {
    /// Execute the train-base-adapter command
    pub async fn execute(&self) -> Result<()> {
        tracing::warn!(
            "train-base-adapter command is temporarily disabled pending reimplementation"
        );

        Err(AosError::Config(
            "train-base-adapter: pending reimplementation".to_string(),
        ))
    }
}
