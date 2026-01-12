//! `aosctl adapter train-from-code` implementation
//!
//! # CLI Inputs Aligned with Repo Commit Overrides
//!
//! The `scope_overrides` field provides CLI arguments that align with
//! `CodebaseScopeMetadata` in the orchestrator, allowing users to override
//! auto-detected git metadata for deterministic training.

use crate::commands::adapter::validate_adapter_id;
use crate::commands::adapter_codebase::CodebaseScopeOverrides;
use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use crate::output::OutputWriter;
use adapteros_config::resolve_tokenizer_path;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    DeterminismConfig as TrainingDeterminismConfig, TrainingConfig,
};
use adapteros_orchestrator::code_ingestion::{
    CodeDatasetConfig, CodeIngestionPipeline, CodeIngestionRequest, CodeIngestionSource,
};

use clap::Args;
use std::path::{Path, PathBuf};

/// Train an adapter directly from a repository
#[derive(Debug, Clone, Args)]
pub struct TrainFromCodeArgs {
    /// Repository path or git URL
    #[arg(long)]
    pub repo: String,

    /// Adapter ID (defaults to code.<repo>.<commit>)
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Logical project name for metadata
    #[arg(long)]
    pub project_name: Option<String>,

    /// Registry repo identifier override
    #[arg(long)]
    pub repo_id: Option<String>,

    /// Output directory for `.aos` artifacts
    #[arg(long, default_value = "./adapters")]
    pub output_dir: PathBuf,

    /// Base model name for metadata
    #[arg(long, default_value = "qwen2.5-7b")]
    pub base_model: String,

    /// Maximum number of symbols to sample per repo
    #[arg(long, default_value_t = 64)]
    pub max_symbols: usize,

    /// Include private symbols in dataset
    #[arg(long)]
    pub include_private: bool,

    /// Positive sample weight
    #[arg(long, default_value_t = 1.0)]
    pub positive_weight: f32,

    /// Abstention sample weight (must be non-negative; sample_role metadata classifies)
    #[arg(long, default_value_t = 0.5)]
    pub negative_weight: f32,

    /// Skip registry registration
    #[arg(long)]
    pub skip_register: bool,

    /// Registry tier (integer)
    #[arg(long, default_value_t = 1)]
    pub tier: i32,

    /// Enable deterministic training
    #[arg(long)]
    pub deterministic: bool,

    /// Deterministic seed override
    #[arg(long)]
    pub seed: Option<u64>,

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

pub async fn run(args: &TrainFromCodeArgs, output: &OutputWriter) -> Result<()> {
    if let Some(adapter_id) = &args.adapter_id {
        validate_adapter_id(adapter_id)?;
    }

    if args.max_symbols == 0 {
        return Err(AosError::Validation(
            "--max-symbols must be greater than zero".to_string(),
        ));
    }

    if args.tier <= 0 {
        return Err(AosError::Validation(
            "--tier must be a positive integer".to_string(),
        ));
    }

    if (args.positive_weight - 0.0).abs() < f32::EPSILON {
        return Err(AosError::Validation(
            "--positive-weight cannot be zero".to_string(),
        ));
    }

    if args.negative_weight < 0.0 {
        return Err(AosError::Validation(
            "--negative-weight must be >= 0.0. Sample classification uses sample_role metadata, not weight sign.".to_string(),
        ));
    }

    // Log any scope overrides for debugging/audit trail
    args.scope_overrides.log_overrides();

    // Validate common training arguments
    args.common.validate()?;

    // Display configuration
    output.info("Codebase ingestion configuration:");
    output.kv("Repository", &args.repo);
    if let Some(adapter_id) = &args.adapter_id {
        output.kv("Adapter ID", adapter_id);
    } else {
        output.kv("Adapter ID", "(auto-generated: code.<slug>.<commit>)");
    }
    output.kv("Max symbols", &args.max_symbols.to_string());
    output.kv("Include private", &args.include_private.to_string());
    if let Some(seed) = args.seed {
        output.kv("Seed", &seed.to_string());
    }
    if args.deterministic {
        output.kv("Deterministic mode", "enabled");
    }

    // Resolve tokenizer path
    let tokenizer_path = resolve_tokenizer_path(args.tokenizer_arg.tokenizer.as_ref())?;
    output.kv("Tokenizer", &tokenizer_path.display().to_string());
    let tokenizer = QwenTokenizer::from_file(&tokenizer_path)?;
    let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
        AosError::Validation("Tokenizer missing pad_token_id for code training".to_string())
    })?;
    let vocab_size = tokenizer.vocab_size(true);
    let ignore_index = i32::try_from(pad_token_id)
        .map_err(|_| AosError::Validation("pad_token_id exceeds i32 range".to_string()))?;

    // Resolve repository source
    let source = resolve_repo_source(&args.repo)?;

    // Build training config from common args
    let mut training_config = TrainingConfig {
        rank: args.common.rank,
        alpha: args.common.alpha,
        learning_rate: args.common.learning_rate,
        batch_size: args.common.batch_size,
        epochs: args.common.epochs,
        hidden_dim: args.common.hidden_dim,
        vocab_size,
        pad_token_id,
        ignore_index,
        ..TrainingConfig::default()
    };

    // Apply determinism settings
    if args.deterministic || args.seed.is_some() {
        training_config.determinism = Some(TrainingDeterminismConfig {
            seed: args.seed,
            ..Default::default()
        });
    }

    // Build dataset config
    let dataset_config = CodeDatasetConfig {
        max_symbols: args.max_symbols,
        include_private: args.include_private,
        positive_weight: args.positive_weight,
        negative_weight: args.negative_weight,
    };

    // Build scope metadata from CLI overrides
    let scope_metadata = if args.scope_overrides.has_overrides() {
        Some(args.scope_overrides.to_scope_metadata())
    } else {
        None
    };

    // Build the ingestion request
    let request = CodeIngestionRequest {
        source,
        tokenizer_path,
        training_config,
        dataset: dataset_config,
        output_dir: args.output_dir.clone(),
        adapter_id: args.adapter_id.clone(),
        base_model: args.base_model.clone(),
        register: !args.skip_register,
        tier: args.tier,
        repo_id: args.repo_id.clone(),
        project_name: args.project_name.clone(),
        seed: args.seed,
        determinism_config: None,
        session_name: None,
        session_tags: None,
        session_id: None,
        repo_scope: None,
        scan_roots: Vec::new(),
        stream: None,
        scope_metadata,
        lineage: None,
        adapter_scope: None,
        repo_slug: None,
    };

    // Run the pipeline
    output.info("Starting codebase ingestion pipeline...");
    let pipeline = CodeIngestionPipeline::new();
    let result = pipeline.run(request).await?;

    // Display results
    output.success("Codebase ingestion completed");
    output.kv("Adapter ID", &result.adapter_id);
    output.kv(
        "Repo",
        &format!("{} ({})", result.repo_name, result.repo_slug),
    );
    output.kv("Repo identifier", &result.repo_identifier);
    if let Some(branch) = &result.branch {
        output.kv("Branch", branch);
    } else {
        output.kv("Branch", "(detached)");
    }
    output.kv("Commit", &result.commit_sha);
    output.kv("Dataset hash", &result.dataset_hash);
    output.kv("Examples", &result.dataset_examples.to_string());
    output.kv("AOS path", &result.aos_path.display().to_string());
    output.kv("AOS hash", &result.aos_hash_b3);
    if let Some(registry_id) = &result.registry_id {
        output.kv("Registry ID", registry_id);
    }

    Ok(())
}

fn resolve_repo_source(repo: &str) -> Result<CodeIngestionSource> {
    let path_candidate = Path::new(repo);
    if path_candidate.exists() {
        let absolute = std::fs::canonicalize(path_candidate).map_err(|e| {
            AosError::Io(format!("Failed to canonicalize repo path {}: {}", repo, e))
        })?;
        Ok(CodeIngestionSource::LocalPath(absolute))
    } else {
        Ok(CodeIngestionSource::GitUrl(repo.to_string()))
    }
}
