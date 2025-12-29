//! Train adapter from codebase command
//!
//! This module is temporarily stubbed pending migration from the deleted
//! adapteros-single-file-adapter crate.
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
// Removed: use adapteros_db::Db;
// Removed: use adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig};
// Removed: use adapteros_orchestrator::training::TrainingConfig as OrchestratorTrainingConfig;
// Removed: use adapteros_single_file_adapter::format::WeightGroupConfig;

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
        tracing::warn!(
            "train-from-code command is temporarily disabled pending crate migration"
        );

        // The original implementation used:
        // - adapteros_single_file_adapter::format::WeightGroupConfig
        // - adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig}
        //
        // These need to be replaced with types from adapteros-aos

        Err(AosError::Config(
            "train-from-code: pending crate migration".to_string()
        ))
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
