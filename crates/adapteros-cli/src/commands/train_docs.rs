//! Train adapter on documentation files
//!
//! Full end-to-end pipeline: ingest docs -> generate training data -> train LoRA -> register adapter
//! The trained adapter is automatically registered and set for owner chat.

use adapteros_core::{AosError, Result};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use adapteros_ingest_docs::{
    generate_training_data_from_documents, load_tokenizer, ChunkingOptions, DocumentIngestor,
    TrainingGenConfig, TrainingStrategy,
};
use adapteros_lora_worker::training::{MicroLoRATrainer, TrainingConfig, TrainingExample};
use clap::Args;
use glob::glob;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// Train adapter on documentation markdown files
#[derive(Args, Debug)]
pub struct TrainDocsArgs {
    /// Docs directory to scan for markdown files
    #[arg(long, default_value = "./docs")]
    docs_dir: PathBuf,

    /// Tokenizer path (required for processing)
    #[arg(long, default_value = "models/qwen2.5-7b-mlx/tokenizer.json")]
    tokenizer: PathBuf,

    /// Output directory for trained adapter
    #[arg(long, default_value = "./adapters/docs-assistant")]
    output: PathBuf,

    /// Version/revision for the adapter (defaults to timestamp)
    #[arg(long)]
    revision: Option<String>,

    /// Automatically activate adapter for owner chat
    #[arg(long, default_value = "true")]
    auto_activate: bool,

    /// Maximum sequence length for training examples
    #[arg(long, default_value = "512")]
    max_seq_length: usize,

    /// Chunk size in tokens
    #[arg(long, default_value = "512")]
    chunk_tokens: usize,

    /// Overlap size in tokens
    #[arg(long, default_value = "128")]
    overlap_tokens: usize,

    /// LoRA rank
    #[arg(long, default_value = "8")]
    rank: usize,

    /// LoRA alpha scaling factor
    #[arg(long, default_value = "16.0")]
    alpha: f32,

    /// Learning rate
    #[arg(long, default_value = "0.0001")]
    learning_rate: f32,

    /// Batch size
    #[arg(long, default_value = "4")]
    batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value = "3")]
    epochs: usize,

    /// Hidden dimension
    #[arg(long, default_value = "768")]
    hidden_dim: usize,

    /// Dry run - show what would be done without executing
    #[arg(long)]
    dry_run: bool,

    /// Database path (for registration)
    #[arg(long, env = "DATABASE_URL")]
    db_url: Option<String>,

    /// Skip training (only generate data)
    #[arg(long)]
    skip_training: bool,
}

impl TrainDocsArgs {
    pub async fn execute(&self) -> Result<()> {
        info!("=== Documentation Training Pipeline ===");

        // Validate docs directory
        if !self.docs_dir.exists() {
            return Err(AosError::Validation(format!(
                "Docs directory not found: {}",
                self.docs_dir.display()
            )));
        }

        // Discover markdown files
        let doc_paths = self.discover_docs()?;
        if doc_paths.is_empty() {
            return Err(AosError::Validation(
                "No markdown files found in docs directory".to_string(),
            ));
        }

        info!("Found {} markdown files to process", doc_paths.len());

        // Generate revision
        let revision = self
            .revision
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());
        let adapter_id = format!("system/docs/adapteros/{}", revision);

        // Dry run mode
        if self.dry_run {
            info!("=== DRY RUN MODE ===");
            info!("Would train on {} documents", doc_paths.len());
            for path in doc_paths.iter().take(10) {
                info!("  - {}", path.display());
            }
            if doc_paths.len() > 10 {
                info!("  ... and {} more", doc_paths.len() - 10);
            }
            info!("Adapter ID: {}", adapter_id);
            info!("Output: {}", self.output.display());
            info!(
                "Training config: rank={}, alpha={}, epochs={}",
                self.rank, self.alpha, self.epochs
            );
            return Ok(());
        }

        // Verify tokenizer
        if !self.tokenizer.exists() {
            return Err(AosError::Validation(format!(
                "Tokenizer not found: {}. Run 'aosctl import-model' first.",
                self.tokenizer.display()
            )));
        }

        // === Step 1: Ingest Documents ===
        info!("Step 1/4: Ingesting documents...");
        let tokenizer = load_tokenizer(&self.tokenizer)?;
        let chunking_options = ChunkingOptions {
            chunk_tokens: self.chunk_tokens,
            overlap_tokens: self.overlap_tokens,
            min_chunk_chars: 160,
        };
        let ingestor = DocumentIngestor::new(chunking_options, Some(tokenizer.clone()));

        let mut ingested_docs = Vec::new();
        let mut failed_count = 0;
        for path in &doc_paths {
            match ingestor.ingest_markdown_path(path) {
                Ok(doc) => {
                    debug!(
                        "Ingested: {} ({} chunks)",
                        doc.source_name,
                        doc.chunks.len()
                    );
                    ingested_docs.push(doc);
                }
                Err(e) => {
                    warn!("Skipping {}: {}", path.display(), e);
                    failed_count += 1;
                }
            }
        }

        if ingested_docs.is_empty() {
            return Err(AosError::Validation(
                "No documents were ingested".to_string(),
            ));
        }

        let total_chunks: usize = ingested_docs.iter().map(|d| d.chunks.len()).sum();
        info!(
            "Ingested {} documents with {} chunks ({} failed)",
            ingested_docs.len(),
            total_chunks,
            failed_count
        );

        // === Step 2: Generate Training Data ===
        info!("Step 2/4: Generating training data...");
        let gen_config = TrainingGenConfig {
            strategy: TrainingStrategy::QuestionAnswer,
            max_seq_length: self.max_seq_length,
            add_special_tokens: true,
        };

        let training_data =
            generate_training_data_from_documents(&ingested_docs, &tokenizer, &gen_config)?;
        info!(
            "Generated {} training examples",
            training_data.examples.len()
        );

        if training_data.examples.is_empty() {
            return Err(AosError::Validation(
                "No training examples generated".to_string(),
            ));
        }

        // Convert to worker TrainingExample format
        let examples: Vec<TrainingExample> = training_data
            .examples
            .into_iter()
            .map(|ex| TrainingExample {
                input: ex.input,
                target: ex.target,
                metadata: ex.metadata.unwrap_or_default(),
                weight: 1.0,
            })
            .collect();

        // Create output directory
        fs::create_dir_all(&self.output)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        if self.skip_training {
            // Just save training data
            let data_path = self.output.join("training_data.json");
            let data_json = serde_json::json!({
                "examples": examples.iter().map(|ex| {
                    serde_json::json!({
                        "input": ex.input,
                        "target": ex.target,
                        "metadata": ex.metadata,
                    })
                }).collect::<Vec<_>>()
            });
            fs::write(&data_path, serde_json::to_string_pretty(&data_json)?)?;
            info!(
                "Saved training data to {} (training skipped)",
                data_path.display()
            );
            return Ok(());
        }

        // === Step 3: Train LoRA Adapter ===
        info!("Step 3/4: Training LoRA adapter...");
        let train_config = TrainingConfig {
            rank: self.rank,
            alpha: self.alpha,
            learning_rate: self.learning_rate,
            batch_size: self.batch_size,
            epochs: self.epochs,
            hidden_dim: self.hidden_dim,
            ..TrainingConfig::default()
        };

        let mut trainer = MicroLoRATrainer::new(train_config)?;
        let result = trainer.train(&examples).await?;

        info!(
            "Training complete: loss={:.4}, time={}ms",
            result.final_loss,
            result.training_time_ms()
        );

        // Save adapter weights
        let weights_path = self.output.join("lora_weights.json");
        let weights_json = serde_json::to_string_pretty(&result.weights)?;
        fs::write(&weights_path, &weights_json)?;

        // Save metadata
        let metadata = serde_json::json!({
            "adapter_id": adapter_id,
            "revision": revision,
            "final_loss": result.final_loss,
            "training_time_ms": result.training_time_ms(),
            "doc_count": ingested_docs.len(),
            "example_count": examples.len(),
            "config": {
                "rank": self.rank,
                "alpha": self.alpha,
                "learning_rate": self.learning_rate,
                "epochs": self.epochs,
                "hidden_dim": self.hidden_dim,
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let metadata_path = self.output.join("metadata.json");
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        info!("Saved adapter to {}", self.output.display());

        // === Step 4: Register and Activate ===
        info!("Step 4/4: Registering adapter...");
        let db = self.connect_db().await?;

        // Compute weights hash
        let weights_hash = format!("b3:{}", blake3::hash(weights_json.as_bytes()).to_hex());

        // Register the adapter
        // Valid categories: code, framework, codebase, ephemeral
        // Valid scopes: global, tenant, repo, commit
        let params = AdapterRegistrationBuilder::new()
            .tenant_id("default")
            .adapter_id(&adapter_id)
            .name("Documentation Assistant")
            .hash_b3(&weights_hash)
            .rank(self.rank as i32)
            .tier("warm")
            .category("codebase")
            .scope("global")
            .domain(Some("docs"))
            .purpose(Some("owner-chat"))
            .revision(Some(revision.clone()))
            .build()?;

        let db_id = db.register_adapter(params).await?;
        info!("Registered adapter: {} (db_id={})", adapter_id, db_id);

        // Update current_state to 'warm' so it's usable for inference
        // Note: current_state holds the load state (unloaded/cold/warm/hot/resident)
        // while lifecycle_state holds business state (draft/active/deprecated/retired)
        db.update_adapter_state(&adapter_id, "warm", "trained from docs")
            .await?;
        info!("Set adapter current_state to 'warm'");

        // Auto-activate for owner chat
        if self.auto_activate {
            db.set_system_setting("owner_chat_adapter_id", &adapter_id)
                .await?;
            info!("Activated adapter for owner chat");
        }

        info!("=== Training Pipeline Complete ===");
        info!("Adapter ID: {}", adapter_id);
        info!("Weights: {}", weights_path.display());
        info!("Status: Ready for inference");

        Ok(())
    }

    /// Discover all markdown files in the docs directory
    fn discover_docs(&self) -> Result<Vec<PathBuf>> {
        let pattern = format!("{}/**/*.md", self.docs_dir.display());
        let paths: Vec<PathBuf> = glob(&pattern)
            .map_err(|e| AosError::Io(format!("Invalid glob pattern: {}", e)))?
            .filter_map(|r| r.ok())
            .filter(|p| {
                let path_str = p.to_string_lossy().to_lowercase();
                !path_str.contains("/archive/")
                    && !path_str.contains("/.git/")
                    && !path_str.contains("/node_modules/")
            })
            .collect();

        debug!("Discovered {} markdown files", paths.len());
        Ok(paths)
    }

    /// Connect to the database
    async fn connect_db(&self) -> Result<Db> {
        let db_url = self.db_url.clone().unwrap_or_else(|| {
            std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:var/aos-cp.sqlite3".to_string())
        });
        Db::connect(&db_url).await
    }
}
