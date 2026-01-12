//! Training dataset integration - connects document ingestion to training pipeline
//!
//! This module provides the bridge between document ingestion and adapter training:
//! 1. Ingest documents (PDF, Markdown, code files)
//! 2. Generate training examples using document ingestion
//! 3. Save examples to JSONL format on disk
//! 4. Create training dataset record in database
//! 5. Link dataset to training jobs

use adapteros_config::resolve_tokenizer_path;
use adapteros_core::{AosError, Result};
use adapteros_db::ProtectedDb;
use adapteros_ingest_docs::{
    default_ingest_options, generate_training_data, load_tokenizer, DocumentIngestor,
    TrainingExample as IngestTrainingExample, TrainingGenConfig, TrainingStrategy,
};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
use adapteros_types::training::{provenance_from_map, ExampleMetadataV1};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// Training dataset manager for creating and managing training datasets
pub struct TrainingDatasetManager {
    db: ProtectedDb,
    storage_root: PathBuf,
    tokenizer_path: Option<PathBuf>,
}

/// Serializable training generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTrainingConfig {
    /// Strategy: "identity" or "question_answer"
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

impl From<SerializableTrainingConfig> for TrainingGenConfig {
    fn from(config: SerializableTrainingConfig) -> Self {
        let strategy = match config.strategy.as_str() {
            "question_answer" => TrainingStrategy::QuestionAnswer,
            _ => TrainingStrategy::Identity,
        };

        TrainingGenConfig {
            strategy,
            max_seq_length: config.max_seq_length,
            add_special_tokens: config.add_special_tokens,
        }
    }
}

/// Request to create a training dataset from ingested documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatasetFromDocumentsRequest {
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
    ) -> Result<(Vec<WorkerTrainingExample>, String, String)> {
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

        let examples = self.load_examples_from_jsonl(&storage_path).await?;

        Ok((examples, actual_hash, version.dataset_id))
    }

    /// Create a training dataset from documents
    pub async fn create_dataset_from_documents(
        &self,
        request: CreateDatasetFromDocumentsRequest,
    ) -> Result<DatasetCreationResult> {
        info!(
            "Creating training dataset '{}' from {} documents",
            request.name,
            request.document_paths.len()
        );

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

        // Convert serializable config to internal config
        let training_config: TrainingGenConfig = request.training_config.into();

        // Ingest all documents and generate training examples
        let mut all_examples = Vec::new();
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

            // Generate training examples from the document
            let training_data =
                generate_training_data(&ingested_doc, &tokenizer, &training_config)?;

            let examples_count = training_data.examples.len();
            let tokens_count: usize = training_data
                .examples
                .iter()
                .map(|ex| ex.input_tokens.len() + ex.target_tokens.len())
                .sum();

            info!(
                "Generated {} examples ({} tokens) from {} | Total so far: {} examples",
                examples_count,
                tokens_count,
                doc_path.display(),
                all_examples.len() + examples_count
            );

            all_examples.extend(training_data.examples);
        }

        if all_examples.is_empty() {
            return Err(AosError::Validation(
                "No training examples generated from documents".to_string(),
            ));
        }

        info!("Total training examples generated: {}", all_examples.len());

        // Calculate statistics
        let total_tokens: usize = all_examples
            .iter()
            .map(|ex| ex.input_tokens.len() + ex.target_tokens.len())
            .sum();

        let avg_input_length = all_examples
            .iter()
            .map(|ex| ex.input_tokens.len())
            .sum::<usize>() as f64
            / all_examples.len() as f64;

        let avg_target_length = all_examples
            .iter()
            .map(|ex| ex.target_tokens.len())
            .sum::<usize>() as f64
            / all_examples.len() as f64;

        // Create storage directory if it doesn't exist
        tokio::fs::create_dir_all(&self.storage_root).await?;

        // Generate unique filename for this dataset
        let dataset_filename = format!("{}.jsonl", uuid::Uuid::now_v7());
        let storage_path = self.storage_root.join(&dataset_filename);

        // Save examples to JSONL format
        self.save_examples_to_jsonl(&all_examples, &storage_path)
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
                all_examples.len() as i32,
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
            "Training dataset created: {} ({} examples, {} tokens)",
            dataset_id,
            all_examples.len(),
            total_tokens
        );

        Ok(DatasetCreationResult {
            dataset_id,
            num_examples: all_examples.len(),
            total_tokens,
            storage_path: storage_path.to_string_lossy().to_string(),
            hash_b3,
        })
    }

    /// Load training examples from a dataset
    pub async fn load_dataset_examples(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<WorkerTrainingExample>> {
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

        let examples = self.load_examples_from_jsonl(&storage_path).await?;

        info!(
            "Loaded {} training examples from dataset {} (hash verified)",
            examples.len(),
            dataset_id
        );

        Ok(examples)
    }

    /// Save training examples to JSONL format
    async fn save_examples_to_jsonl(
        &self,
        examples: &[IngestTrainingExample],
        path: &Path,
    ) -> Result<()> {
        let mut file = File::create(path).await?;

        for example in examples {
            // TrainingExample and WorkerTrainingExample are now the same type (TrainingExampleV1)
            let json = serde_json::to_string(&example)?;
            file.write_all(json.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.sync_all().await?;

        info!(
            "Saved {} training examples to {}",
            examples.len(),
            path.display()
        );

        Ok(())
    }

    /// Load training examples from JSONL format
    async fn load_examples_from_jsonl(&self, path: &Path) -> Result<Vec<WorkerTrainingExample>> {
        let content = tokio::fs::read_to_string(path).await?;

        let mut examples = Vec::new();
        let mut tokenizer: Option<QwenTokenizer> = None;
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(example) = serde_json::from_str::<WorkerTrainingExample>(trimmed) {
                examples.push(example);
                continue;
            }

            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                AosError::Internal(format!(
                    "Failed to parse line {} in {}: {}",
                    line_num + 1,
                    path.display(),
                    e
                ))
            })?;

            let obj = value.as_object().ok_or_else(|| {
                AosError::Internal(format!(
                    "Failed to parse line {} in {}: expected JSON object",
                    line_num + 1,
                    path.display()
                ))
            })?;

            let prompt = obj
                .get("prompt")
                .or_else(|| obj.get("input"))
                .or_else(|| obj.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let response = obj
                .get("response")
                .or_else(|| obj.get("output"))
                .or_else(|| obj.get("completion"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if prompt.trim().is_empty() {
                return Err(AosError::Internal(format!(
                    "Failed to parse line {} in {}: prompt is empty",
                    line_num + 1,
                    path.display()
                )));
            }

            let response = if response.trim().is_empty() {
                prompt
            } else {
                response
            };

            if tokenizer.is_none() {
                tokenizer = Some(self.load_text_tokenizer()?);
            }
            let tokenizer_ref = tokenizer
                .as_ref()
                .ok_or_else(|| AosError::Internal("Failed to initialize tokenizer".to_string()))?;

            let input = tokenizer_ref.encode(prompt)?;
            let target = tokenizer_ref.encode(response)?;
            let pad_token_id = tokenizer_ref
                .pad_token_id()
                .ok_or_else(|| AosError::Internal("Tokenizer missing pad_token_id".to_string()))?;

            if input.is_empty() || target.is_empty() {
                return Err(AosError::Internal(format!(
                    "Failed to parse line {} in {}: empty token sequence",
                    line_num + 1,
                    path.display()
                )));
            }

            let weight = obj
                .get("weight")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(1.0);

            let source_str = path.display().to_string();
            let mut provenance = BTreeMap::new();
            provenance.insert(
                "source_path".to_string(),
                serde_json::Value::String(source_str.clone()),
            );
            if let Some(num) = serde_json::Number::from_f64(weight as f64) {
                provenance.insert("weight".to_string(), serde_json::Value::Number(num));
            } else {
                provenance.insert(
                    "weight".to_string(),
                    serde_json::Value::String(weight.to_string()),
                );
            }
            if let Some(metadata_obj) = obj.get("metadata").and_then(|v| v.as_object()) {
                for (key, value) in metadata_obj {
                    let flat_value = flatten_metadata_value(value);
                    if !flat_value.is_empty() {
                        provenance.insert(key.clone(), serde_json::Value::String(flat_value));
                    }
                }
            }
            let provenance = provenance_from_map(&provenance)
                .map_err(|e| AosError::Internal(format!("Failed to serialize metadata: {}", e)))?;
            let created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let metadata =
                ExampleMetadataV1::new(source_str, line_num as u64, provenance, created_at);
            let attention_mask =
                WorkerTrainingExample::attention_mask_from_tokens(&input, pad_token_id);

            examples.push(WorkerTrainingExample::new(
                input,
                target,
                attention_mask,
                metadata,
            ));
        }

        Ok(examples)
    }

    /// Compute BLAKE3 hash of a file
    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let bytes = tokio::fs::read(path).await?;
        let hash = blake3::hash(&bytes);
        Ok(hash.to_hex().to_string())
    }

    fn load_text_tokenizer(&self) -> Result<QwenTokenizer> {
        let tokenizer_path = match self.tokenizer_path.as_ref() {
            Some(path) => path.clone(),
            None => resolve_tokenizer_path(None)?,
        };
        QwenTokenizer::from_file(tokenizer_path)
    }
}

fn flatten_metadata_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .map(flatten_metadata_value)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::AosError;
    use adapteros_db::sqlx;
    use adapteros_db::Db;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    fn make_example(
        input_tokens: Vec<u32>,
        target_tokens: Vec<u32>,
        row_id: u64,
    ) -> WorkerTrainingExample {
        let metadata = ExampleMetadataV1::new("test", row_id, "{}", 0);
        let attention_mask = WorkerTrainingExample::attention_mask_from_tokens(&input_tokens, 0);
        WorkerTrainingExample::new(input_tokens, target_tokens, attention_mask, metadata)
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

        let manager =
            TrainingDatasetManager::new(ProtectedDb::new(db), temp_dir.path().to_path_buf(), None);

        // Create test examples
        let examples = vec![
            make_example(vec![1, 2, 3], vec![4, 5, 6], 1),
            make_example(vec![7, 8, 9], vec![10, 11, 12], 2),
        ];

        // Save examples
        manager
            .save_examples_to_jsonl(&examples, &storage_path)
            .await
            .unwrap();

        // Load examples
        let loaded = manager
            .load_examples_from_jsonl(&storage_path)
            .await
            .unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].input_tokens, vec![1, 2, 3]);
        assert_eq!(loaded[0].target_tokens, vec![4, 5, 6]);
        assert_eq!(loaded[1].input_tokens, vec![7, 8, 9]);
        assert_eq!(loaded[1].target_tokens, vec![10, 11, 12]);
    }

    #[tokio::test]
    async fn dataset_validation_gate_allows_valid_dataset() {
        let temp_dir = new_test_tempdir();
        let dataset_path = temp_dir.path().join("dataset.jsonl");

        // Prepare on-disk dataset file with a single training example
        let example_json = serde_json::to_string(&make_example(vec![1, 2], vec![3, 4], 1)).unwrap();
        tokio::fs::write(&dataset_path, format!("{}\n", example_json))
            .await
            .unwrap();

        // Create in-memory DB with minimal schema (skip global migrations)
        let db = minimal_dataset_db().await;
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db.clone()),
            temp_dir.path().to_path_buf(),
            None,
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

        let examples = manager
            .load_dataset_examples("ds-valid")
            .await
            .expect("valid dataset should load");

        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].input_tokens, vec![1, 2]);
        assert_eq!(examples[0].target_tokens, vec![3, 4]);
    }

    #[tokio::test]
    async fn dataset_validation_gate_rejects_non_valid_dataset() {
        let temp_dir = new_test_tempdir();
        let dataset_path = temp_dir.path().join("dataset.jsonl");

        let example_json = serde_json::to_string(&make_example(vec![1], vec![2], 1)).unwrap();
        tokio::fs::write(&dataset_path, format!("{}\n", example_json))
            .await
            .unwrap();

        let db = minimal_dataset_db().await;
        let manager = TrainingDatasetManager::new(
            ProtectedDb::new(db.clone()),
            temp_dir.path().to_path_buf(),
            None,
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
