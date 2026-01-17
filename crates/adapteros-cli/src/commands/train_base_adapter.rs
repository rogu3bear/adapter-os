//! Train base adapter command implementation for CLI
//!
//! Loads the curated dataset manifest, runs the deterministic Micro-LoRA trainer,
//! and packages quantized weights into `adapters/<adapter_id>/`.

use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    load_examples_from_manifest, AdapterPackager, DeterminismConfig, LoRAQuantizer,
    MicroLoRATrainer, TrainingConfig,
};

use chrono::Utc;
use clap::Args;
use std::fs;
use std::path::PathBuf;
use tracing::info;

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
        info!("=== Train Base Adapter Pipeline ===");

        // Validate common training arguments
        self.common.validate()?;

        // Validate manifest exists
        if !self.manifest.exists() {
            return Err(AosError::Validation(format!(
                "Dataset manifest not found: {}",
                self.manifest.display()
            )));
        }

        // Resolve tokenizer path (validates existence)
        let tokenizer_path =
            adapteros_config::resolve_tokenizer_path(self.tokenizer_arg.tokenizer.as_ref())?;
        info!("Using tokenizer: {}", tokenizer_path.display());

        // Step 1: Load tokenizer
        info!("Step 1/4: Loading tokenizer...");
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path)?;
        let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
            AosError::Validation(
                "Tokenizer missing pad_token_id for base adapter training".to_string(),
            )
        })?;
        let vocab_size = tokenizer.vocab_size(true);
        let ignore_index = i32::try_from(pad_token_id)
            .map_err(|_| AosError::Validation("pad_token_id exceeds i32 range".to_string()))?;

        // Step 2: Load manifest and convert to TrainingExample vec
        info!("Step 2/4: Loading dataset manifest...");
        let examples = load_examples_from_manifest(&self.manifest, &tokenizer)?;
        info!(
            "Loaded {} training examples from manifest: {}",
            examples.len(),
            self.manifest.display()
        );

        if examples.is_empty() {
            return Err(AosError::Validation(
                "No training examples found in manifest".to_string(),
            ));
        }

        // Step 3: Create TrainingConfig from CommonTrainingArgs and train
        info!("Step 3/4: Training LoRA adapter...");
        let train_config = TrainingConfig {
            rank: self.common.rank,
            alpha: self.common.alpha,
            learning_rate: self.common.learning_rate,
            batch_size: self.common.batch_size,
            epochs: self.common.epochs,
            hidden_dim: self.common.hidden_dim,
            vocab_size,
            pad_token_id,
            ignore_index,
            determinism: Some(DeterminismConfig::default()),
            ..TrainingConfig::default()
        };

        let mut trainer = MicroLoRATrainer::new(train_config.clone())?;
        let result = trainer.train(&examples).await?;

        info!(
            "Training complete: loss={:.4}, time={}ms",
            result.final_loss,
            result.training_time_ms()
        );

        // Create output directory
        fs::create_dir_all(&self.output_dir)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        // Step 4: Quantize and package
        info!("Step 4/4: Quantizing and packaging adapter...");
        let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);

        // Determine base model ID (use a sensible default based on hidden_dim)
        let base_model_id = match self.common.hidden_dim {
            3584 => "Qwen2.5-7B-Instruct",
            4096 => "Qwen2.5-14B-Instruct",
            _ => "Qwen2.5-7B-Instruct",
        };

        // Package based on --output-format
        match self.output_format.as_str() {
            "aos" => {
                let packager = AdapterPackager::new(&self.output_dir);
                let packaged = packager
                    .package_aos_for_tenant(
                        "default",
                        &self.adapter_id,
                        &quantized,
                        &train_config,
                        base_model_id,
                    )
                    .await?;

                info!(
                    "Created .aos archive: {} ({} bytes)",
                    packaged.weights_path.display(),
                    fs::metadata(&packaged.weights_path)
                        .map(|m| m.len())
                        .unwrap_or(0)
                );
                info!("Adapter ID: {}", self.adapter_id);
            }
            _ => {
                // Save adapter weights (JSON format for compatibility)
                let weights_path = self
                    .output_dir
                    .join(&self.adapter_id)
                    .join("lora_weights.json");
                let adapter_dir = weights_path.parent().unwrap();
                fs::create_dir_all(adapter_dir).map_err(|e| {
                    AosError::Io(format!("Failed to create adapter directory: {}", e))
                })?;

                let weights_json = serde_json::to_string_pretty(&result.weights)?;
                fs::write(&weights_path, &weights_json)?;

                // Save metadata
                let metadata = serde_json::json!({
                    "adapter_id": self.adapter_id,
                    "final_loss": result.final_loss,
                    "training_time_ms": result.training_time_ms(),
                    "example_count": examples.len(),
                    "config": {
                        "rank": self.common.rank,
                        "alpha": self.common.alpha,
                        "learning_rate": self.common.learning_rate,
                        "epochs": self.common.epochs,
                        "hidden_dim": self.common.hidden_dim,
                    },
                    "created_at": Utc::now().to_rfc3339(),
                });
                let metadata_path = adapter_dir.join("metadata.json");
                fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

                info!("Saved adapter to: {}", adapter_dir.display());
                info!("  Weights: {}", weights_path.display());
                info!("  Metadata: {}", metadata_path.display());
            }
        }

        info!("=== Training Pipeline Complete ===");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::new().expect("create temp dir")
    }

    #[derive(Debug, Parser)]
    struct TrainBaseAdapterTestCmd {
        #[command(flatten)]
        args: TrainBaseAdapterArgs,
    }

    fn parse_args(args: &[&str]) -> TrainBaseAdapterArgs {
        let mut argv = vec!["train-base-adapter-test".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        TrainBaseAdapterTestCmd::try_parse_from(argv)
            .expect("cli args should parse")
            .args
    }

    #[test]
    fn test_default_args_parse() {
        let args = parse_args(&[]);
        assert_eq!(
            args.manifest,
            PathBuf::from("training/datasets/base/code/adapteros/manifest.json")
        );
        assert_eq!(args.output_dir, PathBuf::from("adapters"));
        assert_eq!(args.output_format, "directory");
        assert_eq!(args.adapter_id, "code_lang_v1");
        assert_eq!(args.common.rank, 16);
        assert_eq!(args.common.alpha, 32.0);
    }

    #[test]
    fn test_custom_args_parse() {
        let args = parse_args(&[
            "--manifest",
            "custom/manifest.json",
            "--output-dir",
            "custom/output",
            "--output-format",
            "aos",
            "--adapter-id",
            "my_adapter_v2",
            "--rank",
            "8",
            "--alpha",
            "16.0",
            "--epochs",
            "5",
        ]);
        assert_eq!(args.manifest, PathBuf::from("custom/manifest.json"));
        assert_eq!(args.output_dir, PathBuf::from("custom/output"));
        assert_eq!(args.output_format, "aos");
        assert_eq!(args.adapter_id, "my_adapter_v2");
        assert_eq!(args.common.rank, 8);
        assert_eq!(args.common.alpha, 16.0);
        assert_eq!(args.common.epochs, 5);
    }

    #[test]
    fn test_validation_rejects_missing_manifest() {
        let temp_dir = new_test_tempdir();
        let args = TrainBaseAdapterArgs {
            manifest: temp_dir.path().join("nonexistent/manifest.json"),
            output_dir: temp_dir.path().to_path_buf(),
            output_format: "directory".to_string(),
            adapter_id: "test_adapter".to_string(),
            tokenizer_arg: TokenizerArg { tokenizer: None },
            common: CommonTrainingArgs {
                rank: 16,
                alpha: 32.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        // Execute should fail due to missing manifest
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(args.execute());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("manifest not found") || err_msg.contains("not found"),
            "Expected manifest not found error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_common_args_validation_is_invoked() {
        let temp_dir = new_test_tempdir();
        let manifest_path = temp_dir.path().join("manifest.json");

        // Create a dummy manifest file so it passes the file existence check
        fs::write(&manifest_path, r#"{"name": "test", "entries": []}"#).unwrap();

        let args = TrainBaseAdapterArgs {
            manifest: manifest_path,
            output_dir: temp_dir.path().to_path_buf(),
            output_format: "directory".to_string(),
            adapter_id: "test_adapter".to_string(),
            tokenizer_arg: TokenizerArg { tokenizer: None },
            common: CommonTrainingArgs {
                rank: 0, // Invalid: rank cannot be 0
                alpha: 32.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        // Execute should fail due to invalid rank
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(args.execute());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("rank") && err_msg.contains("greater than zero"),
            "Expected rank validation error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_output_format_values() {
        // Test "directory" format
        let args_dir = parse_args(&["--output-format", "directory"]);
        assert_eq!(args_dir.output_format, "directory");

        // Test "aos" format
        let args_aos = parse_args(&["--output-format", "aos"]);
        assert_eq!(args_aos.output_format, "aos");
    }
}
