use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported document source types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentSource {
    Pdf,
    Markdown,
    Text,
}

impl DocumentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentSource::Pdf => "pdf",
            DocumentSource::Markdown => "markdown",
            DocumentSource::Text => "text",
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

/// Result of ingesting a single document (pdf/markdown/text)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedDocument {
    pub source: DocumentSource,
    pub source_name: String,
    pub source_path: Option<PathBuf>,
    pub doc_hash: B3Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text_hash: Option<B3Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text_len: Option<usize>,
    pub byte_len: usize,
    pub page_count: Option<usize>,
    pub chunks: Vec<DocumentChunk>,
}

impl IngestedDocument {
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// Result of extracting text from a single PDF page
#[derive(Debug, Clone)]
pub struct PageExtractionResult {
    pub page_number: u32,
    pub text: Option<String>,
    pub error: Option<String>,
    /// True if this page contains image XObjects (charts, figures, scanned content)
    /// that were not extracted as text. Callers should be aware that visual content
    /// may be missing from the extracted text.
    pub has_unextracted_images: bool,
    /// True if visual content was successfully extracted and described via vision model.
    /// When true, `visual_description` contains the AI-generated description of the
    /// visual content (charts, figures, tables).
    pub visual_content_extracted: bool,
    /// AI-generated description of visual content on this page.
    /// Only populated when `visual_content_extracted` is true.
    pub visual_description: Option<String>,
}

/// Extracted image from a PDF page
#[derive(Debug, Clone)]
pub struct ExtractedImage {
    /// Page number where the image was found
    pub page_number: u32,
    /// Image name/ID within the PDF
    pub image_name: String,
    /// Raw image bytes (decoded to PNG format)
    pub image_bytes: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
}

/// Ingestion result with partial success tracking
#[derive(Debug)]
pub struct IngestedDocumentWithErrors {
    pub document: IngestedDocument,
    pub page_errors: Vec<PageExtractionResult>,
    pub total_pages: usize,
    pub successful_pages: usize,
    /// Number of pages that contain image XObjects (visual content) that could not
    /// be extracted as text. When non-zero, callers should be aware that charts,
    /// figures, or scanned content may be missing from the extracted text.
    pub pages_with_images: usize,
    /// Number of pages where visual content was successfully extracted and described.
    pub pages_with_visual_extraction: usize,
}
