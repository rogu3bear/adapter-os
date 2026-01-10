//! Train adapter from codebase command
//!
//! Automatically extracts knowledge from a repository and trains a LoRA adapter
//!
//! # CLI Inputs Aligned with Repo Commit Overrides
//!
//! The `scope_overrides` field provides CLI arguments that align with
//! `CodebaseScopeMetadata` in the orchestrator, allowing users to override
//! auto-detected git metadata for deterministic training.

use crate::commands::adapter_codebase::CodebaseScopeOverrides;
use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::TrainingConfig;
use adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig};
use tracing::info;

use clap::Args;
use std::path::PathBuf;

/// Train a LoRA adapter from a codebase
#[derive(Args, Debug)]
pub struct TrainFromCodeArgs {
    /// Repository path to ingest
    #[arg(short, long)]
    pub repo: PathBuf,

    /// Adapter ID to create
    #[arg(short, long)]
    pub adapter_id: String,

    /// Output directory for packaged adapter
    #[arg(short, long, default_value = "./adapters")]
    pub output: PathBuf,

    /// Maximum Q&A pairs to generate per symbol
    #[arg(long, default_value = "3")]
    pub max_pairs_per_symbol: usize,

    /// Include private symbols (default: only public APIs)
    #[arg(long)]
    pub include_private: bool,

    /// Minimum documentation length to generate Q&A pairs
    #[arg(long, default_value = "20")]
    pub min_doc_length: usize,

    /// Generate negative examples for abstention training
    #[arg(long, default_value = "true")]
    pub generate_negative: bool,

    /// Base model identifier
    #[arg(long, default_value = "qwen2.5-7b")]
    pub base_model: String,

    /// Register adapter in database after training
    #[arg(long)]
    pub register: bool,

    /// Database path (required if --register is used)
    #[arg(long)]
    pub db_path: Option<PathBuf>,

    /// Tenant ID for registration (default: "default")
    #[arg(long, default_value = "default")]
    pub tenant_id: String,

    /// Adapter tier for registration
    #[arg(long, default_value = "2")]
    pub tier: i32,

    /// Adapter category for registration
    #[arg(long, default_value = "code")]
    pub category: String,

    /// Adapter scope for registration
    #[arg(long, default_value = "codebase")]
    pub scope: String,

    /// Tokenizer configuration
    #[command(flatten)]
    pub tokenizer_arg: TokenizerArg,

    /// Common training hyperparameters
    #[command(flatten)]
    pub common: CommonTrainingArgs,

    /// Codebase scope overrides for repo metadata
    ///
    /// These flags allow overriding auto-detected git metadata (repo name,
    /// branch, commit SHA, scan root, remote URL) for deterministic training.
    /// Aligned with CodebaseScopeMetadata in adapteros-orchestrator.
    #[command(flatten)]
    pub scope_overrides: CodebaseScopeOverrides,
}

impl TrainFromCodeArgs {
    /// Execute the train-from-code command
    pub async fn execute(&self) -> Result<()> {
        // Validate arguments first
        self.validate()?;

        // Log scope overrides if any are set
        self.scope_overrides.log_overrides();

        info!("=== Train from Code Pipeline ===");
        info!("Repository: {}", self.repo.display());
        info!("Adapter ID: {}", self.adapter_id);
        info!("Output: {}", self.output.display());

        // Resolve tokenizer path
        let tokenizer_path =
            adapteros_config::resolve_tokenizer_path(self.tokenizer_arg.tokenizer.as_ref())?;
        info!("Tokenizer: {}", tokenizer_path.display());
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path)?;
        let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
            AosError::Validation("Tokenizer missing pad_token_id for code training".to_string())
        })?;
        let vocab_size = tokenizer.vocab_size(true);
        let ignore_index = i32::try_from(pad_token_id)
            .map_err(|_| AosError::Validation("pad_token_id exceeds i32 range".to_string()))?;

        // Build training config from common args
        let training_config = TrainingConfig {
            rank: self.common.rank,
            alpha: self.common.alpha,
            learning_rate: self.common.learning_rate,
            batch_size: self.common.batch_size,
            epochs: self.common.epochs,
            hidden_dim: self.common.hidden_dim,
            vocab_size,
            pad_token_id,
            ignore_index,
            ..TrainingConfig::default()
        };

        // The orchestrator derives its own deterministic seed from content hash,
        // commit SHA, etc. No explicit seed configuration needed here.

        // Build ingestion config
        let ingestion_config = IngestionConfig {
            training_config,
            tokenizer_path: Some(tokenizer_path),
            max_pairs_per_symbol: self.max_pairs_per_symbol,
            include_private: self.include_private,
            min_doc_length: self.min_doc_length,
            generate_negative_examples: self.generate_negative,
            base_model: self.base_model.clone(),
        };

        // Create and run the ingestion pipeline
        let pipeline = CodebaseIngestion::new(ingestion_config)?;

        // Ensure output directory exists
        std::fs::create_dir_all(&self.output).map_err(|e| {
            AosError::Io(format!(
                "Failed to create output directory {}: {}",
                self.output.display(),
                e
            ))
        })?;

        let result = pipeline
            .ingest_and_train(&self.repo, &self.adapter_id, &self.output)
            .await?;

        info!("=== Training Complete ===");
        info!("Adapter ID: {}", result.adapter_id);
        info!("Adapter hash: {}", result.adapter_hash);
        info!("Symbols extracted: {}", result.symbols_count);
        info!("Training examples: {}", result.examples_count);
        info!("Final loss: {:.6}", result.final_loss);
        info!("Training time: {}ms", result.training_time_ms);
        info!("Content hash: {}", result.content_hash);

        // Register adapter if requested
        if self.register {
            let db_path = self.db_path.as_ref().ok_or_else(|| {
                AosError::Validation("--register requires --db-path to be specified".to_string())
            })?;

            info!("Registering adapter to database: {}", db_path.display());
            let db = Db::connect(&db_path.to_string_lossy()).await?;

            // Build the adapter path
            let aos_path = self
                .output
                .join("default")
                .join(format!("{}.aos", self.adapter_id));

            let register_request = crate::commands::register_adapter::RegisterAosRequest {
                adapter_id: self.adapter_id.clone(),
                aos_path,
                tenant_id: self.tenant_id.clone(),
                base_model_id: self.base_model.clone(),
                tier: self.tier.to_string(),
                rank: self.common.rank as u32,
                name: Some(format!("Codebase adapter: {}", self.adapter_id)),
                revision: result.commit_sha.clone(),
            };

            crate::commands::register_adapter::register_aos_with_db(&db, register_request).await?;
            info!("Adapter registered successfully");
        }

        info!("=== Train from Code Complete ===");
        Ok(())
    }

    /// Validate command arguments
    fn validate(&self) -> Result<()> {
        if !self.repo.exists() {
            return Err(AosError::NotFound(format!(
                "Repository path does not exist: {}",
                self.repo.display()
            )));
        }

        if !self.repo.is_dir() {
            return Err(AosError::Validation(format!(
                "Repository path is not a directory: {}",
                self.repo.display()
            )));
        }

        if self.register && self.db_path.is_none() {
            return Err(AosError::Validation(
                "--register requires --db-path to be specified".to_string(),
            ));
        }

        // Validate common training args
        self.common.validate()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[tokio::test]
    async fn test_validate() {
        let temp_dir = new_test_tempdir();

        // Valid arguments
        let args = TrainFromCodeArgs {
            repo: temp_dir.path().to_path_buf(),
            adapter_id: "test".to_string(),
            output: PathBuf::from("./adapters"),
            max_pairs_per_symbol: 3,
            include_private: false,
            min_doc_length: 20,
            generate_negative: true,
            base_model: "qwen2.5-7b".to_string(),
            register: false,
            db_path: None,
            tenant_id: "default".to_string(),
            tier: 2,
            category: "code".to_string(),
            scope: "codebase".to_string(),
            tokenizer_arg: TokenizerArg { tokenizer: None },
            common: CommonTrainingArgs {
                rank: 4,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
            scope_overrides: CodebaseScopeOverrides::default(),
        };

        assert!(args.validate().is_ok());

        // Invalid: register without db_path
        let invalid_args = TrainFromCodeArgs {
            register: true,
            ..args
        };

        assert!(invalid_args.validate().is_err());
    }

    #[test]
    fn test_validation_errors() {
        let temp_dir = new_test_tempdir();

        // Test zero rank
        let args = TrainFromCodeArgs {
            repo: temp_dir.path().to_path_buf(),
            adapter_id: "test".to_string(),
            output: PathBuf::from("./adapters"),
            max_pairs_per_symbol: 3,
            include_private: false,
            min_doc_length: 20,
            generate_negative: true,
            base_model: "qwen2.5-7b".to_string(),
            register: false,
            db_path: None,
            tenant_id: "default".to_string(),
            tier: 2,
            category: "code".to_string(),
            scope: "codebase".to_string(),
            tokenizer_arg: TokenizerArg { tokenizer: None },
            common: CommonTrainingArgs {
                rank: 0,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
            scope_overrides: CodebaseScopeOverrides::default(),
        };

        let err = args.validate().unwrap_err();
        assert!(err.to_string().contains("rank must be greater than zero"));
    }
}
