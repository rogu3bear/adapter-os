use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported document source types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentSource {
    Pdf,
    Markdown,
}

impl DocumentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentSource::Pdf => "pdf",
            DocumentSource::Markdown => "markdown",
        }
    }
}

/// A normalized text chunk extracted from a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub page_number: Option<u32>,
    pub start_offset: usize,
    pub end_offset: usize,
    pub text: String,
    pub span_hash: B3Hash,
}

impl DocumentChunk {
    pub fn new(
        chunk_index: usize,
        page_number: Option<u32>,
        start_offset: usize,
        end_offset: usize,
        text: String,
    ) -> Self {
        let span_hash = B3Hash::hash(text.as_bytes());
        Self {
            chunk_index,
            total_chunks: 0,
            page_number,
            start_offset,
            end_offset,
            text,
            span_hash,
        }
    }

    pub fn with_total(mut self, total: usize) -> Self {
        self.total_chunks = total;
        self
    }
}

/// Result of ingesting a single document (pdf/markdown)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedDocument {
    pub source: DocumentSource,
    pub source_name: String,
    pub source_path: Option<PathBuf>,
    pub doc_hash: B3Hash,
    pub byte_len: usize,
    pub page_count: Option<usize>,
    pub chunks: Vec<DocumentChunk>,
}

impl IngestedDocument {
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
