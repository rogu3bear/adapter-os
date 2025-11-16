//! Training dataset integration - connects document ingestion to training pipeline
//!
//! This module provides the bridge between document ingestion and adapter training:
//! 1. Ingest documents (PDF, Markdown, code files)
//! 2. Generate training examples using document ingestion
//! 3. Save examples to JSONL format on disk
//! 4. Create training dataset record in database
//! 5. Link dataset to training jobs

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_ingest_docs::{
    default_ingest_options, generate_training_data, load_tokenizer, DocumentIngestor,
    TrainingExample as IngestTrainingExample, TrainingGenConfig, TrainingStrategy,
};
use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// Training dataset manager for creating and managing training datasets
pub struct TrainingDatasetManager {
    db: Db,
    storage_root: PathBuf,
    tokenizer_path: Option<PathBuf>,
}

/// Serializable training generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTrainingConfig {
    /// Strategy: "identity", "question_answer", or "masked_lm"
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
            "masked_lm" => TrainingStrategy::MaskedLM,
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
    pub fn new(db: Db, storage_root: PathBuf, tokenizer_path: Option<PathBuf>) -> Self {
        Self {
            db,
            storage_root,
            tokenizer_path,
        }
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
                .map(|ex| ex.input.len() + ex.target.len())
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
            .map(|ex| ex.input.len() + ex.target.len())
            .sum();

        let avg_input_length = all_examples.iter().map(|ex| ex.input.len()).sum::<usize>() as f64
            / all_examples.len() as f64;

        let avg_target_length = all_examples.iter().map(|ex| ex.target.len()).sum::<usize>() as f64
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
            .update_dataset_validation(&dataset_id, "valid", None)
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
            // Convert to worker format
            let worker_example = WorkerTrainingExample {
                input: example.input.clone(),
                target: example.target.clone(),
                metadata: example.metadata.clone().unwrap_or_default(),
            };

            let json = serde_json::to_string(&worker_example)?;
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
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let example: WorkerTrainingExample = serde_json::from_str(line).map_err(|e| {
                AosError::Internal(format!(
                    "Failed to parse line {} in {}: {}",
                    line_num + 1,
                    path.display(),
                    e
                ))
            })?;

            examples.push(example);
        }

        Ok(examples)
    }

    /// Compute BLAKE3 hash of a file
    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let bytes = tokio::fs::read(path).await?;
        let hash = blake3::hash(&bytes);
        Ok(hash.to_hex().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_save_and_load_examples() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("test.jsonl");

        // Create a test database
        let db_path = temp_dir.path().join("test.db");
        let db = Db::new(db_path.to_str().unwrap()).await.unwrap();

        let manager = TrainingDatasetManager::new(db, temp_dir.path().to_path_buf(), None);

        // Create test examples
        let examples = vec![
            IngestTrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: Some(std::collections::HashMap::from([(
                    "source".to_string(),
                    "test".to_string(),
                )])),
            },
            IngestTrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: None,
            },
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
        assert_eq!(loaded[0].input, vec![1, 2, 3]);
        assert_eq!(loaded[0].target, vec![4, 5, 6]);
        assert_eq!(loaded[1].input, vec![7, 8, 9]);
        assert_eq!(loaded[1].target, vec![10, 11, 12]);
    }
}
