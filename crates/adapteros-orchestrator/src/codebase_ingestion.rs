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
//! NOTE: This module is currently a stub implementation pending MicroLoRATrainer API updates.

use adapteros_codegraph::{CodeGraph, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{MicroLoRATrainer, TrainingConfig, TrainingExample};
use adapteros_platform::common::PlatformUtils;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tracing::{debug, info, warn};

/// Configuration for codebase ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    /// Training configuration for the adapter
    pub training_config: TrainingConfig,

    /// Tokenizer path (defaults to models/qwen2.5-7b-mlx/tokenizer.json)
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

    /// Run the full ingestion pipeline
    ///
    /// NOTE: This is a stub implementation. The full training pipeline requires
    /// MicroLoRATrainer API updates for seed override and .aos packaging.
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

        // Get git commit SHA if available
        let commit_sha = get_commit_sha(repo_path);

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

        // Load tokenizer and encode samples
        let tokenizer_path = adapteros_config::resolve_tokenizer_path(self.config.tokenizer_path.as_ref())?;
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load tokenizer {}: {}",
                tokenizer_path.display(),
                e
            ))
        })?;

        let training_examples = encode_qa_samples(&tokenizer, &samples)?;
        let examples_count = training_examples.len();

        // Train the LoRA adapter
        let mut trainer = MicroLoRATrainer::new(self.config.training_config.clone())?;

        // Derive deterministic seed from content (logged for reproducibility)
        let seed = derive_training_seed(&content_hash, commit_sha.as_deref());
        info!(seed = seed, "Deterministic seed derived from content");

        // Run training
        let training_result = trainer.train(&training_examples).await?;
        let final_loss = training_result.final_loss;

        // Package the adapter as .aos file
        fs::create_dir_all(adapters_root).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create adapters directory {}: {}",
                adapters_root.display(),
                e
            ))
        })?;

        let aos_path = adapters_root.join(format!("{}.aos", adapter_id));

        // TODO: Implement proper .aos packaging when MicroLoRATrainer API is extended
        // For now, we save a placeholder manifest
        warn!("Full .aos packaging not yet implemented - saving placeholder");
        let placeholder_manifest = serde_json::json!({
            "adapter_id": adapter_id,
            "repo_path": repo_path.display().to_string(),
            "symbols_count": symbols_count,
            "examples_count": examples_count,
            "content_hash": content_hash,
            "commit_sha": commit_sha,
            "generator": "codebase_ingestion",
            "final_loss": final_loss,
        });
        fs::write(
            &aos_path,
            serde_json::to_string_pretty(&placeholder_manifest).unwrap_or_default(),
        )
        .await
        .map_err(|e| AosError::Io(format!("Failed to write {}: {}", aos_path.display(), e)))?;

        // Compute adapter hash
        let aos_bytes = fs::read(&aos_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read {}: {}", aos_path.display(), e)))?;
        let adapter_hash = blake3::hash(&aos_bytes).to_hex().to_string();

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
            repo_path: repo_path.display().to_string(),
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
        symbols.sort_by(|a, b| a.qualified_name().cmp(&b.qualified_name()));

        for symbol in symbols {
            // Generate positive examples (knowledge)
            let positive_pairs = self.generate_positive_pairs(symbol, repo_path);
            pairs.extend(positive_pairs);

            // Generate negative examples (abstention) for undocumented symbols
            if self.config.generate_negative_examples {
                if symbol
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

/// Get git commit SHA from repository
fn get_commit_sha(repo_path: &Path) -> Option<String> {
    let repo = match git2::Repository::discover(repo_path) {
        Ok(r) => r,
        Err(_) => return None,
    };
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return None,
    };
    let commit = match head.peel_to_commit() {
        Ok(c) => c,
        Err(_) => return None,
    };
    Some(commit.id().to_string())
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

/// Derive deterministic training seed
fn derive_training_seed(content_hash: &str, commit_sha: Option<&str>) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(content_hash.as_bytes());
    if let Some(sha) = commit_sha {
        hasher.update(sha.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

/// Encode Q&A samples to training examples
fn encode_qa_samples(
    tokenizer: &QwenTokenizer,
    samples: &[QAPair],
) -> Result<Vec<TrainingExample>> {
    let mut examples = Vec::with_capacity(samples.len());
    for sample in samples {
        let input = tokenizer.encode(&sample.question)?;
        let target = tokenizer.encode(&sample.answer)?;
        if input.is_empty() || target.is_empty() {
            continue;
        }
        let metadata: HashMap<String, String> = sample
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        examples.push(TrainingExample {
            input,
            target,
            metadata,
            weight: sample.weight,
        });
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
