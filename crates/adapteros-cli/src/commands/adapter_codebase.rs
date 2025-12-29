//! Adapter codebase ingestion CLI commands
//!
//! Provides CLI interface for codebase-to-adapter training pipeline with explicit
//! repository slug configuration. The repo_slug is used for:
//! - Generating deterministic adapter IDs (code.<repo_slug>.<commit>)
//! - Tracking dataset provenance in training samples
//! - Registry repository identification
//!
//! # Alias Update Gating (Set 23, Point 3c)
//!
//! This module also provides gated alias (semantic name) updates. Alias updates
//! are controlled based on the adapter's lifecycle state:
//! - **Draft/Training**: Alias updates are allowed (mutable states)
//! - **Ready**: Alias updates require confirmation (transitional state)
//! - **Active/Deprecated**: Alias updates are blocked (immutable in production)
//! - **Retired/Failed**: Alias updates are blocked (terminal states)

use crate::commands::adapter::validate_adapter_id;
use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use crate::output::OutputWriter;
use adapteros_core::lifecycle::LifecycleState;
use adapteros_core::{AosError, Result};
use adapteros_orchestrator::code_ingestion::{
    CodebaseScopeMetadata, CodeDatasetConfig, CodeIngestionPipeline, CodeIngestionRequest,
    CodeIngestionSource, RepoScopeConfig, StreamConfig, StreamFormat,
};
use std::str::FromStr;
use tracing::{debug, info, warn};
use uuid::Uuid;

use clap::Args;
use std::path::{Path, PathBuf};

/// Arguments for codebase ingestion and adapter training
#[derive(Debug, Clone, Args)]
pub struct CodebaseIngestArgs {
    /// Repository path or git URL
    #[arg(long)]
    pub repo: String,

    /// Repository slug for adapter naming and provenance tracking.
    /// Used to generate adapter IDs in format: code.<repo_slug>.<commit>
    /// If not provided, auto-derived from repository name.
    #[arg(long)]
    pub repo_slug: Option<String>,

    /// Adapter ID override (defaults to code.<repo_slug>.<commit>)
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Logical project name for metadata
    #[arg(long)]
    pub project_name: Option<String>,

    /// Registry repo identifier override
    #[arg(long)]
    pub repo_id: Option<String>,

    /// Git branch to use for ingestion.
    /// If not specified, uses the current branch of the repository.
    #[arg(long)]
    pub branch: Option<String>,

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

    /// Deterministic seed override
    #[arg(long)]
    pub seed: Option<u64>,

    /// Tokenizer configuration
    #[command(flatten)]
    pub tokenizer_arg: TokenizerArg,

    /// Common training hyperparameters
    #[command(flatten)]
    pub common: CommonTrainingArgs,
}

impl CodebaseIngestArgs {
    /// Validate command arguments
    pub fn validate(&self) -> Result<()> {
        if let Some(adapter_id) = &self.adapter_id {
            validate_adapter_id(adapter_id)?;
        }

        if let Some(slug) = &self.repo_slug {
            validate_repo_slug(slug)?;
        }

        if self.max_symbols == 0 {
            return Err(AosError::Validation(
                "--max-symbols must be greater than zero".to_string(),
            ));
        }

        if self.tier <= 0 {
            return Err(AosError::Validation(
                "--tier must be a positive integer".to_string(),
            ));
        }

        if (self.positive_weight - 0.0).abs() < f32::EPSILON {
            return Err(AosError::Validation(
                "--positive-weight cannot be zero".to_string(),
            ));
        }

        Ok(())
    }
}

/// Validate repo_slug format
///
/// Repo slugs must be:
/// - 1-64 characters
/// - Lowercase alphanumeric with underscores
/// - No leading/trailing underscores
/// - No consecutive underscores
pub fn validate_repo_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(AosError::Validation(
            "--repo-slug cannot be empty".to_string(),
        ));
    }

    if slug.len() > 64 {
        return Err(AosError::Validation(
            "--repo-slug cannot exceed 64 characters".to_string(),
        ));
    }

    if slug.starts_with('_') || slug.ends_with('_') {
        return Err(AosError::Validation(
            "--repo-slug cannot start or end with underscore".to_string(),
        ));
    }

    if slug.contains("__") {
        return Err(AosError::Validation(
            "--repo-slug cannot contain consecutive underscores".to_string(),
        ));
    }

    for c in slug.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return Err(AosError::Validation(format!(
                "--repo-slug contains invalid character '{}': must be lowercase alphanumeric or underscore",
                c
            )));
        }
    }

    Ok(())
}

/// Run the codebase ingestion command
pub async fn run(args: &CodebaseIngestArgs, output: &OutputWriter) -> Result<()> {
    args.validate()?;

    if args.negative_weight >= 0.0 {
        output.warning("--negative-weight is non-negative; abstention training may be ineffective");
    }

    // Display configuration
    output.info("Codebase ingestion configuration:");
    output.kv("Repository", &args.repo);
    if let Some(branch) = &args.branch {
        output.kv("Branch", branch);
    } else {
        output.kv("Branch", "(current branch)");
    }
    if let Some(slug) = &args.repo_slug {
        output.kv("Repo slug", slug);
    } else {
        output.kv("Repo slug", "(auto-derived from repo name)");
    }
    if let Some(adapter_id) = &args.adapter_id {
        output.kv("Adapter ID", adapter_id);
    } else {
        output.kv("Adapter ID", "(auto-generated: code.<slug>.<commit>)");
    }
    output.kv("Max symbols", &args.max_symbols.to_string());
    output.kv("Include private", &args.include_private.to_string());

    output.warning("codebase ingest command is temporarily disabled pending orchestrator integration");

    // TODO: Integrate with CodeIngestionPipeline once available
    // The implementation should:
    // 1. Resolve repo source (local path or git URL)
    // 2. Pass repo_slug to CodeIngestionRequest if provided
    // 3. Use auto-derived slug if not provided
    // 4. Pass branch override via scope_metadata.branch if provided
    // 5. Build and run the ingestion pipeline

    Err(AosError::Config(
        "adapter codebase ingest: pending orchestrator integration".to_string(),
    ))
}

/// Resolve repository source from path or URL
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

/// Repository source type
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RepoSource {
    /// Local filesystem path
    LocalPath(PathBuf),
    /// Remote git URL
    GitUrl(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_repo_slug_valid() {
        assert!(validate_repo_slug("myrepo").is_ok());
        assert!(validate_repo_slug("my_repo").is_ok());
        assert!(validate_repo_slug("repo123").is_ok());
        assert!(validate_repo_slug("my_repo_123").is_ok());
        assert!(validate_repo_slug("a").is_ok());
    }

    #[test]
    fn test_validate_repo_slug_empty() {
        let err = validate_repo_slug("").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_repo_slug_too_long() {
        let long_slug = "a".repeat(65);
        let err = validate_repo_slug(&long_slug).unwrap_err();
        assert!(err.to_string().contains("64 characters"));
    }

    #[test]
    fn test_validate_repo_slug_leading_underscore() {
        let err = validate_repo_slug("_myrepo").unwrap_err();
        assert!(err.to_string().contains("start or end with underscore"));
    }

    #[test]
    fn test_validate_repo_slug_trailing_underscore() {
        let err = validate_repo_slug("myrepo_").unwrap_err();
        assert!(err.to_string().contains("start or end with underscore"));
    }

    #[test]
    fn test_validate_repo_slug_consecutive_underscores() {
        let err = validate_repo_slug("my__repo").unwrap_err();
        assert!(err.to_string().contains("consecutive underscores"));
    }

    #[test]
    fn test_validate_repo_slug_uppercase() {
        let err = validate_repo_slug("MyRepo").unwrap_err();
        assert!(err.to_string().contains("invalid character"));
    }

    #[test]
    fn test_validate_repo_slug_special_chars() {
        assert!(validate_repo_slug("my-repo").is_err());
        assert!(validate_repo_slug("my.repo").is_err());
        assert!(validate_repo_slug("my@repo").is_err());
        assert!(validate_repo_slug("my repo").is_err());
    }
}
