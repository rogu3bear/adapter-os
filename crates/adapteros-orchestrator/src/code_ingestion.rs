//! Automated codebase ingestion and adapter training pipeline
//!
//! Provides a deterministic end-to-end workflow that scans a repository,
//! extracts symbol knowledge via adapteros-codegraph, builds Q&A training
//! samples, fine-tunes a Micro-LoRA adapter, packages it into a `.aos`
//! artifact, and optionally registers it in the adapter registry.

use adapteros_codegraph::{CodeGraph, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use adapteros_db::{AdapterRegistrationBuilder, Db};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{MicroLoRATrainer, TrainingConfig, TrainingExample};
use adapteros_platform::common::PlatformUtils;
use blake3::Hasher;
use chrono::{DateTime, TimeZone, Utc};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;
use tokio::task;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Source repository specification for ingestion.
#[derive(Debug, Clone)]
pub enum CodeIngestionSource {
    /// Use a local path (automatically discovers the git root)
    LocalPath(PathBuf),
    /// Clone a remote git repository URL into a temporary workspace
    GitUrl(String),
}

/// Dataset generation tuning parameters
#[derive(Debug, Clone)]
pub struct CodeDatasetConfig {
    /// Maximum number of symbols to sample from the repository
    pub max_symbols: usize,
    /// Include non-public symbols
    pub include_private: bool,
    /// Weight assigned to knowledge samples
    pub positive_weight: f32,
    /// Weight assigned to abstention samples when documentation is missing
    pub negative_weight: f32,
}

impl Default for CodeDatasetConfig {
    fn default() -> Self {
        Self {
            max_symbols: 64,
            include_private: false,
            positive_weight: 1.0,
            negative_weight: -0.5,
        }
    }
}

/// Configuration for filtering repository scope during ingestion.
///
/// Used to selectively include or exclude files based on paths and extensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoScopeConfig {
    /// Paths to include (e.g., ["src/", "lib/"])
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// Paths to exclude (e.g., ["tests/", "vendor/"])
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// File extensions to include (e.g., ["rs", "py"])
    #[serde(default)]
    pub include_extensions: Vec<String>,
    /// File extensions to exclude (e.g., ["md", "txt"])
    #[serde(default)]
    pub exclude_extensions: Vec<String>,
}

impl RepoScopeConfig {
    /// Check if any filters are configured
    pub fn has_filters(&self) -> bool {
        !self.include_paths.is_empty()
            || !self.exclude_paths.is_empty()
            || !self.include_extensions.is_empty()
            || !self.exclude_extensions.is_empty()
    }
}

/// Stream output format for progress events during ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamFormat {
    /// JSON Lines format for machine parsing
    Json,
    /// Human-readable text format
    #[default]
    Text,
}

impl StreamFormat {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" | "jsonl" => Self::Json,
            _ => Self::Text,
        }
    }
}

/// Configuration for streaming progress events during ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Whether streaming is enabled
    pub enabled: bool,
    /// Output format for events
    pub format: StreamFormat,
    /// Minimum interval between events in milliseconds (0 = every event)
    pub interval_ms: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl StreamConfig {
    /// Create a new enabled stream config
    pub fn new(format: StreamFormat, interval_ms: u64) -> Self {
        Self {
            enabled: true,
            format,
            interval_ms,
        }
    }

    /// Create a disabled stream config
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            format: StreamFormat::Text,
            interval_ms: 0,
        }
    }
}

/// Metadata overrides for codebase scope.
///
/// Allows CLI or CI/CD to override auto-detected repository metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodebaseScopeMetadata {
    /// Override repository name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Override branch name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Override commit SHA
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Override scan root path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_root: Option<String>,
    /// Override remote URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

impl CodebaseScopeMetadata {
    /// Check if any overrides are configured
    pub fn has_overrides(&self) -> bool {
        self.repo.is_some()
            || self.branch.is_some()
            || self.commit.is_some()
            || self.scan_root.is_some()
            || self.remote_url.is_some()
    }
}

/// Dataset lineage information for provenance tracking.
///
/// Links adapters to their training data sources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetLineageInfo {
    /// Parent dataset ID for single-parent lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_dataset_id: Option<String>,
    /// Human-readable label for the lineage relationship
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_label: Option<String>,
    /// List of source dataset IDs this was derived from
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    /// Explicit version string
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Additional key-value metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl DatasetLineageInfo {
    /// Check if any lineage information is present
    pub fn has_lineage(&self) -> bool {
        self.parent_dataset_id.is_some()
            || self.lineage_label.is_some()
            || !self.derived_from.is_empty()
            || self.version.is_some()
            || !self.metadata.is_empty()
    }
}

/// Git commit metadata captured during code ingestion.
///
/// Provides full commit provenance including author information, timestamps,
/// and message for traceability and reproducibility of training runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitMetadata {
    /// Full 40-character commit SHA
    pub sha: String,
    /// Short SHA (first 8 characters) for display purposes
    pub short_sha: String,
    /// Commit author name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// Commit author email
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_email: Option<String>,
    /// Commit timestamp in ISO 8601 format (UTC)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_date: Option<String>,
    /// Unix timestamp of the commit (seconds since epoch)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_timestamp: Option<i64>,
    /// First line of the commit message (summary)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_summary: Option<String>,
    /// Full commit message body
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_body: Option<String>,
    /// Committer name (may differ from author in rebased commits)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_name: Option<String>,
    /// Committer email
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_email: Option<String>,
    /// Parent commit SHA(s) - empty for initial commits
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_shas: Vec<String>,
}

impl CommitMetadata {
    /// Create a new CommitMetadata with just the SHA
    pub fn new(sha: String) -> Self {
        let short_sha = sha.get(0..8).unwrap_or(&sha).to_string();
        Self {
            sha,
            short_sha,
            ..Default::default()
        }
    }

    /// Check if full metadata is available (beyond just SHA)
    pub fn has_full_metadata(&self) -> bool {
        self.author_name.is_some() || self.commit_date.is_some() || self.message_summary.is_some()
    }

    /// Convert to a HashMap for metadata storage
    pub fn to_metadata_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("commit_sha".to_string(), self.sha.clone());
        map.insert("commit_short_sha".to_string(), self.short_sha.clone());

        if let Some(ref name) = self.author_name {
            map.insert("commit_author_name".to_string(), name.clone());
        }
        if let Some(ref email) = self.author_email {
            map.insert("commit_author_email".to_string(), email.clone());
        }
        if let Some(ref date) = self.commit_date {
            map.insert("commit_date".to_string(), date.clone());
        }
        if let Some(ts) = self.commit_timestamp {
            map.insert("commit_timestamp".to_string(), ts.to_string());
        }
        if let Some(ref summary) = self.message_summary {
            map.insert("commit_message_summary".to_string(), summary.clone());
        }
        if let Some(ref committer) = self.committer_name {
            map.insert("commit_committer_name".to_string(), committer.clone());
        }
        if !self.parent_shas.is_empty() {
            map.insert("commit_parent_shas".to_string(), self.parent_shas.join(","));
        }

        map
    }
}

/// Request to train an adapter directly from a codebase
#[derive(Debug, Clone)]
pub struct CodeIngestionRequest {
    pub source: CodeIngestionSource,
    pub tokenizer_path: PathBuf,
    pub training_config: TrainingConfig,
    pub dataset: CodeDatasetConfig,
    pub output_dir: PathBuf,
    pub adapter_id: Option<String>,
    pub base_model: String,
    pub register: bool,
    pub tier: i32,
    pub repo_id: Option<String>,
    pub project_name: Option<String>,
    pub seed: Option<u64>,
    /// Human-readable name for the ingestion session (e.g., "nightly-build", "pr-123")
    pub session_name: Option<String>,
    /// Arbitrary tags for categorizing the session (e.g., ["ci", "production"])
    pub session_tags: Option<Vec<String>>,
    /// Unique identifier for this ingestion session, used for correlation across pipeline stages
    pub session_id: Option<Uuid>,
    /// Repository scope filtering configuration (include/exclude paths and extensions)
    pub repo_scope: Option<RepoScopeConfig>,
    /// Streaming configuration for real-time progress updates
    pub stream: Option<StreamConfig>,
    /// Codebase scope metadata (repo, branch, commit, etc.)
    pub scope_metadata: Option<CodebaseScopeMetadata>,
    /// Dataset lineage information for provenance tracking
    pub lineage: Option<DatasetLineageInfo>,
}

/// Result of a code ingestion training run
#[derive(Debug, Clone)]
pub struct CodeIngestionResult {
    pub adapter_id: String,
    pub repo_name: String,
    pub repo_slug: String,
    /// Git branch name at time of ingestion (e.g., "main", "feature/xyz")
    pub branch: Option<String>,
    pub commit_sha: String,
    pub short_commit_sha: String,
    /// Full commit metadata including author, date, and message
    pub commit_metadata: CommitMetadata,
    /// Absolute path to the git repository root
    pub repo_root_path: PathBuf,
    /// Absolute path to the scan root (the directory being scanned)
    pub scan_root_path: PathBuf,
    /// Scan root path relative to repo root (empty string if same as repo root)
    pub scan_root_relative: String,
    pub dataset_examples: usize,
    pub positive_examples: usize,
    pub negative_examples: usize,
    pub dataset_hash: String,
    pub aos_path: PathBuf,
    pub aos_hash_b3: String,
    pub registry_id: Option<String>,
}

/// Primary entry point for the ingestion pipeline
#[derive(Debug, Default, Clone)]
pub struct CodeIngestionPipeline;

impl CodeIngestionPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute ingestion + training end-to-end
    pub async fn run(&self, request: CodeIngestionRequest) -> Result<CodeIngestionResult> {
        let prepared_repo = prepare_repo(&request.source).await?;
        let project_name = request
            .project_name
            .clone()
            .unwrap_or_else(|| prepared_repo.repo_name.clone());

        let adapter_id = request.adapter_id.clone().unwrap_or_else(|| {
            format!(
                "code.{}.{}",
                prepared_repo.repo_slug,
                prepared_repo.short_sha()
            )
        });

        let repo_identifier = normalize_repo_id(
            &request
                .repo_id
                .clone()
                .unwrap_or_else(|| format!("repo:{}", prepared_repo.repo_slug)),
        );

        info!(
            repo_root = %prepared_repo.root.display(),
            scan_root = %prepared_repo.scan_root.display(),
            scan_root_relative = %prepared_repo.scan_root_relative,
            commit = %prepared_repo.commit_sha,
            project = %project_name,
            adapter_id = %adapter_id,
            "Starting code ingestion pipeline",
        );

        // Build CodeGraph from repository
        let codegraph = CodeGraph::from_directory(&prepared_repo.root, None).await?;
        debug!(
            symbol_count = codegraph.symbols.len(),
            "Parsed repository into CodeGraph"
        );

        let samples =
            build_symbol_samples(&codegraph, &prepared_repo, &project_name, &request.dataset);
        if samples.is_empty() {
            return Err(AosError::Training(
                "Code ingestion did not produce any training samples".to_string(),
            ));
        }

        let dataset_hash = compute_dataset_hash(&samples);

        let tokenizer = QwenTokenizer::from_file(&request.tokenizer_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load tokenizer {}: {}",
                request.tokenizer_path.display(),
                e
            ))
        })?;
        let training_examples = encode_samples(&tokenizer, &samples)?;

        let stats = SampleStats::from_examples(&training_examples);
        info!(
            samples = training_examples.len(),
            positives = stats.positive,
            negatives = stats.negative,
            hash = %dataset_hash,
            "Constructed training dataset"
        );

        let mut trainer = MicroLoRATrainer::new(request.training_config.clone())?;
        let seed = request.seed.or_else(|| {
            Some(derive_seed(
                &prepared_repo.commit_sha,
                &dataset_hash,
                &request.training_config,
            ))
        });
        if let Some(seed_value) = seed {
            trainer.override_training_seed(seed_value)?;
            info!(seed = seed_value, "Using deterministic training seed");
        }

        let mut training_result = trainer.train_separated(&training_examples).await?;
        training_result.adapter_id = adapter_id.clone();
        training_result.positive_result.adapter_id = adapter_id.clone();
        training_result.negative_result.adapter_id = adapter_id.clone();

        fs::create_dir_all(&request.output_dir).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create adapter output directory {}: {}",
                request.output_dir.display(),
                e
            ))
        })?;
        let aos_path = request.output_dir.join(format!("{}.aos", adapter_id));
        if fs::try_exists(&aos_path).await.unwrap_or(false) {
            fs::remove_file(&aos_path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove existing {}: {}",
                    aos_path.display(),
                    e
                ))
            })?;
        }

        let mut metadata = HashMap::new();
        metadata.insert("repo_name".to_string(), prepared_repo.repo_name.clone());
        metadata.insert("repo_slug".to_string(), prepared_repo.repo_slug.clone());
        metadata.insert("repo_commit".to_string(), prepared_repo.commit_sha.clone());
        metadata.insert(
            "repo_short_commit".to_string(),
            prepared_repo.short_sha().to_string(),
        );
        metadata.insert(
            "repo_path".to_string(),
            prepared_repo.root.display().to_string(),
        );
        // Record scan root path and its relative path to repo root
        metadata.insert(
            "scan_root_path".to_string(),
            prepared_repo.scan_root.display().to_string(),
        );
        if !prepared_repo.scan_root_relative.is_empty() {
            metadata.insert(
                "scan_root_relative".to_string(),
                prepared_repo.scan_root_relative.clone(),
            );
        }
        if let Some(remote) = &prepared_repo.remote_url {
            metadata.insert("repo_remote".to_string(), remote.clone());
        }
        metadata.insert("dataset_hash".to_string(), dataset_hash.clone());
        metadata.insert(
            "dataset_examples".to_string(),
            training_examples.len().to_string(),
        );
        metadata.insert(
            "dataset_positive_examples".to_string(),
            stats.positive.to_string(),
        );
        metadata.insert(
            "dataset_negative_examples".to_string(),
            stats.negative.to_string(),
        );
        metadata.insert("project".to_string(), project_name.clone());
        metadata.insert(
            "generator".to_string(),
            "code_ingestion_pipeline".to_string(),
        );
        metadata.insert("category".to_string(), "codebase".to_string());

        // Include session context metadata if provided
        if let Some(session_name) = &request.session_name {
            metadata.insert("session_name".to_string(), session_name.clone());
        }
        if let Some(session_tags) = &request.session_tags {
            metadata.insert("session_tags".to_string(), session_tags.join(","));
        }

        trainer
            .save_as_aos_package_with_metadata(&training_result, &aos_path, &metadata)
            .await?;

        let aos_bytes = fs::read(&aos_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read {} for hashing: {}",
                aos_path.display(),
                e
            ))
        })?;
        let aos_hash = blake3::hash(&aos_bytes).to_hex().to_string();

        info!(
            path = %aos_path.display(),
            hash = %aos_hash,
            "Packaged SingleFile adapter"
        );

        let registry_id = if request.register {
            register_adapter(
                &adapter_id,
                &aos_hash,
                request.tier,
                &request.training_config,
                &repo_identifier,
                &prepared_repo.commit_sha,
            )
            .await?
        } else {
            None
        };

        Ok(CodeIngestionResult {
            adapter_id,
            repo_name: prepared_repo.repo_name.clone(),
            repo_slug: prepared_repo.repo_slug.clone(),
            commit_sha: prepared_repo.commit_sha.clone(),
            short_commit_sha: prepared_repo.short_sha().to_string(),
            repo_root_path: prepared_repo.root.clone(),
            scan_root_path: prepared_repo.scan_root.clone(),
            scan_root_relative: prepared_repo.scan_root_relative.clone(),
            dataset_examples: training_examples.len(),
            positive_examples: stats.positive,
            negative_examples: stats.negative,
            dataset_hash,
            aos_path,
            aos_hash_b3: aos_hash,
            registry_id,
        })
    }
}

struct PreparedRepo {
    /// Git repository root (the directory containing `.git`)
    root: PathBuf,
    /// Scan root path (the directory being scanned, may differ from repo root)
    scan_root: PathBuf,
    /// Scan root path relative to the repo root (empty string if same as repo root)
    scan_root_relative: String,
    repo_name: String,
    repo_slug: String,
    /// Current branch name (e.g., "main", "feature/xyz")
    branch: Option<String>,
    commit_sha: String,
    commit_summary: String,
    /// Full commit metadata including author, date, and message
    commit_metadata: CommitMetadata,
    remote_url: Option<String>,
    _temp_dir: Option<TempDir>,
}

impl PreparedRepo {
    fn short_sha(&self) -> &str {
        self.commit_metadata.short_sha.as_str()
    }
}

#[derive(Default)]
struct SymbolSample {
    prompt: String,
    response: String,
    metadata: BTreeMap<String, String>,
    weight: f32,
}

struct SampleStats {
    positive: usize,
    negative: usize,
}

impl From<&[SymbolSample]> for SampleStats {
    fn from(samples: &[SymbolSample]) -> Self {
        let mut positive = 0usize;
        let mut negative = 0usize;
        for sample in samples {
            if sample.weight.is_sign_negative() {
                negative += 1;
            } else {
                positive += 1;
            }
        }
        Self { positive, negative }
    }
}

impl SampleStats {
    fn from_examples(examples: &[TrainingExample]) -> Self {
        let mut stats = SampleStats {
            positive: 0,
            negative: 0,
        };
        for example in examples {
            if example.weight.is_sign_negative() {
                stats.negative += 1;
            } else {
                stats.positive += 1;
            }
        }
        stats
    }
}

fn build_symbol_samples(
    graph: &CodeGraph,
    repo: &PreparedRepo,
    project_name: &str,
    cfg: &CodeDatasetConfig,
) -> Vec<SymbolSample> {
    let mut selected: Vec<&SymbolNode> = graph
        .symbols
        .values()
        .filter(|symbol| should_capture_symbol(symbol, cfg))
        .collect();
    selected.sort_by(|a, b| a.qualified_name().cmp(&b.qualified_name()));
    selected.truncate(cfg.max_symbols);

    let mut samples = Vec::new();
    for symbol in selected {
        let positive = build_positive_sample(symbol, repo, project_name, cfg);
        samples.push(positive);

        if symbol
            .docstring
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            if let Some(negative) = build_negative_sample(symbol, repo, project_name, cfg) {
                samples.push(negative);
            }
        }
    }

    samples
}

fn build_positive_sample(
    symbol: &SymbolNode,
    repo: &PreparedRepo,
    project_name: &str,
    cfg: &CodeDatasetConfig,
) -> SymbolSample {
    let rel_path = relative_path(&repo.root, &symbol.file_path);
    let mut response = format!(
        "`{}` is a {} defined in `{}` (lines {}-{}) inside the {} repository.",
        symbol.qualified_name(),
        symbol_kind_label(&symbol.kind),
        rel_path,
        symbol.span.start_line,
        symbol.span.end_line,
        project_name,
    );

    if let Some(signature) = &symbol.signature {
        response.push_str(&format!(" Signature: {}.", signature.trim()));
    }

    if let Some(type_annotation) = &symbol.type_annotation {
        if let Some(return_type) = &type_annotation.return_type {
            response.push_str(&format!(" Returns `{}`.", return_type));
        }
    }

    if let Some(docstring) = symbol.docstring.as_ref().filter(|s| !s.trim().is_empty()) {
        response.push_str(&format!(
            " Documentation summary: {}",
            sanitize_whitespace(docstring)
        ));
    } else {
        response.push_str(
            " No inline documentation was found, so refer to the source when deeper semantics are required.",
        );
    }

    response.push_str(&format!(
        " Visibility: {}. Language: {}.",
        visibility_label(&symbol.visibility),
        symbol.language
    ));

    let prompt = format!(
        "In the {} project (commit {}), what does the {} `{}` at {} do?",
        project_name,
        repo.short_sha(),
        symbol_kind_label(&symbol.kind),
        symbol.qualified_name(),
        rel_path
    );

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "symbol_kind".to_string(),
        symbol_kind_label(&symbol.kind).to_string(),
    );
    metadata.insert("language".to_string(), symbol.language.to_string());
    metadata.insert("file_path".to_string(), rel_path);
    metadata.insert(
        "docstring_present".to_string(),
        (!symbol
            .docstring
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true))
        .to_string(),
    );
    metadata.insert("sample_role".to_string(), "positive".to_string());
    metadata.insert("project".to_string(), project_name.to_string());

    SymbolSample {
        prompt,
        response,
        metadata,
        weight: cfg.positive_weight,
    }
}

fn build_negative_sample(
    symbol: &SymbolNode,
    repo: &PreparedRepo,
    project_name: &str,
    cfg: &CodeDatasetConfig,
) -> Option<SymbolSample> {
    let rel_path = relative_path(&repo.root, &symbol.file_path);
    let prompt = format!(
        "Explain the undocumented {} `{}` defined in {} (commit {}).",
        symbol_kind_label(&symbol.kind),
        symbol.qualified_name(),
        rel_path,
        repo.short_sha()
    );
    let response = format!(
        "I don't know. `{}` at `{}` in {} lacks documentation, so I won't speculate about its behaviour.",
        symbol.qualified_name(),
        rel_path,
        project_name
    );

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "symbol_kind".to_string(),
        symbol_kind_label(&symbol.kind).to_string(),
    );
    metadata.insert("language".to_string(), symbol.language.to_string());
    metadata.insert("file_path".to_string(), rel_path);
    metadata.insert("sample_role".to_string(), "negative".to_string());
    metadata.insert("reason".to_string(), "missing_docstring".to_string());

    Some(SymbolSample {
        prompt,
        response,
        metadata,
        weight: cfg.negative_weight,
    })
}

fn encode_samples(
    tokenizer: &QwenTokenizer,
    samples: &[SymbolSample],
) -> Result<Vec<TrainingExample>> {
    let mut encoded = Vec::with_capacity(samples.len());
    for sample in samples {
        let input = tokenizer.encode(&sample.prompt)?;
        let target = tokenizer.encode(&sample.response)?;
        if input.is_empty() || target.is_empty() {
            continue;
        }
        let metadata: HashMap<String, String> = sample
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        encoded.push(TrainingExample {
            input,
            target,
            metadata,
            weight: sample.weight,
        });
    }
    if encoded.is_empty() {
        return Err(AosError::Training(
            "No encodable training samples were produced".to_string(),
        ));
    }
    Ok(encoded)
}

fn compute_dataset_hash(samples: &[SymbolSample]) -> String {
    let mut hasher = Hasher::new();
    for sample in samples {
        hasher.update(sample.prompt.as_bytes());
        hasher.update(sample.response.as_bytes());
        hasher.update(&sample.weight.to_le_bytes());
        for (key, value) in &sample.metadata {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn derive_seed(commit_sha: &str, dataset_hash: &str, config: &TrainingConfig) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(commit_sha.as_bytes());
    hasher.update(dataset_hash.as_bytes());
    hasher.update(&config.rank.to_le_bytes());
    hasher.update(&config.alpha.to_le_bytes());
    hasher.update(&config.learning_rate.to_le_bytes());
    hasher.update(&config.batch_size.to_le_bytes());
    hasher.update(&config.epochs.to_le_bytes());
    hasher.update(&config.hidden_dim.to_le_bytes());
    let digest = hasher.finalize();
    let mut seed_bytes = [0u8; 8];
    seed_bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(seed_bytes)
}

async fn register_adapter(
    adapter_id: &str,
    hash_b3: &str,
    tier: i32,
    config: &TrainingConfig,
    repo_id: &str,
    commit_sha: &str,
) -> Result<Option<String>> {
    let db = Db::connect_env().await?;
    db.migrate().await?;

    if let Some(existing) = db.get_adapter(adapter_id).await? {
        if existing.hash_b3 == hash_b3 {
            info!(
                adapter = adapter_id,
                "Adapter already registered with identical hash"
            );
            return Ok(Some(existing.id));
        }
        return Err(AosError::Validation(format!(
            "Adapter {} already registered with hash {}",
            adapter_id, existing.hash_b3
        )));
    }

    let rank_i32 = i32::try_from(config.rank)
        .map_err(|_| AosError::Validation(format!("Training rank {} exceeds i32", config.rank)))?;

    // Convert numeric tier to string: 0 = ephemeral, 1 = warm, 2+ = persistent
    let tier_str = match tier {
        0 => "ephemeral",
        1 => "warm",
        _ => "persistent",
    };

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(hash_b3)
        .rank(rank_i32)
        .tier(tier_str)
        .category("code".to_string())
        .scope(repo_id.to_string())
        .repo_id(Some(repo_id.to_string()))
        .commit_sha(Some(commit_sha.to_string()))
        .intent(Some("code_ingestion".to_string()))
        .build()
        .map_err(|e| AosError::Validation(format!("invalid registration params: {}", e)))?;

    let row_id = db.register_adapter(params).await?;
    info!(adapter = adapter_id, registry_id = %row_id, "Registered adapter in control plane");
    Ok(Some(row_id))
}

fn should_capture_symbol(symbol: &SymbolNode, cfg: &CodeDatasetConfig) -> bool {
    matches!(
        symbol.kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Struct
            | SymbolKind::Class
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Impl
    ) && (cfg.include_private || matches!(symbol.visibility, Visibility::Public))
}

fn sanitize_whitespace(input: &str) -> String {
    input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Class => "class",
        SymbolKind::Trait => "trait",
        SymbolKind::Enum => "enum",
        SymbolKind::Impl => "impl block",
        SymbolKind::Type => "type",
        SymbolKind::Const => "const",
        SymbolKind::Static => "static",
        SymbolKind::Macro => "macro",
        SymbolKind::Module => "module",
        SymbolKind::Field => "field",
        SymbolKind::Variant => "variant",
        SymbolKind::AssociatedType => "associated type",
        SymbolKind::AssociatedConst => "associated const",
    }
}

fn visibility_label(vis: &Visibility) -> &'static str {
    match vis {
        Visibility::Public => "public",
        Visibility::Private => "private",
    }
}

fn relative_path(root: &Path, file_path: &str) -> String {
    let input = PathBuf::from(file_path);
    if input.is_absolute() {
        if let Ok(stripped) = input.strip_prefix(root) {
            return stripped.to_string_lossy().replace('\\', "/");
        }
        return input.to_string_lossy().replace('\\', "/");
    }
    input.to_string_lossy().replace('\\', "/")
}

async fn prepare_repo(source: &CodeIngestionSource) -> Result<PreparedRepo> {
    match source {
        CodeIngestionSource::LocalPath(path) => {
            let path_clone = path.clone();
            task::spawn_blocking(move || load_local_repo(&path_clone, None, None))
                .await
                .map_err(|e| AosError::Git(format!("Git task join failure: {}", e)))??
        }
        CodeIngestionSource::GitUrl(url) => {
            let url_clone = url.clone();
            task::spawn_blocking(move || clone_remote_repo(&url_clone))
                .await
                .map_err(|e| AosError::Git(format!("Git clone task failed: {}", e)))??
        }
    }
}

fn load_local_repo(
    path: &Path,
    temp_dir: Option<TempDir>,
    remote_url: Option<String>,
) -> Result<PreparedRepo> {
    let repo = Repository::discover(path)
        .map_err(|e| AosError::Git(format!("Failed to open repository: {}", e)))?;
    let workdir = repo.workdir().ok_or_else(|| {
        AosError::Git("Repository is bare; working directory required".to_string())
    })?;
    let root = std::fs::canonicalize(workdir).map_err(|e| {
        AosError::Io(format!(
            "Failed to canonicalize repo root {}: {}",
            workdir.display(),
            e
        ))
    })?;

    // Compute scan root (canonicalized input path) and its relative path to repo root
    let scan_root = std::fs::canonicalize(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to canonicalize scan root {}: {}",
            path.display(),
            e
        ))
    })?;

    // Compute relative path from repo root to scan root
    let scan_root_relative = if scan_root == root {
        String::new()
    } else {
        scan_root
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| scan_root.to_string_lossy().replace('\\', "/"))
    };

    let head = repo
        .head()
        .map_err(|e| AosError::Git(format!("Failed to resolve HEAD: {}", e)))?;
    let commit = head
        .peel_to_commit()
        .map_err(|e| AosError::Git(format!("Failed to read HEAD commit: {}", e)))?;

    let commit_sha = commit.id().to_string();
    let summary = commit.summary().unwrap_or("").to_string();

    // Build full commit metadata
    let author = commit.author();
    let committer = commit.committer();
    let commit_time = commit.time();
    let commit_timestamp = commit_time.seconds();
    let commit_date = Utc
        .timestamp_opt(commit_timestamp, 0)
        .single()
        .map(|dt| dt.to_rfc3339());

    let commit_metadata = CommitMetadata {
        sha: commit_sha.clone(),
        short_sha: commit_sha.get(0..8).unwrap_or(&commit_sha).to_string(),
        author_name: Some(author.name().unwrap_or("").to_string()),
        author_email: Some(author.email().unwrap_or("").to_string()),
        commit_date,
        commit_timestamp: Some(commit_timestamp),
        message_summary: Some(summary.clone()),
        message_body: commit.message().map(|s| s.to_string()),
        committer_name: Some(committer.name().unwrap_or("").to_string()),
        committer_email: Some(committer.email().unwrap_or("").to_string()),
        parent_shas: commit.parent_ids().map(|id| id.to_string()).collect(),
    };

    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "repo".to_string());
    let repo_slug = normalize_repo_slug(&repo_name);

    Ok(PreparedRepo {
        root,
        scan_root,
        scan_root_relative,
        repo_name,
        repo_slug,
        commit_sha,
        commit_summary: summary,
        commit_metadata,
        remote_url,
        _temp_dir: temp_dir,
    })
}

fn clone_remote_repo(url: &str) -> Result<PreparedRepo> {
    let tmp_root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&tmp_root).map_err(|e| {
        AosError::Io(format!(
            "Failed to create temp root {}: {}",
            tmp_root.display(),
            e
        ))
    })?;
    let temp_dir =
        TempDir::new_in(&tmp_root).map_err(|e| AosError::Io(format!("Temp dir error: {}", e)))?;
    let clone_path = temp_dir.path().join("repo");
    Repository::clone(url, &clone_path)
        .map_err(|e| AosError::Git(format!("Clone failed: {}", e)))?;
    load_local_repo(&clone_path, Some(temp_dir), Some(url.to_string()))
}

/// Normalize a repository name into a canonical slug format.
///
/// This function ensures consistent, URL-safe repository slugs by:
/// - Trimming leading/trailing whitespace
/// - Converting to lowercase for case-insensitive matching
/// - Replacing non-alphanumeric characters with underscores
/// - Collapsing consecutive underscores into single underscores
/// - Removing leading/trailing underscores
/// - Truncating to a maximum length of 64 characters
/// - Returns "repo" for empty or invalid inputs
///
/// # Examples
///
/// ```
/// use adapteros_orchestrator::code_ingestion::normalize_repo_slug;
///
/// assert_eq!(normalize_repo_slug("AdapterOS-Core"), "adapteros_core");
/// assert_eq!(normalize_repo_slug("My Awesome Repo!"), "my_awesome_repo");
/// assert_eq!(normalize_repo_slug("__weird__"), "weird");
/// assert_eq!(normalize_repo_slug(""), "repo");
/// ```
pub fn normalize_repo_slug(input: &str) -> String {
    const MAX_SLUG_LENGTH: usize = 64;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "repo".to_string();
    }

    let mut slug = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    // Collapse consecutive underscores
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }

    // Trim leading/trailing underscores
    let trimmed_slug = slug.trim_matches('_');
    if trimmed_slug.is_empty() {
        return "repo".to_string();
    }

    // Truncate to max length, ensuring we don't cut in the middle of a word
    let mut result = trimmed_slug.to_string();
    if result.len() > MAX_SLUG_LENGTH {
        result.truncate(MAX_SLUG_LENGTH);
        // Remove trailing underscore if truncation created one
        result = result.trim_end_matches('_').to_string();
        if result.is_empty() {
            return "repo".to_string();
        }
    }

    result
}

/// Alias for backward compatibility - use `normalize_repo_slug` instead.
#[deprecated(since = "0.1.0", note = "Use normalize_repo_slug instead")]
fn slugify(input: &str) -> String {
    normalize_repo_slug(input)
}

/// Normalize a repository identifier to a canonical form.
///
/// This function ensures consistent repo identifiers by:
/// - Trimming leading/trailing whitespace
/// - Converting to lowercase for case-insensitive matching
/// - Removing trailing slashes
/// - Collapsing multiple consecutive slashes to single slashes
/// - Stripping common URL schemes (https://, http://, git://, ssh://)
/// - Converting git SSH format (git@host:path) to standard path format
/// - Removing `.git` suffix from URLs
///
/// The `repo:` prefix is preserved if present, as it indicates a locally-derived
/// repository identifier rather than a URL-based one.
pub fn normalize_repo_id(repo_id: &str) -> String {
    let trimmed = repo_id.trim();
    if trimmed.is_empty() {
        return "repo".to_string();
    }

    let mut normalized = trimmed.to_lowercase();

    // Strip common URL schemes
    for scheme in &["https://", "http://", "git://", "ssh://"] {
        if let Some(stripped) = normalized.strip_prefix(scheme) {
            normalized = stripped.to_string();
            break;
        }
    }

    // Handle git@ SSH format: git@github.com:org/repo -> github.com/org/repo
    if let Some(stripped) = normalized.strip_prefix("git@") {
        normalized = stripped.to_string();
        // Convert first colon to slash (git@github.com:org/repo -> github.com/org/repo)
        if let Some(colon_pos) = normalized.find(':') {
            let before_colon = &normalized[..colon_pos];
            let after_colon = &normalized[colon_pos + 1..];
            // Only convert if before colon looks like a domain (contains a dot)
            if before_colon.contains('.') {
                normalized = format!("{}/{}", before_colon, after_colon);
            }
        }
    }

    // Remove .git suffix
    if let Some(stripped) = normalized.strip_suffix(".git") {
        normalized = stripped.to_string();
    }

    // Handle repo: prefix specially - preserve it but normalize the rest
    if let Some(rest) = normalized.strip_prefix("repo:") {
        let normalized_rest = normalize_path_segments(rest);
        if normalized_rest.is_empty() {
            return "repo".to_string();
        }
        return format!("repo:{}", normalized_rest);
    }

    // Normalize path segments (collapse slashes, remove trailing)
    let result = normalize_path_segments(&normalized);
    if result.is_empty() {
        "repo".to_string()
    } else {
        result
    }
}

/// Normalize path segments by collapsing multiple slashes and removing trailing slashes.
fn normalize_path_segments(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_slug_handles_symbols() {
        assert_eq!(normalize_repo_slug("AdapterOS-Core"), "adapteros_core");
        assert_eq!(normalize_repo_slug("__weird__"), "weird");
    }

    #[test]
    fn normalize_repo_slug_handles_case() {
        assert_eq!(normalize_repo_slug("MyRepo"), "myrepo");
        assert_eq!(normalize_repo_slug("MY_REPO"), "my_repo");
        assert_eq!(normalize_repo_slug("My-Awesome-Repo"), "my_awesome_repo");
    }

    #[test]
    fn normalize_repo_slug_handles_special_chars() {
        assert_eq!(normalize_repo_slug("repo@v1.0.0"), "repo_v1_0_0");
        assert_eq!(normalize_repo_slug("my.repo.name"), "my_repo_name");
        assert_eq!(normalize_repo_slug("repo#123"), "repo_123");
        assert_eq!(normalize_repo_slug("my repo name"), "my_repo_name");
    }

    #[test]
    fn normalize_repo_slug_collapses_underscores() {
        assert_eq!(normalize_repo_slug("repo___name"), "repo_name");
        assert_eq!(normalize_repo_slug("a--b--c"), "a_b_c");
        assert_eq!(normalize_repo_slug("__leading_trailing__"), "leading_trailing");
    }

    #[test]
    fn normalize_repo_slug_trims_whitespace() {
        assert_eq!(normalize_repo_slug("  myrepo  "), "myrepo");
        assert_eq!(normalize_repo_slug("\t\nrepo\n\t"), "repo");
    }

    #[test]
    fn normalize_repo_slug_handles_empty_input() {
        assert_eq!(normalize_repo_slug(""), "repo");
        assert_eq!(normalize_repo_slug("   "), "repo");
        assert_eq!(normalize_repo_slug("___"), "repo");
        assert_eq!(normalize_repo_slug("---"), "repo");
    }

    #[test]
    fn normalize_repo_slug_truncates_long_names() {
        let long_name = "a".repeat(100);
        let result = normalize_repo_slug(&long_name);
        assert_eq!(result.len(), 64);
        assert_eq!(result, "a".repeat(64));

        // Test that truncation handles trailing underscores
        let long_with_separator = format!("{}_{}", "a".repeat(63), "b".repeat(10));
        let result = normalize_repo_slug(&long_with_separator);
        assert!(result.len() <= 64);
        assert!(!result.ends_with('_'));
    }

    #[test]
    fn sanitize_whitespace_collapses_lines() {
        let doc = "Line one\n\n    Line two  ";
        assert_eq!(sanitize_whitespace(doc), "Line one Line two");
    }

    #[test]
    fn normalize_repo_id_handles_case() {
        assert_eq!(normalize_repo_id("GitHub.com/Org/Repo"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("GITHUB.COM/ORG/REPO"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_removes_trailing_slashes() {
        assert_eq!(normalize_repo_id("github.com/org/repo/"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("github.com/org/repo///"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_collapses_slashes() {
        assert_eq!(normalize_repo_id("github.com//org///repo"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_strips_url_schemes() {
        assert_eq!(normalize_repo_id("https://github.com/org/repo"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("http://github.com/org/repo"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("git://github.com/org/repo"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("ssh://github.com/org/repo"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_handles_git_ssh_format() {
        assert_eq!(normalize_repo_id("git@github.com:org/repo"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("git@github.com:org/repo.git"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_removes_git_suffix() {
        assert_eq!(normalize_repo_id("github.com/org/repo.git"), "github.com/org/repo");
        assert_eq!(normalize_repo_id("https://github.com/org/repo.git"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_preserves_repo_prefix() {
        assert_eq!(normalize_repo_id("repo:my-project"), "repo:my-project");
        assert_eq!(normalize_repo_id("  repo:my-project  "), "repo:my-project");
        assert_eq!(normalize_repo_id("repo:My-Project"), "repo:my-project");
    }

    #[test]
    fn normalize_repo_id_trims_whitespace() {
        assert_eq!(normalize_repo_id("  github.com/org/repo  "), "github.com/org/repo");
        assert_eq!(normalize_repo_id("\t\ngithub.com/org/repo\n\t"), "github.com/org/repo");
    }

    #[test]
    fn normalize_repo_id_handles_empty_input() {
        assert_eq!(normalize_repo_id(""), "repo");
        assert_eq!(normalize_repo_id("   "), "repo");
        assert_eq!(normalize_repo_id("///"), "repo");
    }

    #[test]
    fn normalize_repo_id_simple_names() {
        assert_eq!(normalize_repo_id("my-repo"), "my-repo");
        assert_eq!(normalize_repo_id("org/repo"), "org/repo");
    }
}
