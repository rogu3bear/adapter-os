//! `aosctl adapter train-from-code` implementation

use crate::commands::adapter::validate_adapter_id;
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::TrainingConfig;
use adapteros_orchestrator::code_ingestion::{
    CodeDatasetConfig, CodeIngestionPipeline, CodeIngestionRequest, CodeIngestionSource,
};
use clap::Args;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tracing::info;

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

    /// Tokenizer JSON path
    #[arg(long, default_value = "models/qwen2.5-7b-mlx/tokenizer.json")]
    pub tokenizer: PathBuf,

    /// Output directory for `.aos` artifacts
    #[arg(long, default_value = "./adapters")]
    pub output_dir: PathBuf,

    /// Base model name for metadata
    #[arg(long, default_value = "qwen2.5-7b")]
    pub base_model: String,

    /// LoRA rank
    #[arg(long, default_value_t = 16)]
    pub rank: usize,

    /// LoRA alpha
    #[arg(long, default_value_t = 32.0)]
    pub alpha: f32,

    /// Learning rate
    #[arg(long, default_value_t = 1e-4)]
    pub learning_rate: f32,

    /// Batch size
    #[arg(long, default_value_t = 8)]
    pub batch_size: usize,

    /// Epochs
    #[arg(long, default_value_t = 3)]
    pub epochs: usize,

    /// Hidden dimension
    #[arg(long, default_value_t = 3584)]
    pub hidden_dim: usize,

    /// Maximum number of symbols to sample per repo
    #[arg(long, default_value_t = 64)]
    pub max_symbols: usize,

    /// Include private symbols in dataset
    #[arg(long)]
    pub include_private: bool,

    /// Positive sample weight
    #[arg(long, default_value_t = 1.0)]
    pub positive_weight: f32,

    /// Negative sample weight for abstention pairs
    #[arg(long, default_value_t = -0.5)]
    pub negative_weight: f32,

    /// Skip registry registration
    #[arg(long)]
    pub skip_register: bool,

    /// Registry tier (integer)
    #[arg(long, default_value_t = 1)]
    pub tier: i32,

    /// Deterministic seed override
    #[arg(long)]
    pub seed: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TrainFromCodeOutput {
    adapter_id: String,
    repo_name: String,
    commit_sha: String,
    short_commit_sha: String,
    dataset_examples: usize,
    dataset_positive_examples: usize,
    dataset_negative_examples: usize,
    dataset_hash: String,
    aos_path: String,
    aos_hash_b3: String,
    registry_id: Option<String>,
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

    if args.negative_weight >= 0.0 {
        output.warning("--negative-weight is non-negative; abstention training may be ineffective");
    }

    let source = resolve_repo_source(&args.repo)?;
    let training_config = TrainingConfig {
        rank: args.rank,
        alpha: args.alpha,
        learning_rate: args.learning_rate,
        batch_size: args.batch_size,
        epochs: args.epochs,
        hidden_dim: args.hidden_dim,
        weight_group_config: adapteros_single_file_adapter::format::WeightGroupConfig::default(),
    };

    let dataset_cfg = CodeDatasetConfig {
        max_symbols: args.max_symbols,
        include_private: args.include_private,
        positive_weight: args.positive_weight,
        negative_weight: args.negative_weight,
    };

    let request = CodeIngestionRequest {
        source,
        tokenizer_path: args.tokenizer.clone(),
        training_config,
        dataset: dataset_cfg,
        output_dir: args.output_dir.clone(),
        adapter_id: args.adapter_id.clone(),
        base_model: args.base_model.clone(),
        register: !args.skip_register,
        tier: args.tier,
        repo_id: args.repo_id.clone(),
        project_name: args.project_name.clone(),
        seed: args.seed,
    };

    output.section("Training");
    output.info("Extracting code graph and building dataset...");

    let pipeline = CodeIngestionPipeline::new();
    let result = pipeline.run(request).await?;

    output.success("Adapter trained from repository");
    output.kv("Adapter", &result.adapter_id);
    output.kv("Repository", &result.repo_name);
    output.kv("Commit", &result.commit_sha);
    output.kv("Samples", &result.dataset_examples.to_string());
    output.kv("AOS", &result.aos_path.display().to_string());
    output.kv("AOS Hash", &result.aos_hash_b3);
    if let Some(registry_id) = &result.registry_id {
        output.kv("Registry ID", registry_id);
    } else if !args.skip_register {
        output.warning("Adapter not registered (database unavailable?)");
    }

    if output.is_json() {
        let json = TrainFromCodeOutput {
            adapter_id: result.adapter_id.clone(),
            repo_name: result.repo_name.clone(),
            commit_sha: result.commit_sha.clone(),
            short_commit_sha: result.short_commit_sha.clone(),
            dataset_examples: result.dataset_examples,
            dataset_positive_examples: result.positive_examples,
            dataset_negative_examples: result.negative_examples,
            dataset_hash: result.dataset_hash.clone(),
            aos_path: result.aos_path.display().to_string(),
            aos_hash_b3: result.aos_hash_b3.clone(),
            registry_id: result.registry_id.clone(),
        };
        output
            .json(&json)
            .map_err(|e| AosError::Io(format!("Failed to emit JSON: {}", e)))?;
    }

    info!(adapter = %result.adapter_id, "Code ingestion pipeline finished");
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
