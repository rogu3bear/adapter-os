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

/// OCR mode for PDF ingestion.
///
/// Default is `Off` to keep ingestion deterministic and avoid external tool
/// dependencies unless explicitly enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    #[default]
    Off,
    External,
}

/// Fingerprint of the OCR toolchain used (or skipped) for ingestion.
///
/// This is intentionally "audit-first": even when OCR is disabled we record a
/// stable skip reason so downstream systems (UI, pipelines) can explain gaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrToolFingerprint {
    pub mode: OcrMode,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_hash_b3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrFingerprint {
    pub mode: OcrMode,
    pub tool: OcrToolFingerprint,
}

/// Provenance metadata stamped on each chunk for traceability.
///
/// Enables any downstream consumer (RAG index, training pipeline, audit)
/// to trace a chunk back to its source file, verify integrity, and know
/// which version of the transform logic produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkProvenance {
    /// BLAKE3 hash of the raw source document bytes.
    pub source_doc_hash: B3Hash,
    /// Filesystem path of the source document (if ingested from disk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Ingestion pipeline version that produced this chunk.
    pub transform_version: u32,
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
    /// Provenance linking this chunk to its source document and transform version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ChunkProvenance>,
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
            provenance: None,
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
    /// OCR fingerprint for PDFs (or None for non-PDF sources).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_fingerprint: Option<OcrFingerprint>,
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
    /// Extraction confidence metadata for the document.
    pub extraction_confidence: ExtractionConfidence,
}

// ============================================================================
// Extraction Confidence (Feature: extraction-confidence)
// ============================================================================

/// Method used to extract text from a document.
///
/// This enum tracks how text was extracted, enabling downstream consumers
/// to understand extraction reliability and adjust trust accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMethod {
    /// Direct text extraction from PDF (lopdf, no OCR)
    #[default]
    TextNative,
    /// OCR via Tesseract (future)
    OcrTesseract,
    /// OCR via Apple Vision framework (future)
    OcrAppleVision,
    /// Mixed extraction: some pages text-native, some OCR
    Mixed,
}

/// Extraction confidence metadata for a document.
///
/// Provides quality signals about how reliably text was extracted.
/// Downstream consumers (training pipelines, RAG) can use this to:
/// - Gate low-quality extractions from training
/// - Surface warnings in UI for degraded documents
/// - Track extraction method for audit trails
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfidence {
    /// Overall text extraction quality score (0.0 - 1.0).
    ///
    /// Scoring formula (v1, deterministic):
    /// - text_score = pages_with_text / total_pages
    /// - Penalized by degraded_pages: each degraded page reduces score
    ///
    /// Interpretation:
    /// - 1.0: All pages extracted successfully with text
    /// - 0.8-1.0: Minor extraction issues, usable
    /// - 0.5-0.8: Significant content may be missing
    /// - < 0.5: Severe extraction problems, review required
    pub text_score: f32,

    /// Method used for extraction.
    pub method: ExtractionMethod,

    /// Page numbers with degraded extraction.
    ///
    /// A page is degraded if:
    /// - `has_unextracted_images = true` (visual content not captured)
    /// - OR extracted text is empty AND page is not blank
    pub degraded_pages: Vec<u32>,

    /// Human-readable reason if text_score < 0.8.
    ///
    /// Examples:
    /// - "3 of 10 pages contain unextracted images"
    /// - "Document appears to be a scanned PDF with no OCR"
    /// - "Pages 2, 5, 8 failed text extraction"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,

    /// Total pages in the document.
    pub total_pages: usize,

    /// Pages with successful text extraction (non-empty text).
    pub pages_with_text: usize,
}

impl ExtractionConfidence {
    /// Compute extraction confidence from page extraction results.
    ///
    /// This is the canonical confidence computation for v1.
    pub fn compute(
        total_pages: usize,
        successful_pages: usize,
        pages_with_images: usize,
        page_errors: &[PageExtractionResult],
    ) -> Self {
        // Find degraded pages: pages with unextracted images or extraction errors
        let mut degraded_pages: Vec<u32> = Vec::new();

        for result in page_errors {
            // Page is degraded if it has unextracted images
            if result.has_unextracted_images && !result.visual_content_extracted {
                degraded_pages.push(result.page_number);
            }
            // Page is degraded if extraction failed with no visual fallback
            if result.text.is_none()
                && result.error.is_some()
                && !degraded_pages.contains(&result.page_number)
            {
                degraded_pages.push(result.page_number);
            }
        }

        // Also count pages with images that weren't in errors
        // (they succeeded but have missing visual content)
        let degraded_from_images = pages_with_images.saturating_sub(degraded_pages.len());

        // Compute text score
        let text_score = if total_pages == 0 {
            0.0
        } else {
            let base_score = successful_pages as f32 / total_pages as f32;
            // Penalize for degraded pages (0.1 penalty per degraded page, capped)
            let penalty = (degraded_pages.len() + degraded_from_images) as f32 * 0.1;
            (base_score - penalty).max(0.0)
        };

        // Generate degradation reason if score < 0.8
        let degradation_reason = if text_score < 0.8 {
            if total_pages > 0 && successful_pages == 0 {
                Some("Document appears to be a scanned PDF with no extractable text".to_string())
            } else if !degraded_pages.is_empty() {
                Some(format!(
                    "{} of {} pages contain unextracted visual content",
                    degraded_pages.len(),
                    total_pages
                ))
            } else if successful_pages < total_pages {
                Some(format!(
                    "{} of {} pages failed text extraction",
                    total_pages - successful_pages,
                    total_pages
                ))
            } else {
                None
            }
        } else {
            None
        };

        Self {
            text_score,
            method: ExtractionMethod::TextNative, // v1: always text-native
            degraded_pages,
            degradation_reason,
            total_pages,
            pages_with_text: successful_pages,
        }
    }

    /// Returns true if extraction quality is acceptable for training.
    ///
    /// Default threshold: 0.8 (configurable in future).
    pub fn is_acceptable(&self) -> bool {
        self.text_score >= 0.8
    }

    /// Returns true if document appears to be scanned (no extractable text).
    pub fn is_scanned_document(&self) -> bool {
        self.total_pages > 0 && self.pages_with_text == 0
    }
}

impl Default for ExtractionConfidence {
    fn default() -> Self {
        Self {
            text_score: 1.0,
            method: ExtractionMethod::TextNative,
            degraded_pages: Vec::new(),
            degradation_reason: None,
            total_pages: 0,
            pages_with_text: 0,
        }
    }
}

/// Result of document extraction with confidence metadata.
///
/// This is the preferred return type for extraction functions,
/// bundling the extracted document with quality signals.
#[derive(Debug)]
pub struct ExtractionResult {
    /// The extracted document with chunks.
    pub document: IngestedDocumentWithErrors,
    /// Extraction confidence metadata.
    pub confidence: ExtractionConfidence,
}
