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
use git2::Repository;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;
use tokio::task;
use tracing::{debug, info, warn};

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
}

/// Result of a code ingestion training run
#[derive(Debug, Clone)]
pub struct CodeIngestionResult {
    pub adapter_id: String,
    pub repo_name: String,
    pub repo_slug: String,
    pub commit_sha: String,
    pub short_commit_sha: String,
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

        let repo_identifier = request
            .repo_id
            .clone()
            .unwrap_or_else(|| format!("repo:{}", prepared_repo.repo_slug));

        info!(
            repo = %prepared_repo.root.display(),
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
    root: PathBuf,
    repo_name: String,
    repo_slug: String,
    commit_sha: String,
    commit_summary: String,
    remote_url: Option<String>,
    _temp_dir: Option<TempDir>,
}

impl PreparedRepo {
    fn short_sha(&self) -> &str {
        self.commit_sha.get(0..8).unwrap_or(&self.commit_sha)
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

    let head = repo
        .head()
        .map_err(|e| AosError::Git(format!("Failed to resolve HEAD: {}", e)))?;
    let commit = head
        .peel_to_commit()
        .map_err(|e| AosError::Git(format!("Failed to read HEAD commit: {}", e)))?;

    let commit_sha = commit.id().to_string();
    let summary = commit.summary().unwrap_or("").to_string();
    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "repo".to_string());
    let repo_slug = slugify(&repo_name);

    Ok(PreparedRepo {
        root,
        repo_name,
        repo_slug,
        commit_sha,
        commit_summary: summary,
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

fn slugify(input: &str) -> String {
    let mut slug = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_symbols() {
        assert_eq!(slugify("AdapterOS-Core"), "adapteros_core");
        assert_eq!(slugify("__weird__"), "weird");
    }

    #[test]
    fn sanitize_whitespace_collapses_lines() {
        let doc = "Line one\n\n    Line two  ";
        assert_eq!(sanitize_whitespace(doc), "Line one Line two");
    }
}
