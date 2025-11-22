//! Train base adapter command implementation for CLI
//!
//! TODO: Migrate to adapteros-aos v3.0 types
//! This module is temporarily stubbed pending migration from the deleted
//! adapteros-single-file-adapter crate.
//!
//! Loads the curated dataset manifest, runs the deterministic Micro-LoRA trainer,
//! and packages quantized weights into `adapters/<adapter_id>/`.

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

    /// Qwen tokenizer JSON file
    #[arg(long, default_value = "models/qwen2.5-7b-mlx/tokenizer.json")]
    pub tokenizer: PathBuf,

    /// Output adapters directory
    #[arg(long, default_value = "adapters")]
    pub output_dir: PathBuf,

    /// Output format: directory or aos
    #[arg(long, default_value = "directory")]
    pub output_format: String,

    /// Adapter ID (used for packaged directory name)
    #[arg(long, default_value = "code_lang_v1")]
    pub adapter_id: String,

    /// LoRA rank (MasterPlan Layer 2 default = 16)
    #[arg(long, default_value_t = 16)]
    pub rank: usize,

    /// LoRA alpha scaling factor (MasterPlan Layer 2 default = 32.0)
    #[arg(long, default_value_t = 32.0)]
    pub alpha: f32,

    /// Learning rate for deterministic trainer
    #[arg(long, default_value_t = 5e-4)]
    pub learning_rate: f32,

    /// Batch size for training
    #[arg(long, default_value_t = 8)]
    pub batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value_t = 4)]
    pub epochs: usize,

    /// Hidden dimension (Qwen2.5-7B = 3584)
    #[arg(long, default_value_t = 3584)]
    pub hidden_dim: usize,
}

impl TrainBaseAdapterArgs {
    /// Execute the train-base-adapter command
    ///
    /// TODO: Migrate to adapteros-aos v3.0 types
    pub async fn execute(&self) -> Result<()> {
        tracing::warn!(
            "train-base-adapter command is temporarily disabled pending migration to v3.0 types"
        );

        // TODO: Migrate to adapteros-aos v3.0 types
        // The original implementation used:
        // - adapteros_single_file_adapter::format::{AdapterWeights, LineageInfo, WeightGroup, ...}
        // - adapteros_single_file_adapter::{SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig}
        //
        // These need to be replaced with types from adapteros-aos v3.0

        Err(AosError::Config(
            "train-base-adapter: pending migration to adapteros-aos v3.0 types".to_string()
        ))
    }
}
