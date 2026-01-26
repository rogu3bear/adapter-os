//! Training dataset integration - connects document ingestion to training pipeline
//!
//! This module provides the bridge between document ingestion and adapter training:
//! 1. Ingest documents (PDF, Markdown, code files)
//! 2. Extract deterministic text chunks
//! 3. Save JSONL `{ "text": "..." }` rows on disk
//! 4. Create training dataset record in database
//! 5. Link dataset to training jobs

use adapteros_core::seed::get_deterministic_unix_timestamp_millis;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::ProtectedDb;
use adapteros_ingest_docs::{default_ingest_options, load_tokenizer, DocumentIngestor};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

const MAX_INPUT_TOKENS: usize = 256;
const MAX_TARGET_TOKENS: usize = 128;
const STRIDE_TOKENS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingFramingPolicy {
    Supervised,
    RawContinuationV1,
}

impl TrainingFramingPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingFramingPolicy::Supervised => "supervised",
            TrainingFramingPolicy::RawContinuationV1 => "raw_continuation_v1",
        }
    }
}

#[derive(Debug)]
pub struct LoadedDatasetExamples {
    pub examples: Vec<WorkerTrainingExample>,
    pub dataset_hash_b3: String,
    pub dataset_id: String,
    pub framing_policy: TrainingFramingPolicy,
}

/// Training dataset manager for creating and managing training datasets
pub struct TrainingDatasetManager {
    db: ProtectedDb,
    storage_root: PathBuf,
    tokenizer_path: Option<PathBuf>,
}

/// Serializable training generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SerializableTrainingConfig {
    /// Strategy: "identity" only (others rejected)
    pub strategy: String,
    /// Maximum sequence length
    pub max_seq_length: usize,
    /// Add special tokens during tokenization
    pub add_special_tokens: bool,
}

impl Default for SerializableTrainingConfig {
    fn default() -> Self {
        Self {
            strategy: "identity".to_string(),
            max_seq_length: 512,
            add_special_tokens: true,
        }
    }
}

fn ensure_default_training_config(config: &SerializableTrainingConfig) -> Result<()> {
    let default = SerializableTrainingConfig::default();
    if config.strategy != default.strategy
        || config.max_seq_length != default.max_seq_length
        || config.add_special_tokens != default.add_special_tokens
    {
        return Err(AosError::Validation(
            "training_config is not supported for dataset creation; raw text schema only"
                .to_string(),
        ));
    }
    Ok(())
}

/// Request to create a training dataset from ingested documents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDatasetFromFilePathsRequest {
    /// Name of the dataset
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Paths to documents to ingest (PDF, Markdown, etc.)
    pub document_paths: Vec<PathBuf>,
    /// Training generation configuration
    pub training_config: SerializableTrainingConfig,
    /// User who created the dataset
    pub created_by: Option<String>,
}

/// Statistics about a created dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DatasetCreationResult {
    /// Dataset ID
    pub dataset_id: String,
    /// Number of examples generated
    pub num_examples: usize,
    /// Total tokens across all examples
    pub total_tokens: usize,
    /// Storage path where examples are saved
    pub storage_path: String,
    /// BLAKE3 hash of the dataset file
    pub hash_b3: String,
}

impl TrainingDatasetManager {
    /// Create a new training dataset manager
    pub fn new(db: ProtectedDb, storage_root: PathBuf, tokenizer_path: Option<PathBuf>) -> Self {
        Self {
            db,
            storage_root,
            tokenizer_path,
        }
    }

    /// Load training examples from a specific dataset version (canonical JSONL).
    ///
    /// - Validates the dataset version exists and is marked valid.
    /// - Verifies storage_path hash matches recorded hash_b3.
    /// - Returns examples, hash, and parent dataset_id.
    pub async fn load_dataset_version_examples(
        &self,
        dataset_version_id: &str,
    ) -> Result<LoadedDatasetExamples> {
        debug!("Loading training dataset version: {}", dataset_version_id);

        let version = self
            .db
            .get_training_dataset_version(dataset_version_id)
            .await?
            .ok_or_else(|| {
                AosError::NotFound(format!("Dataset version not found: {}", dataset_version_id))
            })?;

        if version.validation_status != "valid" {
            return Err(AosError::Validation(format!(
                "Dataset version {} is not validated (status: {})",
                dataset_version_id, version.validation_status
            )));
        }

        let storage_path = PathBuf::from(&version.storage_path);

        // Verify file integrity with recorded hash
        let actual_hash = self.compute_file_hash(&storage_path).await?;
        if actual_hash != version.hash_b3 {
            return Err(AosError::Validation(format!(
                "Dataset version {} hash mismatch: expected {}, got {}",
                dataset_version_id, version.hash_b3, actual_hash
            )));
        }

        let (examples, framing_policy) = self
            .load_examples_from_jsonl(&storage_path, &version.dataset_id)
            .await?;

        Ok(LoadedDatasetExamples {
            examples,
            dataset_hash_b3: actual_hash,
            dataset_id: version.dataset_id,
            framing_policy,
        })
    }

    /// Create a training dataset from documents
    pub async fn create_dataset_from_documents(
        &self,
        request: CreateDatasetFromFilePathsRequest,
    ) -> Result<DatasetCreationResult> {
        info!(
            "Creating training dataset '{}' from {} documents",
            request.name,
            request.document_paths.len()
        );

        ensure_default_training_config(&request.training_config)?;

        // Load tokenizer (required for training data generation)
        let tokenizer = if let Some(tok_path) = &self.tokenizer_path {
            load_tokenizer(tok_path)?
        } else {
            return Err(AosError::Config(
                "No tokenizer configured for training dataset manager".to_string(),
            ));
        };

        // Create document ingestor
        let chunking_options = default_ingest_options();
        let ingestor = DocumentIngestor::new(chunking_options, Some(tokenizer.clone()));

        // Ingest all documents and generate training examples
        let mut all_rows: Vec<String> = Vec::new();
        let mut total_tokens = 0usize;
        let total_docs = request.document_paths.len();

        for (idx, doc_path) in request.document_paths.iter().enumerate() {
            let progress = ((idx + 1) as f64 / total_docs as f64) * 100.0;

            info!(
                "Processing document {}/{} ({:.1}%): {}",
                idx + 1,
                total_docs,
                progress,
                doc_path.display()
            );

            // Determine document type and ingest
            let ingested_doc = if doc_path.extension().and_then(|s| s.to_str()) == Some("pdf") {
                ingestor
                    .ingest_pdf_path(doc_path)
                    .context(format!("Failed to ingest PDF {}", doc_path.display()))?
            } else {
                ingestor
                    .ingest_markdown_path(doc_path)
                    .context(format!("Failed to ingest Markdown {}", doc_path.display()))?
            };

            let mut rows_count = 0usize;
            for chunk in &ingested_doc.chunks {
                let text = chunk.text.trim();
                if text.is_empty() {
                    warn!(
                        source = %ingested_doc.source_name,
                        chunk_index = chunk.chunk_index,
                        "Skipping empty document chunk"
                    );
                    continue;
                }
                let encoding = tokenizer
                    .encode(text, false)
                    .map_err(|e| AosError::Validation(format!("Failed to tokenize chunk: {e}")))?;
                total_tokens = total_tokens.saturating_add(encoding.get_ids().len());
                all_rows.push(text.to_string());
                rows_count += 1;
            }

            info!(
                "Generated {} rows ({} tokens) from {} | Total so far: {} rows",
                rows_count,
                total_tokens,
                doc_path.display(),
                all_rows.len()
            );
        }

        if all_rows.is_empty() {
            return Err(AosError::Validation(
                "No dataset rows generated from documents".to_string(),
            ));
        }

        info!("Total dataset rows generated: {}", all_rows.len());

        let avg_input_length = total_tokens as f64 / all_rows.len() as f64;
        let avg_target_length = 0.0;

        // Create storage directory if it doesn't exist
        tokio::fs::create_dir_all(&self.storage_root).await?;

        // Generate deterministic filename for this dataset using deterministic timestamp
        // Format: dataset-{timestamp_ms}.jsonl for deterministic replay
        let timestamp_ms = get_deterministic_unix_timestamp_millis();
        let dataset_filename = format!("dataset-{}.jsonl", timestamp_ms);
        let storage_path = self.storage_root.join(&dataset_filename);

        // Save rows to JSONL format
        self.save_text_rows_to_jsonl(&all_rows, &storage_path)
            .await?;

        // Compute BLAKE3 hash of the file
        let hash_b3 = self.compute_file_hash(&storage_path).await?;

        // Create database record
        let dataset_id = self
            .db
            .create_training_dataset(
                &request.name,
                request.description.as_deref(),
                "jsonl",
                &hash_b3,
                storage_path
                    .to_str()
                    .ok_or_else(|| AosError::Internal("Invalid storage path".to_string()))?,
                request.created_by.as_deref(),
                None,
                Some("ready"),
                Some(&hash_b3),
                None,
            )
            .await?;

        // Store dataset statistics
        self.db
            .store_dataset_statistics(
                &dataset_id,
                all_rows.len() as i32,
                avg_input_length,
                avg_target_length,
                None, // language_distribution
                None, // file_type_distribution
                total_tokens as i64,
            )
            .await?;

        // Update validation status to valid
        self.db
            .update_dataset_validation(&dataset_id, "valid", None, None)
            .await?;

        info!(
            "Training dataset created: {} ({} rows, {} tokens)",
            dataset_id,
            all_rows.len(),
            total_tokens
        );

        Ok(DatasetCreationResult {
            dataset_id,
            num_examples: all_rows.len(),
            total_tokens,
            storage_path: storage_path.to_string_lossy().to_string(),
            hash_b3,
        })
    }

    /// Load training examples from a dataset
    pub async fn load_dataset_examples(&self, dataset_id: &str) -> Result<LoadedDatasetExamples> {
        debug!("Loading training dataset: {}", dataset_id);

        // Get dataset record from database
        let dataset = self
            .db
            .get_training_dataset(dataset_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Dataset not found: {}", dataset_id)))?;

        // Verify dataset is valid
        if dataset.validation_status != "valid" {
            return Err(AosError::Validation(format!(
                "Dataset {} is not validated (status: {})",
                dataset_id, dataset.validation_status
            )));
        }

        // Load examples from JSONL file
        let storage_path = PathBuf::from(&dataset.storage_path);

        // Verify file integrity with BLAKE3 hash
        let actual_hash = self.compute_file_hash(&storage_path).await?;
        if actual_hash != dataset.hash_b3 {
            return Err(AosError::Validation(format!(
                "Dataset {} hash mismatch: expected {}, got {}",
                dataset_id, dataset.hash_b3, actual_hash
            )));
        }

        let (examples, framing_policy) = self
            .load_examples_from_jsonl(&storage_path, dataset_id)
            .await?;

        info!(
            "Loaded {} training examples from dataset {} (hash verified)",
            examples.len(),
            dataset_id
        );

        Ok(LoadedDatasetExamples {
            examples,
            dataset_hash_b3: actual_hash,
            dataset_id: dataset_id.to_string(),
            framing_policy,
        })
    }

    /// Save raw text rows to JSONL format
    async fn save_text_rows_to_jsonl(&self, rows: &[String], path: &Path) -> Result<()> {
        let mut file = File::create(path).await?;

        for text in rows {
            let json = serde_json::to_string(&serde_json::json!({ "text": text }))?;
            file.write_all(json.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.sync_all().await?;

        info!("Saved {} dataset rows to {}", rows.len(), path.display());

        Ok(())
    }

    /// Load training examples from JSONL format
    async fn load_examples_from_jsonl(
        &self,
        path: &Path,
        dataset_id: &str,
    ) -> Result<(Vec<WorkerTrainingExample>, TrainingFramingPolicy)> {
        let content = tokio::fs::read_to_string(path).await?;
        let tokenizer = self.load_text_tokenizer()?;
        let pad_token_id = tokenizer
            .pad_token_id()
            .ok_or_else(|| AosError::Validation("Tokenizer missing pad_token_id".to_string()))?;

        let mut examples = Vec::new();
        let mut schema_mode: Option<TrainingFramingPolicy> = None;
        let created_at = get_deterministic_unix_timestamp_millis() as u64;
        let source_path = path.display().to_string();

        for (line_idx, line) in content.lines().enumerate() {
            let line_number = line_idx + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return Err(AosError::Validation(format!(
                    "Empty JSONL line {} in {}",
                    line_number,
                    path.display()
                )));
            }

            let source_hash = B3Hash::hash(line.as_bytes()).to_hex();
            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                AosError::Validation(format!(
                    "Failed to parse line {} in {}: {}",
                    line_number,
                    path.display(),
                    e
                ))
            })?;
            let obj = value.as_object().ok_or_else(|| {
                AosError::Validation(format!(
                    "Failed to parse line {} in {}: expected JSON object",
                    line_number,
                    path.display()
                ))
            })?;

            let is_supervised =
                obj.len() == 2 && obj.contains_key("prompt") && obj.contains_key("completion");
            let is_raw = obj.len() == 1 && obj.contains_key("text");
            let line_schema = if is_supervised {
                TrainingFramingPolicy::Supervised
            } else if is_raw {
                TrainingFramingPolicy::RawContinuationV1
            } else {
                let keys = obj.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(AosError::Validation(format!(
                    "Unsupported JSONL schema at line {} in {} (fields: {}). Expected {{\"prompt\",\"completion\"}} or {{\"text\"}} only",
                    line_number,
                    path.display(),
                    keys
                )));
            };

            if let Some(active) = schema_mode {
                if active != line_schema {
                    return Err(AosError::Validation(format!(
                        "Mixed JSONL schemas in {}: expected {}, found {} on line {}",
                        path.display(),
                        active.as_str(),
                        line_schema.as_str(),
                        line_number
                    )));
                }
            } else {
                schema_mode = Some(line_schema);
            }

            match line_schema {
                TrainingFramingPolicy::Supervised => {
                    let prompt = obj
                        .get("prompt")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            AosError::Validation(format!(
                                "Line {} in {} has empty prompt",
                                line_number,
                                path.display()
                            ))
                        })?;
                    let completion = obj
                        .get("completion")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            AosError::Validation(format!(
                                "Line {} in {} has empty completion",
                                line_number,
                                path.display()
                            ))
                        })?;

                    let input_tokens = tokenizer.encode(prompt)?;
                    let target_tokens = tokenizer.encode(completion)?;
                    if input_tokens.is_empty() || target_tokens.is_empty() {
                        return Err(AosError::Validation(format!(
                            "Line {} in {} produced empty token sequence",
                            line_number,
                            path.display()
                        )));
                    }

                    let mut provenance = BTreeMap::new();
                    provenance.insert(
                        "schema".to_string(),
                        serde_json::Value::String(line_schema.as_str().to_string()),
                    );
                    provenance.insert(
                        "source_path".to_string(),
                        serde_json::Value::String(source_path.clone()),
                    );
                    provenance.insert(
                        "line_number".to_string(),
                        serde_json::Value::String(line_number.to_string()),
                    );
                    let provenance = provenance_from_map(&provenance).map_err(|e| {
                        AosError::Validation(format!("Failed to serialize provenance: {}", e))
                    })?;
                    let metadata = ExampleMetadataV1::new(
                        dataset_id.to_string(),
                        line_number as u64,
                        source_hash,
                        provenance,
                        created_at,
                    );
                    let attention_mask = WorkerTrainingExample::attention_mask_from_tokens(
                        &input_tokens,
                        pad_token_id,
                    );
                    examples.push(WorkerTrainingExample::new(
                        input_tokens,
                        target_tokens,
                        attention_mask,
                        metadata,
                    ));
                }
                TrainingFramingPolicy::RawContinuationV1 => {
                    let text = obj
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            AosError::Validation(format!(
                                "Line {} in {} has empty text",
                                line_number,
                                path.display()
                            ))
                        })?;
                    let tokens = tokenizer.encode(text)?;
                    if tokens.len() <= MAX_INPUT_TOKENS {
                        tracing::warn!(
                            dataset_id = %dataset_id,
                            line_number,
                            token_count = tokens.len(),
                            "Raw text row too short for continuation framing; dropping row"
                        );
                        continue;
                    }

                    let mut produced = 0usize;
                    let mut start = 0usize;
                    while start < tokens.len() {
                        let input_end = start + MAX_INPUT_TOKENS;
                        if input_end >= tokens.len() {
                            break;
                        }
                        let target_end = input_end + MAX_TARGET_TOKENS;
                        let input_tokens = tokens[start..input_end].to_vec();
                        let target_tokens =
                            tokens[input_end..tokens.len().min(target_end)].to_vec();
                        if input_tokens.is_empty() || target_tokens.is_empty() {
                            break;
                        }

                        let mut provenance = BTreeMap::new();
                        provenance.insert(
                            "schema".to_string(),
                            serde_json::Value::String(line_schema.as_str().to_string()),
                        );
                        provenance.insert(
                            "source_path".to_string(),
                            serde_json::Value::String(source_path.clone()),
                        );
                        provenance.insert(
                            "line_number".to_string(),
                            serde_json::Value::String(line_number.to_string()),
                        );
                        provenance.insert(
                            "chunk_index".to_string(),
                            serde_json::Value::String((produced).to_string()),
                        );
                        let provenance = provenance_from_map(&provenance).map_err(|e| {
                            AosError::Validation(format!("Failed to serialize provenance: {}", e))
                        })?;
                        let metadata = ExampleMetadataV1::new(
                            dataset_id.to_string(),
                            line_number as u64,
                            source_hash.clone(),
                            provenance,
                            created_at,
                        );
                        let attention_mask = WorkerTrainingExample::attention_mask_from_tokens(
                            &input_tokens,
                            pad_token_id,
                        );
                        examples.push(WorkerTrainingExample::new(
                            input_tokens,
                            target_tokens,
                            attention_mask,
                            metadata,
                        ));

                        produced += 1;
                        start = start.saturating_add(STRIDE_TOKENS);
                    }

                    if produced == 0 {
                        tracing::warn!(
                            dataset_id = %dataset_id,
                            line_number,
                            token_count = tokens.len(),
                            "Raw text row produced no training chunks"
                        );
                    }
                }
            }
        }

        let framing_policy = schema_mode.ok_or_else(|| {
            AosError::Validation(format!(
                "Dataset {} contains no valid JSONL entries",
                path.display()
            ))
        })?;

        if examples.is_empty() {
            return Err(AosError::Validation(format!(
                "Dataset {} contains no valid training examples",
                path.display()
            )));
        }

        Ok((examples, framing_policy))
    }

    /// Compute BLAKE3 hash of a file
    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let bytes = tokio::fs::read(path).await?;
        let hash = blake3::hash(&bytes);
        Ok(hash.to_hex().to_string())
    }

    fn load_text_tokenizer(&self) -> Result<QwenTokenizer> {
        let tokenizer_path = self.tokenizer_path.as_ref().ok_or_else(|| {
            AosError::Config("Tokenizer path required; set base_model_path".to_string())
        })?;
        QwenTokenizer::from_file(tokenizer_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::AosError;
    use adapteros_db::sqlx;
    use adapteros_db::Db;
    use adapteros_storage::platform::common::PlatformUtils;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    fn fixture_tokenizer_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/models/tiny-test/tokenizer.json")
    }

    /// Minimal in-memory DB for dataset validation gates (no global migrations)
    async fn minimal_dataset_db() -> Db {
        let db = Db::connect(":memory:").await.unwrap();

        sqlx::query(
            "CREATE TABLE training_datasets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                file_count INTEGER NOT NULL DEFAULT 0,
                total_size_bytes INTEGER NOT NULL DEFAULT 0,
                format TEXT NOT NULL,
                hash_b3 TEXT NOT NULL,
                dataset_hash_b3 TEXT,
                storage_path TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'uploaded' CHECK (status IN ('uploaded','processing','ready','failed')),
                validation_status TEXT NOT NULL DEFAULT 'pending',
                validation_errors TEXT,
                metadata_json TEXT,
                created_by TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                dataset_type TEXT,
                purpose TEXT,
                source_location TEXT,
                collection_method TEXT,
                ownership TEXT,
                tenant_id TEXT,
                workspace_id TEXT,
                hash_needs_recompute INTEGER NOT NULL DEFAULT 0,
                hash_algorithm_version INTEGER NOT NULL DEFAULT 1,
                repo_slug TEXT,
                branch TEXT,
                commit_sha TEXT,
                session_id TEXT,
                session_name TEXT,
                session_tags TEXT,
                scope_repo_id TEXT,
                scope_repo TEXT,
                scope_scan_root TEXT,
                scope_remote_url TEXT,
                scan_root_count INTEGER,
                total_scan_root_files INTEGER,
                total_scan_root_bytes INTEGER,
                scan_roots_content_hash TEXT,
                scan_roots_updated_at TEXT
            )",
        )
        .execute(db.pool())
        .await
        .unwrap();

        db
    }

    #[tokio::test]
    async fn test_save_and_load_examples() {
        let temp_dir = new_test_tempdir();
        let storage_path = temp_dir.path().join("test.jsonl");

        // Create a test database
        let db_path = temp_dir.path().join("test.db");
        let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();

        let tokenizer_path = fixture_tokenizer_path();
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db),
            temp_dir.path().to_path_buf(),
            Some(tokenizer_path.clone()),
        );

        let rows = [
            serde_json::json!({"prompt": "Hello", "completion": "World"}).to_string(),
            serde_json::json!({"prompt": "Good", "completion": "Morning"}).to_string(),
        ];
        tokio::fs::write(&storage_path, format!("{}\n{}\n", rows[0], rows[1]))
            .await
            .unwrap();

        // Load examples
        let (loaded, framing_policy) = manager
            .load_examples_from_jsonl(&storage_path, "ds-test")
            .await
            .unwrap();

        assert_eq!(framing_policy, TrainingFramingPolicy::Supervised);
        assert_eq!(loaded.len(), 2);

        let tokenizer = QwenTokenizer::from_file(&tokenizer_path).unwrap();
        assert_eq!(loaded[0].input_tokens, tokenizer.encode("Hello").unwrap());
        assert_eq!(loaded[0].target_tokens, tokenizer.encode("World").unwrap());
        assert_eq!(loaded[1].input_tokens, tokenizer.encode("Good").unwrap());
        assert_eq!(
            loaded[1].target_tokens,
            tokenizer.encode("Morning").unwrap()
        );
    }

    #[tokio::test]
    async fn test_load_raw_text_examples() {
        let temp_dir = new_test_tempdir();
        let storage_path = temp_dir.path().join("raw.jsonl");

        let db_path = temp_dir.path().join("test.db");
        let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
        let tokenizer_path = fixture_tokenizer_path();
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db),
            temp_dir.path().to_path_buf(),
            Some(tokenizer_path),
        );

        let long_text = "hello ".repeat(600);
        let row = serde_json::json!({"text": long_text}).to_string();
        tokio::fs::write(&storage_path, format!("{}\n", row))
            .await
            .unwrap();

        let (loaded, framing_policy) = manager
            .load_examples_from_jsonl(&storage_path, "ds-raw")
            .await
            .unwrap();

        assert_eq!(framing_policy, TrainingFramingPolicy::RawContinuationV1);
        assert!(!loaded.is_empty());
        for example in &loaded {
            assert_eq!(example.input_tokens.len(), MAX_INPUT_TOKENS);
            assert!(!example.target_tokens.is_empty());
            assert!(example.target_tokens.len() <= MAX_TARGET_TOKENS);
            assert_eq!(example.attention_mask.len(), example.input_tokens.len());
        }
    }

    #[tokio::test]
    async fn dataset_validation_gate_allows_valid_dataset() {
        let temp_dir = new_test_tempdir();
        let dataset_path = temp_dir.path().join("dataset.jsonl");

        let example_json = serde_json::json!({"prompt": "Alpha", "completion": "Beta"}).to_string();
        tokio::fs::write(&dataset_path, format!("{}\n", example_json))
            .await
            .unwrap();

        // Create in-memory DB with minimal schema (skip global migrations)
        let db = minimal_dataset_db().await;
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db.clone()),
            temp_dir.path().to_path_buf(),
            Some(fixture_tokenizer_path()),
        );
        let hash = manager.compute_file_hash(&dataset_path).await.unwrap();

        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, file_count, total_size_bytes, format, hash_b3, dataset_hash_b3,
                storage_path, status, validation_status, validation_errors, metadata_json, created_by,
                workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("ds-valid")
        .bind("Valid Dataset")
        .bind(None::<String>)
        .bind(1)
        .bind(0_i64)
        .bind("jsonl")
        .bind(&hash)
        .bind(&hash)
        .bind(dataset_path.to_string_lossy().to_string())
        .bind("ready")
        .bind("valid")
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .execute(db.pool())
        .await
        .unwrap();

        let loaded = manager
            .load_dataset_examples("ds-valid")
            .await
            .expect("valid dataset should load");

        assert_eq!(loaded.dataset_id, "ds-valid");
        assert_eq!(loaded.framing_policy, TrainingFramingPolicy::Supervised);
        assert_eq!(loaded.examples.len(), 1);
    }

    #[tokio::test]
    async fn dataset_validation_gate_rejects_non_valid_dataset() {
        let temp_dir = new_test_tempdir();
        let dataset_path = temp_dir.path().join("dataset.jsonl");

        let example_json =
            serde_json::json!({"prompt": "Gamma", "completion": "Delta"}).to_string();
        tokio::fs::write(&dataset_path, format!("{}\n", example_json))
            .await
            .unwrap();

        let db = minimal_dataset_db().await;
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db.clone()),
            temp_dir.path().to_path_buf(),
            Some(fixture_tokenizer_path()),
        );
        let hash = manager.compute_file_hash(&dataset_path).await.unwrap();

        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, file_count, total_size_bytes, format, hash_b3, dataset_hash_b3,
                storage_path, status, validation_status, validation_errors, metadata_json, created_by,
                workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("ds-draft")
        .bind("Draft Dataset")
        .bind(None::<String>)
        .bind(1)
        .bind(0_i64)
        .bind("jsonl")
        .bind(&hash)
        .bind(&hash)
        .bind(dataset_path.to_string_lossy().to_string())
        .bind("ready")
        .bind("draft")
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .execute(db.pool())
        .await
        .unwrap();

        let err = manager
            .load_dataset_examples("ds-draft")
            .await
            .expect_err("non-valid datasets must be rejected");

        match err {
            AosError::Validation(msg) => {
                assert!(msg.contains("ds-draft"));
                assert!(msg.contains("draft"));
                assert!(msg.contains("not validated"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }
}
