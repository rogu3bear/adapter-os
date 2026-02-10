//! Document ingestion helpers for adapterOS.
//!
//! Provides deterministic PDF/Markdown parsing, token-aware chunking, and
//! normalized outputs that downstream pipelines can index or convert into
//! training examples.

mod chunker;
pub mod embeddings;
mod markdown;
mod pdf;
pub mod pdf_render;
pub mod rag_integration;
mod text;
pub mod training_gen;
pub mod types;
mod utils;

#[cfg(feature = "ocr-external")]
use adapteros_core::B3Hash;
use adapteros_core::{reject_forbidden_tmp_path, AosError, Result};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

pub use chunker::{ChunkingOptions, DocumentChunker};
pub use embeddings::{
    EmbeddingModel, ProductionEmbeddingModel, SimpleEmbeddingModel, EMBEDDING_DIMENSION,
};
pub use rag_integration::{
    generate_revision, index_document_with_provenance, prepare_document_for_rag,
    prepare_documents_for_rag, RagChunkParams,
};
pub const INGESTION_VERSION: u32 = 2;
pub use training_gen::{
    generate_training_data, generate_training_data_from_documents, TrainingData, TrainingExample,
    TrainingGenConfig, TrainingStrategy,
};
pub use types::{
    ChunkProvenance, DocumentChunk, DocumentSource, ExtractedImage, IngestedDocument,
    IngestedDocumentWithErrors, OcrFingerprint, OcrMode, OcrToolFingerprint, PageExtractionResult,
};

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

    /// Ingest PDF bytes and attach an OCR fingerprint.
    ///
    /// OCR is **off by default** and must be explicitly requested. This method
    /// does not guarantee OCR is performed; it always records an audit-grade
    /// fingerprint describing whether OCR was skipped and why.
    pub fn ingest_pdf_bytes_with_ocr<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        source_name: &str,
        mode: types::OcrMode,
        ocr_artifacts_root: Option<&Path>,
    ) -> Result<IngestedDocument> {
        if let Some(root) = ocr_artifacts_root {
            // Enforce AdapterOS path hygiene: no persistent writes under /tmp.
            reject_forbidden_tmp_path(root, "ocr_artifacts_root")?;
        }

        let mut doc = pdf::ingest_pdf_bytes(bytes.as_ref(), source_name, None, &self.chunker)?;
        doc.ocr_fingerprint = Some(build_pdf_ocr_fingerprint(mode, ocr_artifacts_root));
        Ok(doc)
    }

    pub fn ingest_pdf_bytes_resilient<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        source_name: &str,
    ) -> Result<IngestedDocumentWithErrors> {
        pdf::ingest_pdf_bytes_resilient(bytes.as_ref(), source_name, None, &self.chunker)
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

    pub fn ingest_text_path<P: AsRef<Path>>(&self, path: P) -> Result<IngestedDocument> {
        text::ingest_text_path(path.as_ref(), &self.chunker)
    }

    pub fn ingest_text_bytes<B: AsRef<[u8]>>(
        &self,
        bytes: B,
        source_name: &str,
    ) -> Result<IngestedDocument> {
        text::ingest_text_bytes(bytes.as_ref(), source_name, None, &self.chunker)
    }
}

fn build_pdf_ocr_fingerprint(
    mode: types::OcrMode,
    _ocr_artifacts_root: Option<&Path>,
) -> types::OcrFingerprint {
    use types::{OcrFingerprint, OcrMode, OcrToolFingerprint};

    match mode {
        OcrMode::Off => OcrFingerprint {
            mode,
            tool: OcrToolFingerprint {
                mode,
                command: "tesseract".to_string(),
                version: None,
                binary_path: None,
                binary_hash_b3: None,
                skipped_reason: Some("mode_off".to_string()),
                args: Vec::new(),
            },
        },
        OcrMode::External => {
            #[cfg(not(feature = "ocr-external"))]
            {
                OcrFingerprint {
                    mode,
                    tool: OcrToolFingerprint {
                        mode,
                        command: "tesseract".to_string(),
                        version: None,
                        binary_path: None,
                        binary_hash_b3: None,
                        skipped_reason: Some("ocr_external_feature_not_enabled".to_string()),
                        args: Vec::new(),
                    },
                }
            }

            #[cfg(feature = "ocr-external")]
            {
                let mut tool = OcrToolFingerprint {
                    mode,
                    command: "tesseract".to_string(),
                    version: None,
                    binary_path: None,
                    binary_hash_b3: None,
                    skipped_reason: None,
                    args: Vec::new(),
                };

                match find_executable_in_path("tesseract") {
                    None => {
                        tool.skipped_reason = Some("tesseract_not_found".to_string());
                    }
                    Some(path) => {
                        tool.binary_path = Some(path.to_string_lossy().to_string());

                        if let Ok(bytes) = std::fs::read(&path) {
                            tool.binary_hash_b3 = Some(B3Hash::hash(&bytes).to_hex());
                        }

                        tool.version = read_tool_version(&path, &["--version"]);
                    }
                }

                OcrFingerprint { mode, tool }
            }
        }
    }
}

#[cfg(feature = "ocr-external")]
fn find_executable_in_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(feature = "ocr-external")]
fn read_tool_version(bin: &std::path::Path, args: &[&str]) -> Option<String> {
    use std::process::Command;

    let output = Command::new(bin).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        None
    } else {
        Some(first_line.to_string())
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
