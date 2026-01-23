//! Document ingestion command
//!
//! Ingests PDF and Markdown documents for RAG indexing and/or adapter training.

use crate::commands::training_common::TokenizerArg;
use adapteros_core::{AosError, Result};
use adapteros_ingest_docs::{
    generate_revision, generate_training_data_from_documents, load_tokenizer,
    prepare_documents_for_rag, ChunkingOptions, DocumentIngestor, EmbeddingModel,
    ProductionEmbeddingModel, TrainingGenConfig, TrainingStrategy, EMBEDDING_DIMENSION,
};
use adapteros_lora_rag::pgvector::PgVectorIndex;
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

/// Ingest documents for RAG and/or training
#[derive(Args, Debug)]
pub struct IngestDocsArgs {
    /// Input files (PDFs or Markdown)
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Tenant ID for RAG indexing
    #[arg(short, long)]
    tenant: Option<String>,

    /// Index documents in RAG vector store
    #[arg(long)]
    index_rag: bool,

    /// Generate training data from documents
    #[arg(long)]
    generate_training: bool,

    /// Output path for training data (JSONL format)
    #[arg(long)]
    training_output: Option<PathBuf>,

    /// Training strategy: identity or qa
    #[arg(long, default_value = "identity")]
    training_strategy: String,

    /// Maximum sequence length for training examples
    #[arg(long, default_value = "512")]
    max_seq_length: usize,

    /// Chunk size in tokens
    #[arg(long, default_value = "512")]
    chunk_tokens: usize,

    /// Overlap size in tokens
    #[arg(long, default_value = "128")]
    overlap_tokens: usize,

    /// Database URL for RAG indexing (SQLite)
    #[arg(long)]
    db_url: Option<String>,

    /// Document revision (defaults to auto-generated timestamp)
    #[arg(long)]
    rev: Option<String>,

    /// Path to embedding model (sentence-transformer format)
    /// If not provided, falls back to simple feature-based embeddings
    #[arg(long)]
    embedding_model: Option<PathBuf>,

    /// Tokenizer configuration
    #[command(flatten)]
    tokenizer_arg: TokenizerArg,
}

impl IngestDocsArgs {
    pub async fn execute(&self) -> Result<()> {
        info!("Starting document ingestion for {} files", self.files.len());

        // Validate args
        if !self.index_rag && !self.generate_training {
            return Err(AosError::Validation(
                "At least one of --index-rag or --generate-training must be specified".to_string(),
            ));
        }

        if self.generate_training && self.training_output.is_none() {
            return Err(AosError::Validation(
                "--training-output is required when --generate-training is enabled".to_string(),
            ));
        }

        if self.index_rag && self.tenant.is_none() {
            return Err(AosError::Validation(
                "--tenant is required when --index-rag is enabled".to_string(),
            ));
        }
        if self.generate_training && !self.training_strategy.eq_ignore_ascii_case("identity") {
            return Err(AosError::Validation(
                "Only training_strategy=identity is supported by PLAN_4".to_string(),
            ));
        }

        // Resolve and load tokenizer
        let tokenizer_path =
            adapteros_config::resolve_tokenizer_path(self.tokenizer_arg.tokenizer.as_ref())?;
        info!("Loading tokenizer from {}", tokenizer_path.display());
        let tokenizer = load_tokenizer(&tokenizer_path)?;

        // Create document ingestor
        let chunking_options = ChunkingOptions {
            chunk_tokens: self.chunk_tokens,
            overlap_tokens: self.overlap_tokens,
            min_chunk_chars: 160,
        };
        let ingestor = DocumentIngestor::new(chunking_options, Some(tokenizer.clone()));

        // Ingest all documents
        let mut ingested_docs = Vec::new();
        let mut failed_docs: Vec<(PathBuf, String)> = Vec::new();
        for file_path in &self.files {
            info!("Ingesting file: {}", file_path.display());

            if !file_path.exists() {
                warn!("File not found, skipping: {}", file_path.display());
                continue;
            }

            match self.ingest_file(&ingestor, file_path) {
                Ok(document) => {
                    info!(
                        "Ingested {} with {} chunks",
                        document.source_name,
                        document.chunk_count()
                    );
                    ingested_docs.push(document);
                }
                Err(e) => {
                    warn!(
                        path = %file_path.display(),
                        error = %e,
                        "Failed to ingest file, continuing with remaining inputs"
                    );
                    failed_docs.push((file_path.clone(), e.to_string()));
                    continue;
                }
            }
        }

        if !failed_docs.is_empty() {
            for (path, error) in &failed_docs {
                warn!(path = %path.display(), %error, "Document ingestion failed");
            }
        }

        if ingested_docs.is_empty() {
            warn!("No documents were successfully ingested");
            return Ok(());
        }

        info!("Successfully ingested {} documents", ingested_docs.len());

        // Index in RAG if requested
        if self.index_rag {
            self.index_documents_in_rag(&ingested_docs, &tokenizer)
                .await?;
        }

        // Generate training data if requested
        if self.generate_training {
            self.generate_training_data(&ingested_docs, &tokenizer)?;
        }

        info!("Document ingestion complete");
        Ok(())
    }

    fn ingest_file(
        &self,
        ingestor: &DocumentIngestor,
        file_path: &Path,
    ) -> Result<adapteros_ingest_docs::IngestedDocument> {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension.to_lowercase().as_str() {
            "pdf" => ingestor.ingest_pdf_path(file_path),
            "md" | "markdown" => ingestor.ingest_markdown_path(file_path),
            _ => Err(AosError::Validation(format!(
                "Unsupported file type: {}. Only .pdf, .md, and .markdown are supported",
                extension
            ))),
        }
    }

    async fn index_documents_in_rag(
        &self,
        documents: &[adapteros_ingest_docs::IngestedDocument],
        tokenizer: &Arc<tokenizers::Tokenizer>,
    ) -> Result<()> {
        info!("Indexing documents in RAG vector store");

        let tenant_id = self.tenant.as_ref().ok_or_else(|| {
            AosError::Validation("--tenant is required for RAG indexing".to_string())
        })?;

        let embedding_resolution = adapteros_config::resolve_embedding_model_path_with_override(
            self.embedding_model.as_deref(),
        )?;
        let embedding_model_path = embedding_resolution.path;
        let tokenizer_path = embedding_model_path.join("tokenizer.json");
        info!(
            path = %embedding_model_path.display(),
            tokenizer_path = %tokenizer_path.display(),
            source = %embedding_resolution.source,
            "Resolved embedding model path for RAG ingest"
        );

        // Create embedding model - use ProductionEmbeddingModel which will try MLX first
        let embedding_model: Arc<dyn EmbeddingModel> = Arc::new(ProductionEmbeddingModel::load(
            Some(&embedding_model_path),
            tokenizer.clone(),
        ));

        info!(
            "Using embedding model with dimension={}, hash={}",
            embedding_model.dimension(),
            embedding_model.model_hash().to_hex()
        );

        // Prepare documents for RAG
        let default_rev = generate_revision();
        let rev = self.rev.as_deref().unwrap_or(&default_rev);
        let rag_params =
            prepare_documents_for_rag(tenant_id, documents, &embedding_model, Some(rev)).await?;

        info!("Prepared {} chunks for RAG indexing", rag_params.len());

        // Get the model hash for database index
        let model_hash = embedding_model.model_hash();

        // Connect to database and insert
        if let Some(db_url) = &self.db_url {
            self.insert_into_database(db_url, &rag_params, model_hash)
                .await?;
        } else {
            warn!("No database URL provided, skipping actual database insertion");
            info!("To index documents, provide --db-url with SQLite connection string");
        }

        Ok(())
    }

    async fn insert_into_database(
        &self,
        db_url: &str,
        rag_params: &[adapteros_ingest_docs::RagChunkParams],
        embedding_hash: adapteros_core::B3Hash,
    ) -> Result<()> {
        info!("Connecting to database: {}", db_url);

        // Determine database type from URL (SQLite only)
        let is_sqlite =
            db_url.starts_with("sqlite://") || db_url.contains(".db") || db_url.contains(".sqlite");

        if is_sqlite {
            self.insert_into_sqlite(db_url, rag_params, embedding_hash)
                .await
        } else {
            Err(AosError::Config(
                "Database URL must be SQLite (sqlite:// or .db file)".to_string(),
            ))
        }
    }

    async fn insert_into_sqlite(
        &self,
        db_url: &str,
        rag_params: &[adapteros_ingest_docs::RagChunkParams],
        embedding_hash: adapteros_core::B3Hash,
    ) -> Result<()> {
        use sqlx::sqlite::SqlitePool;

        // Normalize SQLite URL
        let url = if db_url.starts_with("sqlite://") {
            db_url.to_string()
        } else {
            format!("sqlite://{}", db_url)
        };

        let pool = SqlitePool::connect(&url)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to SQLite: {}", e)))?;

        // Create PgVectorIndex with SQLite backend and actual embedding model hash
        let index = PgVectorIndex::new_sqlite(pool, embedding_hash, EMBEDDING_DIMENSION);

        // Insert each chunk
        for params in rag_params {
            index
                .add_document(
                    &params.tenant_id,
                    params.doc_id.clone(),
                    params.text.clone(),
                    params.embedding.clone(),
                    params.rev.clone(),
                    params.effectivity.clone(),
                    params.source_type.clone(),
                    None, // superseded_by
                )
                .await?;
        }

        info!("Successfully indexed {} chunks in SQLite", rag_params.len());
        Ok(())
    }

    fn generate_training_data(
        &self,
        documents: &[adapteros_ingest_docs::IngestedDocument],
        tokenizer: &Arc<tokenizers::Tokenizer>,
    ) -> Result<()> {
        info!("Generating training data from documents");

        let strategy = match self.training_strategy.to_lowercase().as_str() {
            "identity" => TrainingStrategy::Identity,
            "qa" | "question-answer" => TrainingStrategy::QuestionAnswer,
            _ => {
                return Err(AosError::Validation(format!(
                    "Invalid training strategy: {}. Must be one of: identity, qa",
                    self.training_strategy
                )));
            }
        };

        let config = TrainingGenConfig {
            strategy,
            max_seq_length: self.max_seq_length,
            add_special_tokens: true,
        };

        let training_data = generate_training_data_from_documents(documents, tokenizer, &config)?;

        // Write to output file
        let output_path = self.training_output.as_ref().ok_or_else(|| {
            AosError::Validation(
                "--training-output is required for training data generation".to_string(),
            )
        })?;
        let json = serde_json::to_string_pretty(&training_data).map_err(AosError::Serialization)?;

        fs::write(output_path, json).map_err(|e| {
            AosError::Io(format!(
                "Failed to write training data to {}: {}",
                output_path.display(),
                e
            ))
        })?;

        info!(
            "Generated {} training examples, written to {}",
            training_data.examples.len(),
            output_path.display()
        );

        Ok(())
    }
}
