//! `aosctl adapter train-from-code` implementation
//!
//! This module is temporarily stubbed pending migration from the deleted
//! adapteros-single-file-adapter crate.
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
use adapteros_core::{AosError, Result};
// Removed: use adapteros_lora_worker::training::TrainingConfig;
// Removed: use adapteros_orchestrator::code_ingestion::{...};
// Removed: use adapteros_single_file_adapter::format::WeightGroupConfig;

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

    /// Negative sample weight for abstention pairs
    #[arg(long, default_value_t = -0.5)]
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

    if args.negative_weight >= 0.0 {
        output.warning("--negative-weight is non-negative; abstention training may be ineffective");
    }

    // Log any scope overrides for debugging/audit trail
    args.scope_overrides.log_overrides();

    output.warning("adapter train-from-code command is temporarily disabled pending crate migration");

    // The original implementation used:
    // - adapteros_single_file_adapter::format::WeightGroupConfig
    // - adapteros_orchestrator::code_ingestion::{CodeDatasetConfig, CodeIngestionPipeline, ...}
    // - adapteros_lora_worker::training::TrainingConfig
    //
    // These need to be replaced with types from adapteros-aos

    Err(AosError::Config(
        "adapter train-from-code: pending crate migration".to_string()
    ))
}

#[allow(dead_code)]
fn resolve_repo_source(repo: &str) -> Result<RepoSource> {
    let path_candidate = Path::new(repo);
    if path_candidate.exists() {
        let absolute = std::fs::canonicalize(path_candidate).map_err(|e| {
            AosError::Io(format!("Failed to canonicalize repo path {}: {}", repo, e))
        })?;
        Ok(RepoSource::LocalPath(absolute))
    } else {
        Ok(RepoSource::GitUrl(repo.to_string()))
    }
}

/// Stub enum for repo source (pending v3.0 migration)
#[allow(dead_code)]
enum RepoSource {
    LocalPath(PathBuf),
    GitUrl(String),
}
