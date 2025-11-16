//! Document ingestion helpers for AdapterOS.
//!
//! Provides deterministic PDF/Markdown parsing, token-aware chunking, and
//! normalized outputs that downstream pipelines can index or convert into
//! training examples.

mod chunker;
pub mod embeddings;
mod markdown;
mod pdf;
pub mod rag_integration;
pub mod training_gen;
pub mod types;
mod utils;

use adapteros_core::{AosError, Result};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

pub use chunker::{ChunkingOptions, DocumentChunker};
pub use embeddings::{
    EmbeddingModel, ProductionEmbeddingModel, SimpleEmbeddingModel, EMBEDDING_DIMENSION,
};
pub use rag_integration::{
    generate_revision, prepare_document_for_rag, prepare_documents_for_rag, RagChunkParams,
};
pub use training_gen::{
    generate_training_data, generate_training_data_from_documents, TrainingData, TrainingExample,
    TrainingGenConfig, TrainingStrategy,
};
pub use types::{DocumentChunk, DocumentSource, IngestedDocument};

/// High level entrypoint for document ingestion.
#[derive(Clone)]
pub struct DocumentIngestor {
    chunker: DocumentChunker,
}

impl DocumentIngestor {
    pub fn new(options: ChunkingOptions, tokenizer: Option<Arc<Tokenizer>>) -> Self {
        Self {
            chunker: DocumentChunker::new(options, tokenizer),
        }
    }

    pub fn ingest_pdf_path<P: AsRef<Path>>(&self, path: P) -> Result<IngestedDocument> {
        pdf::ingest_pdf_path(path.as_ref(), &self.chunker)
    }

    pub fn ingest_pdf_bytes<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        source_name: &str,
    ) -> Result<IngestedDocument> {
        pdf::ingest_pdf_bytes(bytes.as_ref(), source_name, None, &self.chunker)
    }

    pub fn ingest_markdown_path<P: AsRef<Path>>(&self, path: P) -> Result<IngestedDocument> {
        markdown::ingest_markdown_path(path.as_ref(), &self.chunker)
    }

    pub fn ingest_markdown_bytes<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        source_name: &str,
    ) -> Result<IngestedDocument> {
        markdown::ingest_markdown_bytes(bytes.as_ref(), source_name, None, &self.chunker)
    }
}

/// Utility for loading a tokenizer from disk (optional dependency for chunking)
pub fn load_tokenizer(path: &Path) -> Result<Arc<Tokenizer>> {
    let tokenizer = Tokenizer::from_file(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to load tokenizer from {}: {e}",
            path.display()
        ))
    })?;
    Ok(Arc::new(tokenizer))
}

/// Helper for building chunker options tailored for embeddings
pub fn default_ingest_options() -> ChunkingOptions {
    ChunkingOptions::default()
}

/// Normalize filesystem names for logging/metadata
fn source_name_from_path(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string())
}
