//! Codebase Ingestion Pipeline for Automated Adapter Training
//!
//! This module implements an end-to-end pipeline to:
//! 1. Parse a repository using CodeGraph to extract symbols and documentation
//! 2. Generate Q&A training pairs from code (functions, structs, docs)
//! 3. Train a LoRA adapter on the extracted knowledge
//! 4. Package and register the adapter with deterministic hashing
//!
//! The pipeline ensures determinism by:
//! - Using content-based seeds for training
//! - Sorting all extracted data consistently
//! - Using BLAKE3 hashing for reproducibility
//!

use adapteros_codegraph::{CodeGraph, SymbolKind, SymbolNode, Visibility};
use adapteros_core::seed::derive_seed_u64;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, ScanRootMetadata, TrainingConfig,
    TrainingExample,
};
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1};
use adapteros_platform::common::PlatformUtils;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tracing::{debug, info};

use crate::code_ingestion::{normalize_repo_slug, CodebaseScopeMetadata};

/// Configuration for codebase ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    /// Training configuration for the adapter
    pub training_config: TrainingConfig,

    /// Tokenizer path (auto-discovered from AOS_MODEL_PATH if not set)
    pub tokenizer_path: Option<PathBuf>,

    /// Maximum number of Q&A pairs to generate per symbol
    pub max_pairs_per_symbol: usize,

    /// Include private symbols (default: false, only public APIs)
    pub include_private: bool,

    /// Minimum documentation length to generate Q&A pairs
    pub min_doc_length: usize,

    /// Generate negative examples (for abstention training)
    pub generate_negative_examples: bool,

    /// Base model identifier for packaged adapter
    pub base_model: String,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            training_config: TrainingConfig::default(),
            tokenizer_path: None,
            max_pairs_per_symbol: 3,
            include_private: false,
            min_doc_length: 20,
            generate_negative_examples: true,
            base_model: "qwen2.5-7b".to_string(),
        }
    }
}

/// Result of the ingestion pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResult {
    /// Adapter ID
    pub adapter_id: String,

    /// BLAKE3 hash of the packaged adapter
    pub adapter_hash: String,

    /// Repository path ingested
    pub repo_path: String,

    /// Git commit SHA (if available)
    pub commit_sha: Option<String>,

    /// Number of symbols extracted
    pub symbols_count: usize,

    /// Number of training examples generated
    pub examples_count: usize,

    /// Final training loss
    pub final_loss: f32,

    /// Training time in milliseconds
    pub training_time_ms: u64,

    /// Content hash for reproducibility
    pub content_hash: String,
}

#[derive(Debug, Default)]
struct RepoGitMetadata {
    commit_sha: Option<String>,
    branch: Option<String>,
    remote_url: Option<String>,
}

/// Codebase ingestion pipeline
pub struct CodebaseIngestion {
    config: IngestionConfig,
}

impl CodebaseIngestion {
    /// Create a new ingestion pipeline
    pub fn new(config: IngestionConfig) -> Result<Self> {
        info!("CodebaseIngestion pipeline initialized");
        Ok(Self { config })
    }

    /// Run the full ingestion pipeline.
    pub async fn ingest_and_train(
        &self,
        repo_path: &Path,
        adapter_id: &str,
        adapters_root: &Path,
    ) -> Result<IngestionResult> {
        info!(
            "Starting codebase ingestion for repository: {} (adapter: {})",
            repo_path.display(),
            adapter_id
        );

        let start_time = Instant::now();

        // Build CodeGraph from the repository
        let graph = CodeGraph::from_directory(repo_path, None).await?;

        let symbols_count = graph.symbols.len();
        info!(symbols = symbols_count, "Extracted symbols from repository");

        let repo_root = repo_path
            .canonicalize()
            .unwrap_or_else(|_| repo_path.to_path_buf());
        let repo_root_str = PlatformUtils::normalize_path_separators(&repo_root.to_string_lossy());
        let repo_name = repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("repo")
            .to_string();
        let repo_slug = normalize_repo_slug(&repo_name);
        let git_meta = get_repo_git_metadata(&repo_root);
        let commit_sha = git_meta.commit_sha;
        let branch = git_meta.branch;
        let remote_url = git_meta.remote_url;

        // Generate Q&A training pairs from symbols
        let samples = self.generate_qa_pairs(&graph, repo_path)?;
        if samples.is_empty() {
            return Err(AosError::Training(
                "No training samples generated from codebase".to_string(),
            ));
        }

        // Compute content hash for reproducibility
        let content_hash = compute_samples_hash(&samples);
        debug!(
            samples = samples.len(),
            hash = %content_hash,
            "Generated training samples"
        );

        // Resolve tokenizer via canonical discovery (config > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
        let tokenizer_path =
            adapteros_config::resolve_tokenizer_path(self.config.tokenizer_path.as_ref())?;
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load tokenizer {}: {}",
                tokenizer_path.display(),
                e
            ))
        })?;

        let training_examples = encode_qa_samples(&tokenizer, &samples)?;
        let examples_count = training_examples.len();
        let (positive_count, negative_count) = count_samples_by_weight(&samples);

        let training_config_hash = compute_training_config_hash(&self.config.training_config);
        let commit_seed = commit_sha.as_deref().unwrap_or("unknown");
        let seed_inputs = SeedInputs {
            commit_sha: commit_seed,
            dataset_hash_b3: &content_hash,
            training_config_hash: &training_config_hash,
            base_model_id: &self.config.base_model,
            repo_slug: &repo_slug,
        };
        let seed_inputs_json = serialize_seed_inputs(&seed_inputs)?;
        let derived_seed = derive_training_seed(&seed_inputs_json);
        let seed_override = self
            .config
            .training_config
            .determinism
            .as_ref()
            .and_then(|d| d.seed);
        let seed_source = if seed_override.is_some() {
            "config"
        } else {
            "derived"
        };
        let seed = seed_override.unwrap_or(derived_seed);

        let mut training_config = self.config.training_config.clone();
        let mut determinism = training_config.determinism.unwrap_or_default();
        determinism.seed = Some(seed);
        training_config.determinism = Some(determinism);

        // Train the LoRA adapter
        let mut trainer = MicroLoRATrainer::new(training_config.clone())?;
        info!(seed, seed_source, "Using deterministic training seed");

        // Run training
        let mut training_result = trainer.train(&training_examples).await?;
        training_result.adapter_id = adapter_id.to_string();
        let final_loss = training_result.final_loss;

        let repo_identifier = format!("repo:{}", repo_slug);
        let mut metadata = BTreeMap::new();
        metadata.insert("repo_name".to_string(), repo_name.clone());
        metadata.insert("repo_slug".to_string(), repo_slug.clone());
        metadata.insert("scope".to_string(), repo_slug.clone());
        metadata.insert("repo_identifier".to_string(), repo_identifier.clone());
        metadata.insert("scope_repo_id".to_string(), repo_identifier.clone());
        if let Some(ref commit) = commit_sha {
            metadata.insert("repo_commit".to_string(), commit.clone());
            metadata.insert(
                "repo_short_commit".to_string(),
                commit.chars().take(8).collect(),
            );
        }
        metadata.insert("repo_root_path".to_string(), repo_root_str.clone());
        metadata.insert("repo_path".to_string(), repo_root_str.clone());
        metadata.insert("scan_root_path".to_string(), repo_root_str.clone());
        if let Some(ref branch) = branch {
            metadata.insert("repo_branch".to_string(), branch.clone());
        }
        if let Some(ref remote) = remote_url {
            metadata.insert("repo_remote".to_string(), remote.clone());
        }
        let scan_roots = vec![ScanRootMetadata {
            path: repo_root_str.clone(),
            label: Some("primary".to_string()),
            file_count: None,
            byte_count: None,
            content_hash: None,
            scanned_at: None,
        }];
        if let Ok(scan_roots_json) = serde_json::to_string(&scan_roots) {
            metadata.insert("scan_roots".to_string(), scan_roots_json);
        }

        let scope_meta = CodebaseScopeMetadata {
            repo: Some(repo_name.clone()),
            repo_slug: Some(repo_slug.clone()),
            repo_id: Some(repo_identifier.clone()),
            branch: branch.clone(),
            commit: commit_sha.clone(),
            scan_root: Some(repo_root_str.clone()),
            remote_url: remote_url.clone(),
        };
        for (key, value) in scope_meta.to_metadata_map() {
            metadata.insert(key, value);
        }

        metadata.insert("dataset_hash".to_string(), content_hash.clone());
        metadata.insert("dataset_hash_b3".to_string(), content_hash.clone());
        metadata.insert(
            "training_config_hash".to_string(),
            training_config_hash.clone(),
        );
        metadata.insert("seed_inputs_json".to_string(), seed_inputs_json);
        metadata.insert("determinism_seed".to_string(), seed.to_string());
        metadata.insert("seed_source".to_string(), seed_source.to_string());
        metadata.insert("dataset_examples".to_string(), examples_count.to_string());
        metadata.insert(
            "dataset_positive_examples".to_string(),
            positive_count.to_string(),
        );
        metadata.insert(
            "dataset_negative_examples".to_string(),
            negative_count.to_string(),
        );
        metadata.insert("base_model_id".to_string(), self.config.base_model.clone());
        metadata.insert("category".to_string(), "codebase".to_string());
        metadata.insert("generator".to_string(), "codebase_ingestion".to_string());
        metadata.insert("stream_mode".to_string(), "false".to_string());

        let mut package_metadata: HashMap<String, String> = metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(ref backend) = training_result.backend {
            package_metadata.insert("training_backend".to_string(), backend.clone());
        }
        if let Some(ref device) = training_result.backend_device {
            package_metadata.insert("training_backend_device".to_string(), device.clone());
        }

        let quantized = LoRAQuantizer::quantize_to_q15(&training_result.weights);
        let packager = AdapterPackager::new(adapters_root);
        let packaged = packager
            .package_aos_with_metadata(
                "default",
                adapter_id,
                &quantized,
                &training_config,
                &self.config.base_model,
                package_metadata,
            )
            .await?;
        let aos_path = packaged.weights_path;

        // Compute adapter hash
        let aos_bytes = fs::read(&aos_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read {}: {}", aos_path.display(), e)))?;
        let adapter_hash = blake3::hash(&aos_bytes).to_hex().to_string();
        info!(
            path = %aos_path.display(),
            hash = %adapter_hash,
            "Packaged codebase adapter"
        );

        let training_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            adapter_id = adapter_id,
            hash = %adapter_hash,
            examples = examples_count,
            loss = final_loss,
            time_ms = training_time_ms,
            "Codebase ingestion completed"
        );

        Ok(IngestionResult {
            adapter_id: adapter_id.to_string(),
            adapter_hash,
            repo_path: repo_root_str,
            commit_sha,
            symbols_count,
            examples_count,
            final_loss,
            training_time_ms,
            content_hash,
        })
    }

    /// Generate Q&A training pairs from CodeGraph symbols
    fn generate_qa_pairs(&self, graph: &CodeGraph, repo_path: &Path) -> Result<Vec<QAPair>> {
        let mut pairs = Vec::new();

        // Select and sort symbols for deterministic ordering
        let mut symbols: Vec<&SymbolNode> = graph
            .symbols
            .values()
            .filter(|s| self.should_include_symbol(s))
            .collect();
        symbols.sort_by_key(|a| a.qualified_name());

        for symbol in symbols {
            // Generate positive examples (knowledge)
            let positive_pairs = self.generate_positive_pairs(symbol, repo_path);
            pairs.extend(positive_pairs);

            // Generate negative examples (abstention) for undocumented symbols
            if self.config.generate_negative_examples
                && symbol
                    .docstring
                    .as_ref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
            {
                if let Some(negative) = self.generate_negative_pair(symbol, repo_path) {
                    pairs.push(negative);
                }
            }
        }

        Ok(pairs)
    }

    /// Check if symbol should be included based on config
    fn should_include_symbol(&self, symbol: &SymbolNode) -> bool {
        // Filter by symbol kind
        let valid_kind = matches!(
            symbol.kind,
            SymbolKind::Function
                | SymbolKind::Method
                | SymbolKind::Struct
                | SymbolKind::Trait
                | SymbolKind::Enum
        );

        // Filter by visibility
        let visible =
            self.config.include_private || matches!(symbol.visibility, Visibility::Public);

        valid_kind && visible
    }

    /// Generate positive Q&A pairs for a symbol
    fn generate_positive_pairs(&self, symbol: &SymbolNode, repo_path: &Path) -> Vec<QAPair> {
        let mut pairs = Vec::new();
        let rel_path = relative_path(repo_path, &symbol.file_path);
        let kind_label = symbol_kind_label(&symbol.kind);

        // Basic "what does X do" question
        let question = format!(
            "What does the {} `{}` in {} do?",
            kind_label,
            symbol.qualified_name(),
            rel_path
        );

        let mut answer = format!(
            "`{}` is a {} defined in `{}` (lines {}-{}).",
            symbol.qualified_name(),
            kind_label,
            rel_path,
            symbol.span.start_line,
            symbol.span.end_line,
        );

        // Add signature if available
        if let Some(sig) = &symbol.signature {
            answer.push_str(&format!(" Signature: {}.", sig.trim()));
        }

        // Add return type if available
        if let Some(type_ann) = &symbol.type_annotation {
            if let Some(ret) = &type_ann.return_type {
                answer.push_str(&format!(" Returns `{}`.", ret));
            }
        }

        // Add documentation if available and meets minimum length
        if let Some(doc) = symbol
            .docstring
            .as_ref()
            .filter(|s| s.trim().len() >= self.config.min_doc_length)
        {
            answer.push_str(&format!(" Documentation: {}", sanitize_whitespace(doc)));
        }

        answer.push_str(&format!(
            " Visibility: {}.",
            visibility_label(&symbol.visibility)
        ));

        let mut metadata = BTreeMap::new();
        metadata.insert("symbol_kind".to_string(), kind_label.to_string());
        metadata.insert("language".to_string(), symbol.language.to_string());
        metadata.insert("file_path".to_string(), rel_path.clone());
        metadata.insert("sample_role".to_string(), "positive".to_string());

        // Generate additional pairs up to max_pairs_per_symbol
        if pairs.len() < self.config.max_pairs_per_symbol {
            // "Where is X defined" question
            let where_q = format!("Where is `{}` defined?", symbol.qualified_name());
            let where_a = format!(
                "`{}` is defined in `{}` at lines {}-{}.",
                symbol.qualified_name(),
                rel_path,
                symbol.span.start_line,
                symbol.span.end_line
            );

            let mut meta = metadata.clone();
            meta.insert("question_type".to_string(), "location".to_string());

            pairs.push(QAPair {
                question: where_q,
                answer: where_a,
                metadata: meta,
                weight: 1.0,
            });
        }

        pairs.push(QAPair {
            question,
            answer,
            metadata,
            weight: 1.0,
        });

        pairs.truncate(self.config.max_pairs_per_symbol);
        pairs
    }

    /// Generate negative Q&A pair for abstention training
    fn generate_negative_pair(&self, symbol: &SymbolNode, repo_path: &Path) -> Option<QAPair> {
        let rel_path = relative_path(repo_path, &symbol.file_path);
        let kind_label = symbol_kind_label(&symbol.kind);

        let question = format!(
            "Explain the undocumented {} `{}` in {}.",
            kind_label,
            symbol.qualified_name(),
            rel_path
        );

        let answer = format!(
            "I don't know. `{}` at `{}` lacks documentation, so I won't speculate about its behavior.",
            symbol.qualified_name(),
            rel_path
        );

        let mut metadata = BTreeMap::new();
        metadata.insert("symbol_kind".to_string(), kind_label.to_string());
        metadata.insert("language".to_string(), symbol.language.to_string());
        metadata.insert("file_path".to_string(), rel_path);
        metadata.insert("sample_role".to_string(), "negative".to_string());
        metadata.insert("reason".to_string(), "missing_docstring".to_string());

        Some(QAPair {
            question,
            answer,
            metadata,
            weight: -0.5,
        })
    }
}

/// Q&A training pair
#[derive(Debug, Clone)]
struct QAPair {
    question: String,
    answer: String,
    metadata: BTreeMap<String, String>,
    weight: f32,
}

fn get_repo_git_metadata(repo_path: &Path) -> RepoGitMetadata {
    let repo = match git2::Repository::discover(repo_path) {
        Ok(r) => r,
        Err(_) => return RepoGitMetadata::default(),
    };

    let mut meta = RepoGitMetadata::default();
    if let Ok(head) = repo.head() {
        if head.is_branch() {
            if let Some(name) = head.shorthand().filter(|name| !name.is_empty()) {
                meta.branch = Some(name.to_string());
            }
        }
        if let Ok(commit) = head.peel_to_commit() {
            meta.commit_sha = Some(commit.id().to_string());
        }
    }

    if let Ok(remote) = repo.find_remote("origin") {
        if let Some(url) = remote.url().filter(|url| !url.trim().is_empty()) {
            meta.remote_url = Some(url.to_string());
        }
    } else if let Ok(remotes) = repo.remotes() {
        for name in remotes.iter().flatten() {
            if let Ok(remote) = repo.find_remote(name) {
                if let Some(url) = remote.url().filter(|url| !url.trim().is_empty()) {
                    meta.remote_url = Some(url.to_string());
                    break;
                }
            }
        }
    }

    meta
}

/// Compute hash of training samples for reproducibility
fn compute_samples_hash(samples: &[QAPair]) -> String {
    let mut hasher = Hasher::new();
    for sample in samples {
        hasher.update(sample.question.as_bytes());
        hasher.update(sample.answer.as_bytes());
        hasher.update(&sample.weight.to_le_bytes());
        for (k, v) in &sample.metadata {
            hasher.update(k.as_bytes());
            hasher.update(v.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn count_samples_by_weight(samples: &[QAPair]) -> (usize, usize) {
    let mut positive = 0;
    let mut negative = 0;
    for sample in samples {
        if sample.weight < 0.0 {
            negative += 1;
        } else {
            positive += 1;
        }
    }
    (positive, negative)
}

/// Compute a BLAKE3 hash of the training configuration for reproducibility tracking.
fn compute_training_config_hash(config: &TrainingConfig) -> String {
    let mut hasher = Hasher::new();
    hasher.update(&config.rank.to_le_bytes());
    hasher.update(&config.alpha.to_le_bytes());
    hasher.update(&config.learning_rate.to_le_bytes());
    hasher.update(&config.batch_size.to_le_bytes());
    hasher.update(&config.epochs.to_le_bytes());
    hasher.update(&config.hidden_dim.to_le_bytes());
    hasher.update(&config.vocab_size.to_le_bytes());

    hasher.update(&[config.require_gpu as u8]);
    hasher.update(&config.max_gpu_memory_mb.to_le_bytes());

    if let Some(backend) = config.preferred_backend {
        hasher.update(&[1]);
        hasher.update(backend.tag().as_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(policy) = config.backend_policy {
        hasher.update(&[1]);
        hasher.update(policy.as_str().as_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(backend) = config.coreml_fallback_backend {
        hasher.update(&[1]);
        hasher.update(backend.tag().as_bytes());
    } else {
        hasher.update(&[0]);
    }

    if let Some(max_tokens) = config.max_tokens_per_batch {
        hasher.update(&[1]);
        hasher.update(&max_tokens.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(interval) = config.checkpoint_interval {
        hasher.update(&[1]);
        hasher.update(&interval.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(warmup) = config.warmup_steps {
        hasher.update(&[1]);
        hasher.update(&warmup.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(max_seq) = config.max_seq_length {
        hasher.update(&[1]);
        hasher.update(&max_seq.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(grad_accum) = config.gradient_accumulation_steps {
        hasher.update(&[1]);
        hasher.update(&grad_accum.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }

    if let Some(ref device_policy) = config.device_policy {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(device_policy) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref placement) = config.coreml_placement {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(placement) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref moe_config) = config.moe_config {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(moe_config) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref preprocessing) = config.preprocessing {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(preprocessing) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }

    hasher.finalize().to_hex().to_string()
}

#[derive(Debug, Serialize)]
struct SeedInputs<'a> {
    commit_sha: &'a str,
    dataset_hash_b3: &'a str,
    training_config_hash: &'a str,
    base_model_id: &'a str,
    repo_slug: &'a str,
}

fn serialize_seed_inputs(inputs: &SeedInputs<'_>) -> Result<String> {
    serde_json::to_string(inputs).map_err(AosError::Serialization)
}

/// Derive deterministic training seed using HKDF-SHA256 with BLAKE3 global seed.
fn derive_training_seed(seed_inputs_json: &str) -> u64 {
    let global = B3Hash::hash(seed_inputs_json.as_bytes());
    derive_seed_u64(&global, "codebase-training")
}

/// Encode Q&A samples to training examples
fn encode_qa_samples(
    tokenizer: &QwenTokenizer,
    samples: &[QAPair],
) -> Result<Vec<TrainingExample>> {
    let mut examples = Vec::with_capacity(samples.len());
    let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
        AosError::Training("Tokenizer missing pad_token_id for codebase ingestion".to_string())
    })?;
    let created_at_unix_ms = chrono::Utc::now().timestamp_millis() as u64;
    for (index, sample) in samples.iter().enumerate() {
        let input = tokenizer.encode(&sample.question)?;
        let target = tokenizer.encode(&sample.answer)?;
        if input.is_empty() || target.is_empty() {
            continue;
        }
        let mut provenance = BTreeMap::new();
        for (key, value) in sample.metadata.iter() {
            provenance.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        if let Some(num) = serde_json::Number::from_f64(sample.weight as f64) {
            provenance.insert("weight".to_string(), serde_json::Value::Number(num));
        } else {
            provenance.insert(
                "weight".to_string(),
                serde_json::Value::String(sample.weight.to_string()),
            );
        }
        let provenance = provenance_from_map(&provenance)
            .map_err(|e| AosError::Training(format!("Failed to serialize provenance: {}", e)))?;
        let source_id = sample
            .metadata
            .get("file_path")
            .cloned()
            .unwrap_or_else(|| "codebase_ingestion".to_string());
        let metadata = ExampleMetadataV1::new(source_id, index as u64, provenance, created_at_unix_ms);
        let attention_mask =
            TrainingExample::attention_mask_from_tokens(&input, pad_token_id);
        examples.push(TrainingExample::new(
            input,
            target,
            attention_mask,
            metadata,
        ));
    }
    if examples.is_empty() {
        return Err(AosError::Training(
            "No encodable training samples produced".to_string(),
        ));
    }
    Ok(examples)
}

/// Get relative path string
fn relative_path(root: &Path, file_path: &str) -> String {
    let input = PathBuf::from(file_path);
    if input.is_absolute() {
        if let Ok(stripped) = input.strip_prefix(root) {
            return PlatformUtils::normalize_path_separators(&stripped.to_string_lossy());
        }
    }
    PlatformUtils::normalize_path_separators(&input.to_string_lossy())
}

/// Symbol kind to human-readable label
fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
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

/// Visibility to human-readable label
fn visibility_label(vis: &Visibility) -> &'static str {
    match vis {
        Visibility::Public => "public",
        Visibility::Private => "private",
        Visibility::Crate => "crate",
        Visibility::Super => "super",
        Visibility::InPath(_) => "restricted",
    }
}

/// Sanitize whitespace in documentation
fn sanitize_whitespace(input: &str) -> String {
    input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
