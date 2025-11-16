//! Train adapter from codebase command
//!
//! Automatically extracts knowledge from a repository and trains a LoRA adapter

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_orchestrator::codebase_ingestion::{CodebaseIngestion, IngestionConfig};
use adapteros_orchestrator::training::TrainingConfig as OrchestratorTrainingConfig;
use adapteros_single_file_adapter::format::WeightGroupConfig;
use clap::Args;
use std::path::PathBuf;
use tracing::{info, warn};

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

    /// Tokenizer path (defaults to models/qwen2.5-7b-mlx/tokenizer.json)
    #[arg(long)]
    pub tokenizer: Option<PathBuf>,

    /// LoRA rank
    #[arg(long, default_value = "16")]
    pub rank: usize,

    /// LoRA alpha scaling factor
    #[arg(long, default_value = "32.0")]
    pub alpha: f32,

    /// Learning rate
    #[arg(long, default_value = "0.0001")]
    pub learning_rate: f32,

    /// Batch size
    #[arg(long, default_value = "8")]
    pub batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value = "3")]
    pub epochs: usize,

    /// Hidden dimension size
    #[arg(long, default_value = "768")]
    pub hidden_dim: usize,

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
}

impl TrainFromCodeArgs {
    /// Execute the train-from-code command
    pub async fn execute(&self) -> Result<()> {
        info!("Starting codebase ingestion and adapter training");
        info!("Repository: {}", self.repo.display());
        info!("Adapter ID: {}", self.adapter_id);

        // Validate inputs
        self.validate()?;

        // Build ingestion configuration
        let ingestion_config = self.build_config()?;

        // Create ingestion pipeline
        let ingestion = CodebaseIngestion::new(ingestion_config)?;

        // Run the full ingestion and training pipeline
        info!("Starting ingestion pipeline...");
        let result = ingestion
            .ingest_and_train(&self.repo, &self.adapter_id, &self.output)
            .await?;

        // Print results
        println!("\n=== Codebase Ingestion Complete ===");
        println!("Adapter ID: {}", result.adapter_id);
        println!("Adapter Hash (BLAKE3): {}", result.adapter_hash);
        println!("Content Hash: {}", result.content_hash);
        println!("Repository: {}", result.repo_path);
        if let Some(ref commit_sha) = result.commit_sha {
            println!("Commit SHA: {}", commit_sha);
        }
        println!("Symbols Extracted: {}", result.symbols_count);
        println!("Training Examples: {}", result.examples_count);
        println!("Final Loss: {:.6}", result.final_loss);
        println!("Training Time: {}ms", result.training_time_ms);
        println!("Output: {}", self.output.join(&self.adapter_id).display());

        // Register in database if requested
        if self.register {
            self.register_adapter(&result).await?;
        }

        info!("Codebase ingestion and training completed successfully");
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

        if self.rank == 0 {
            return Err(AosError::Validation(
                "Rank must be greater than zero".to_string(),
            ));
        }

        if self.epochs == 0 {
            return Err(AosError::Validation(
                "Epochs must be greater than zero".to_string(),
            ));
        }

        if self.learning_rate <= 0.0 {
            return Err(AosError::Validation(
                "Learning rate must be greater than zero".to_string(),
            ));
        }

        Ok(())
    }

    /// Build ingestion configuration from CLI arguments
    fn build_config(&self) -> Result<IngestionConfig> {
        let training_config = adapteros_lora_worker::training::TrainingConfig {
            rank: self.rank,
            alpha: self.alpha,
            learning_rate: self.learning_rate,
            batch_size: self.batch_size,
            epochs: self.epochs,
            hidden_dim: self.hidden_dim,
            weight_group_config: WeightGroupConfig::default(),
        };

        Ok(IngestionConfig {
            training_config,
            tokenizer_path: self.tokenizer.clone(),
            max_pairs_per_symbol: self.max_pairs_per_symbol,
            include_private: self.include_private,
            min_doc_length: self.min_doc_length,
            generate_negative_examples: self.generate_negative,
            base_model: self.base_model.clone(),
        })
    }

    /// Register the trained adapter in the database
    async fn register_adapter(
        &self,
        result: &adapteros_orchestrator::codebase_ingestion::IngestionResult,
    ) -> Result<()> {
        if let Some(ref db_path) = self.db_path {
            info!("Registering adapter in database: {}", db_path.display());

            let db = Db::new(&db_path.to_string_lossy()).await.map_err(|e| {
                AosError::Database(format!("Failed to open database: {}", e))
            })?;

            // Build registration parameters
            let params = adapteros_db::AdapterRegistrationBuilder::new()
                .adapter_id(&result.adapter_id)
                .name(format!("codebase_{}", self.adapter_id))
                .hash_b3(&result.adapter_hash)
                .rank(self.rank as i32)
                .tier(self.tier)
                .category(&self.category)
                .scope(&self.scope)
                .repo_id(Some(&result.repo_path))
                .commit_sha(result.commit_sha.as_deref())
                .intent(Some("auto-generated from codebase ingestion"))
                .build()
                .map_err(|e| AosError::Database(format!("Failed to build registration params: {}", e)))?;

            db.register_adapter_extended(params)
                .await
                .map_err(|e| AosError::Database(format!("Failed to register adapter: {}", e)))?;

            info!("Successfully registered adapter: {}", result.adapter_id);
            println!("\nAdapter registered in database: {}", db_path.display());
        } else {
            warn!("Skipping registration: no database path provided");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_validate() {
        let temp_dir = TempDir::new().unwrap();

        // Valid arguments
        let args = TrainFromCodeArgs {
            repo: temp_dir.path().to_path_buf(),
            adapter_id: "test".to_string(),
            output: PathBuf::from("./adapters"),
            tokenizer: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
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
        };

        assert!(args.validate().is_ok());

        // Invalid: register without db_path
        let invalid_args = TrainFromCodeArgs {
            register: true,
            ..args
        };

        assert!(invalid_args.validate().is_err());
    }

    #[tokio::test]
    async fn test_build_config() {
        let temp_dir = TempDir::new().unwrap();

        let args = TrainFromCodeArgs {
            repo: temp_dir.path().to_path_buf(),
            adapter_id: "test".to_string(),
            output: PathBuf::from("./adapters"),
            tokenizer: None,
            rank: 8,
            alpha: 32.0,
            learning_rate: 0.001,
            batch_size: 16,
            epochs: 5,
            hidden_dim: 1024,
            max_pairs_per_symbol: 5,
            include_private: true,
            min_doc_length: 50,
            generate_negative: false,
            base_model: "custom-model".to_string(),
            register: false,
            db_path: None,
            tenant_id: "default".to_string(),
            tier: 2,
            category: "code".to_string(),
            scope: "codebase".to_string(),
        };

        let config = args.build_config().unwrap();

        assert_eq!(config.training_config.rank, 8);
        assert_eq!(config.training_config.alpha, 32.0);
        assert_eq!(config.training_config.learning_rate, 0.001);
        assert_eq!(config.max_pairs_per_symbol, 5);
        assert_eq!(config.include_private, true);
        assert_eq!(config.min_doc_length, 50);
        assert_eq!(config.generate_negative_examples, false);
        assert_eq!(config.base_model, "custom-model");
    }

    #[test]
    fn test_validation_errors() {
        let temp_dir = TempDir::new().unwrap();

        // Test zero rank
        let args = TrainFromCodeArgs {
            repo: temp_dir.path().to_path_buf(),
            adapter_id: "test".to_string(),
            output: PathBuf::from("./adapters"),
            tokenizer: None,
            rank: 0,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
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
        };

        let err = args.validate().unwrap_err();
        assert!(err.to_string().contains("Rank must be greater than zero"));
    }
}
