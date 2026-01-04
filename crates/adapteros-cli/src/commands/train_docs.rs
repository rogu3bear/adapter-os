//! Train adapter on documentation files
//!
//! Full end-to-end pipeline: ingest docs -> generate training data -> train LoRA -> register adapter
//! The trained adapter is automatically registered and set for owner chat.

use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_config::resolve_base_model_location;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_ingest_docs::{
    generate_training_data_from_documents, load_tokenizer, ChunkingOptions, DocumentIngestor,
    TrainingGenConfig, TrainingStrategy,
};
use adapteros_lora_worker::training::{
    AdapterPackager, DeterminismConfig, LoRAQuantizer, MicroLoRATrainer, TrainingConfig,
    TrainingExample,
};
use clap::{ArgGroup, Args};
use glob::glob;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Train adapter on documentation markdown files
#[derive(Args, Debug)]
#[command(
    group(
        ArgGroup::new("register_mode")
            .args(&["scenario", "tenant_id"])
            .multiple(false)
            .required(false)
    )
)]
pub struct TrainDocsArgs {
    /// Docs directory to scan for markdown files
    #[arg(long, default_value = "./docs")]
    docs_dir: PathBuf,

    /// Output directory for trained adapter
    ///
    /// Defaults to `${AOS_ADAPTERS_DIR}/docs-assistant` (or `var/adapters/docs-assistant`
    /// when the env var is not set).
    #[arg(long)]
    output: Option<PathBuf>,

    /// Version/revision for the adapter (defaults to timestamp)
    #[arg(long)]
    revision: Option<String>,

    /// Adapter ID to use (otherwise auto-generated)
    #[arg(long)]
    adapter_id: Option<String>,

    /// Scenario profile name (configs/scenarios/<NAME>.toml)
    #[arg(
        long,
        group = "register_mode",
        conflicts_with_all = ["tenant_id", "base_model_id"]
    )]
    scenario: Option<String>,

    /// Tenant ID (explicit mode)
    #[arg(long, group = "register_mode", conflicts_with = "scenario")]
    tenant_id: Option<String>,

    /// Base model ID (explicit mode)
    #[arg(long, conflicts_with = "scenario")]
    base_model_id: Option<String>,

    /// Register the trained adapter (requires scenario or explicit tenant+model)
    #[arg(long)]
    register: bool,

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

    /// Dry run - show what would be done without executing
    #[arg(long)]
    dry_run: bool,

    /// Database path (for registration)
    #[arg(long, env = "DATABASE_URL")]
    db_url: Option<String>,

    /// Skip training (only generate data)
    #[arg(long)]
    skip_training: bool,

    /// Training strategy: identity or qa
    #[arg(long, default_value = "identity")]
    training_strategy: String,

    /// Enable deterministic training
    #[arg(long)]
    deterministic: bool,

    /// Training seed (for deterministic training)
    #[arg(long)]
    seed: Option<u64>,

    /// Resume from latest checkpoint if available
    #[arg(long)]
    resume: bool,

    /// Tokenizer configuration
    #[command(flatten)]
    tokenizer_arg: TokenizerArg,

    /// Common training hyperparameters
    #[command(flatten)]
    common: CommonTrainingArgs,
}

#[derive(Debug, Deserialize)]
struct ScenarioConfig {
    tenant: Option<ScenarioTenant>,
    model: Option<ScenarioModel>,
}

#[derive(Debug, Deserialize)]
struct ScenarioTenant {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScenarioModel {
    id: Option<String>,
}

#[derive(Debug, Clone)]
struct RegistrationContext {
    tenant_id: String,
    base_model_id: String,
}

#[derive(Debug)]
enum RegistrationMode<'a> {
    Scenario(&'a str),
    Explicit {
        tenant_id: &'a str,
        base_model_id: &'a str,
    },
}

const REGISTER_SHAPE_ERROR: &str =
    "--register requires either --scenario or both --tenant-id and --base-model-id";
const BASE_MODEL_REQUIRED_ERROR: &str = "--base-model-id is required when --scenario is not set";

impl TrainDocsArgs {
    /// Resolve output path, honoring env override when no CLI override is provided.
    fn resolved_output_dir(&self) -> PathBuf {
        self.output.clone().unwrap_or_else(Self::default_output_dir)
    }

    fn default_output_dir() -> PathBuf {
        adapteros_core::paths::get_default_adapters_root().join("docs-assistant")
    }

    fn validate_register_shape(&self) -> Result<Option<RegistrationMode<'_>>> {
        if !self.register {
            return Ok(None);
        }

        if let Some(name) = self.scenario.as_deref() {
            return Ok(Some(RegistrationMode::Scenario(name)));
        }

        match (self.tenant_id.as_deref(), self.base_model_id.as_deref()) {
            (Some(tenant_id), Some(base_model_id)) => Ok(Some(RegistrationMode::Explicit {
                tenant_id,
                base_model_id,
            })),
            _ => Err(AosError::Validation(REGISTER_SHAPE_ERROR.to_string())),
        }
    }

    fn resolve_registration_context(&self) -> Result<Option<RegistrationContext>> {
        match self.validate_register_shape()? {
            None => Ok(None),
            Some(RegistrationMode::Scenario(name)) => {
                let resolved = Self::load_scenario_config(name)?;
                Ok(Some(resolved))
            }
            Some(RegistrationMode::Explicit {
                tenant_id,
                base_model_id,
            }) => Ok(Some(RegistrationContext {
                tenant_id: tenant_id.to_string(),
                base_model_id: base_model_id.to_string(),
            })),
        }
    }

    fn resolve_base_model_id(
        &self,
        registration_ctx: Option<&RegistrationContext>,
    ) -> Result<String> {
        if let Some(ctx) = registration_ctx {
            return Ok(ctx.base_model_id.clone());
        }

        if let Some(id) = self.base_model_id.clone() {
            return Ok(id);
        }

        // Fall back to canonical resolver to avoid drift when not provided explicitly
        resolve_base_model_location(None, None, false)
            .map(|loc| loc.id)
            .map_err(|e| AosError::Validation(format!("{} ({})", BASE_MODEL_REQUIRED_ERROR, e)))
    }

    fn load_scenario_config(name: &str) -> Result<RegistrationContext> {
        let path = Path::new("configs")
            .join("scenarios")
            .join(format!("{}.toml", name));

        if !path.exists() {
            return Err(AosError::Validation(format!(
                "Scenario file not found: {}",
                path.display()
            )));
        }

        let contents = fs::read_to_string(&path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read scenario file {}: {}",
                path.display(),
                e
            ))
        })?;

        let parsed: ScenarioConfig = toml::from_str(&contents).map_err(|e| {
            AosError::Validation(format!("Failed to parse scenario '{}': {}", name, e))
        })?;

        let tenant_id = parsed.tenant.and_then(|t| t.id).ok_or_else(|| {
            AosError::Validation(format!(
                "Scenario '{}' is missing tenant.id or model.id; cannot use with --register",
                name
            ))
        })?;

        let base_model_id = parsed.model.and_then(|m| m.id).ok_or_else(|| {
            AosError::Validation(format!(
                "Scenario '{}' is missing tenant.id or model.id; cannot use with --register",
                name
            ))
        })?;

        Ok(RegistrationContext {
            tenant_id,
            base_model_id,
        })
    }

    pub async fn execute(&self) -> Result<()> {
        info!("=== Documentation Training Pipeline ===");

        // Resolve registration context first to surface errors early
        let registration_ctx = self.resolve_registration_context()?;
        let base_model_id = self.resolve_base_model_id(registration_ctx.as_ref())?;
        let base_model_location = resolve_base_model_location(Some(&base_model_id), None, true)
            .map_err(|e| AosError::Validation(e.to_string()))?;

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
        info!(
            "Using base model '{}' at {}",
            base_model_location.id,
            base_model_location.full_path.display()
        );

        // Generate revision
        let revision = self
            .revision
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());
        let adapter_id = self
            .adapter_id
            .clone()
            .unwrap_or_else(|| format!("system/docs/adapteros/{}", revision));
        let output_dir = self.resolved_output_dir();
        let tenant_for_path = registration_ctx
            .as_ref()
            .map(|ctx| ctx.tenant_id.as_str())
            .or(self.tenant_id.as_deref())
            .unwrap_or("default");

        if let Some(ctx) = registration_ctx.as_ref() {
            info!(
                "Training scenario adapter {}/{} for base model {}",
                ctx.tenant_id, adapter_id, base_model_id
            );
        }

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
            info!("Output: {}", output_dir.display());
            info!(
                "Training config: rank={}, alpha={}, epochs={}",
                self.common.rank, self.common.alpha, self.common.epochs
            );
            return Ok(());
        }

        // Resolve tokenizer path (validates existence)
        let tokenizer_path =
            adapteros_config::resolve_tokenizer_path(self.tokenizer_arg.tokenizer.as_ref())?;

        // === Step 1: Ingest Documents ===
        info!("Step 1/4: Ingesting documents...");
        let tokenizer = load_tokenizer(&tokenizer_path)?;
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

        // Parse training strategy from CLI flag
        let strategy = match self.training_strategy.to_lowercase().as_str() {
            "identity" => TrainingStrategy::Identity,
            "qa" | "question-answer" => TrainingStrategy::QuestionAnswer,
            _ => {
                return Err(AosError::Validation(format!(
                    "Invalid training strategy: '{}'. Must be one of: identity, qa",
                    self.training_strategy
                )));
            }
        };
        info!("Using training strategy: {:?}", strategy);

        let gen_config = TrainingGenConfig {
            strategy,
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
        fs::create_dir_all(&output_dir)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        if self.skip_training {
            // Just save training data
            let data_path = output_dir.join("training_data.json");
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
            rank: self.common.rank,
            alpha: self.common.alpha,
            learning_rate: self.common.learning_rate,
            batch_size: self.common.batch_size,
            epochs: self.common.epochs,
            hidden_dim: self.common.hidden_dim,
            determinism: if self.deterministic || self.seed.is_some() {
                Some(DeterminismConfig {
                    seed: self.seed,
                    ..Default::default()
                })
            } else {
                None
            },
            ..TrainingConfig::default()
        };

        let mut trainer = MicroLoRATrainer::new(train_config.clone())?;

        // Enable checkpointing for resume support
        trainer.enable_checkpointing(&output_dir, &adapter_id, 5);

        // Check for checkpoint availability
        let checkpoint_exists = trainer.try_resume_from_checkpoint().await.is_some();

        // Train with optional resume
        let (result, resumed_from_epoch) = if self.resume {
            if let Some((epoch, _weights, loss)) = trainer.try_resume_from_checkpoint().await {
                info!("Resuming from epoch {} with loss {:.4}", epoch, loss);
                let result = trainer.train_with_resume(&examples, |_| {}).await?;
                (result, Some(epoch))
            } else {
                info!("No checkpoint found, starting fresh training");
                let result = trainer.train(&examples).await?;
                (result, None)
            }
        } else {
            let result = trainer.train(&examples).await?;
            (result, None)
        };

        info!(
            "Training complete: loss={:.4}, time={}ms, checkpoint_available={}, resumed_from_epoch={}",
            result.final_loss,
            result.training_time_ms(),
            checkpoint_exists,
            resumed_from_epoch.unwrap_or(0)
        );

        // Save adapter weights (JSON format for compatibility)
        let weights_path = output_dir.join("lora_weights.json");
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
                "rank": self.common.rank,
                "alpha": self.common.alpha,
                "learning_rate": self.common.learning_rate,
                "epochs": self.common.epochs,
                "hidden_dim": self.common.hidden_dim,
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let metadata_path = output_dir.join("metadata.json");
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        info!("Saved adapter to {}", output_dir.display());

        // Package to .aos archive format
        info!("Packaging adapter to .aos format...");
        let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
        let packager = AdapterPackager::new(&output_dir);
        let safe_adapter_id = adapter_id.replace('/', "_");
        let packaged = packager
            .package_aos_for_tenant(
                tenant_for_path,
                &safe_adapter_id,
                &quantized,
                &train_config,
                &base_model_id,
            )
            .await?;
        info!(
            "Created .aos archive: {} ({} bytes)",
            packaged.weights_path.display(),
            fs::metadata(&packaged.weights_path)
                .map(|m| m.len())
                .unwrap_or(0)
        );

        // === Step 4: Register and Activate (optional) ===
        if self.register {
            info!("Step 4/4: Registering adapter...");
            let ctx = registration_ctx
                .as_ref()
                .expect("registration context must be present when --register is set");
            let db = self.connect_db().await?;

            let register_request = crate::commands::register_adapter::RegisterAosRequest {
                adapter_id: adapter_id.clone(),
                aos_path: packaged.weights_path.clone(),
                tenant_id: ctx.tenant_id.clone(),
                base_model_id: base_model_id.clone(),
                tier: "warm".to_string(),
                rank: self.common.rank as u32,
                name: Some("Documentation Assistant".to_string()),
                revision: Some(revision.clone()),
            };

            crate::commands::register_adapter::register_aos_with_db(&db, register_request).await?;

            // Optional activation path
            if self.auto_activate {
                db.set_system_setting("owner_chat_adapter_id", &adapter_id)
                    .await?;
                info!("Activated adapter for owner chat");
            }

            info!("=== Training + Registration Complete ===");
            info!("Adapter ID: {}", adapter_id);
            info!("AOS: {}", packaged.weights_path.display());
            info!("Status: Registered for inference");
        } else {
            info!("=== Training Pipeline Complete (not registered) ===");
            info!("Adapter ID: {}", adapter_id);
            info!("AOS: {}", packaged.weights_path.display());
            info!("Status: Artifact ready; registration skipped");
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_config::DEFAULT_BASE_MODEL_ID;
    use adapteros_core::paths::{AOS_ADAPTERS_DIR_ENV, DEFAULT_ADAPTERS_DIR};
    use adapteros_platform::common::PlatformUtils;
    use clap::Parser;
    use serial_test::serial;
    use std::path::PathBuf;

    #[derive(Debug, Parser)]
    struct TrainDocsTestCmd {
        #[command(flatten)]
        args: TrainDocsArgs,
    }

    fn parse_args(args: &[&str]) -> TrainDocsArgs {
        let mut argv = vec!["train-docs-test".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        TrainDocsTestCmd::try_parse_from(argv)
            .expect("cli args should parse")
            .args
    }

    fn new_test_tempdir() -> tempfile::TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        tempfile::tempdir_in(&root).expect("tempdir")
    }

    #[test]
    #[serial]
    fn default_output_respects_env() {
        let tmp = new_test_tempdir();
        std::env::remove_var(adapteros_core::paths::AOS_ADAPTERS_ROOT_ENV);
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, tmp.path());

        let resolved = TrainDocsArgs::default_output_dir();
        assert!(
            resolved.starts_with(tmp.path()),
            "expected {} to start with {}",
            resolved.display(),
            tmp.path().display()
        );

        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    #[serial]
    fn default_output_falls_back_to_var() {
        std::env::remove_var(adapteros_core::paths::AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let resolved = TrainDocsArgs::default_output_dir();
        assert_eq!(
            resolved,
            PathBuf::from(DEFAULT_ADAPTERS_DIR).join("docs-assistant")
        );
    }

    #[test]
    fn register_with_scenario_is_allowed_shape() {
        let args = parse_args(&["--register", "--scenario", "docs"]);
        let mode = args
            .validate_register_shape()
            .expect("shape should validate")
            .expect("registration mode expected");

        match mode {
            RegistrationMode::Scenario(name) => assert_eq!(name, "docs"),
            other => panic!("unexpected mode: {:?}", other),
        }
    }

    #[test]
    fn register_with_explicit_ids_is_allowed_shape() {
        let args = parse_args(&["--register", "--tenant-id", "t1", "--base-model-id", "m1"]);
        let mode = args
            .validate_register_shape()
            .expect("shape should validate")
            .expect("registration mode expected");

        match mode {
            RegistrationMode::Explicit {
                tenant_id,
                base_model_id,
            } => {
                assert_eq!(tenant_id, "t1");
                assert_eq!(base_model_id, "m1");
            }
            other => panic!("unexpected mode: {:?}", other),
        }
    }

    #[test]
    fn register_alone_is_rejected_with_stable_message() {
        let args = parse_args(&["--register"]);
        let err = args
            .validate_register_shape()
            .expect_err("should reject missing context");

        match err {
            AosError::Validation(msg) => assert_eq!(msg, REGISTER_SHAPE_ERROR),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn register_with_tenant_only_is_rejected_with_stable_message() {
        let args = parse_args(&["--register", "--tenant-id", "t-only"]);
        let err = args
            .validate_register_shape()
            .expect_err("should reject incomplete explicit shape");

        match err {
            AosError::Validation(msg) => assert_eq!(msg, REGISTER_SHAPE_ERROR),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn base_model_defaults_from_resolver_when_missing() {
        let args = parse_args(&[]);
        let resolved = args
            .resolve_base_model_id(None)
            .expect("default base model should resolve");

        assert_eq!(resolved, DEFAULT_BASE_MODEL_ID);
    }

    #[test]
    fn base_model_is_taken_from_cli_when_set() {
        let args = parse_args(&["--base-model-id", "m-cli"]);
        let resolved = args
            .resolve_base_model_id(None)
            .expect("cli base model should resolve");
        assert_eq!(resolved, "m-cli");
    }
}
