use crate::chunker::DocumentChunker;
use crate::types::{
    DocumentSource, IngestedDocument, IngestedDocumentWithErrors, PageExtractionResult,
};
use crate::utils::{finalize_chunks, normalize_whitespace};
use adapteros_core::{AosError, B3Hash, Result};
use lopdf::Document;
use std::path::{Path, PathBuf};

pub fn ingest_pdf_path(path: &Path, chunker: &DocumentChunker) -> Result<IngestedDocument> {
    let bytes = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read PDF file {}: {e}", path.display())))?;
    let name = crate::source_name_from_path(path);
    ingest_pdf_bytes(&bytes, &name, Some(path.to_path_buf()), chunker)
}

pub fn ingest_pdf_bytes(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocument> {
    let doc_hash = B3Hash::hash(bytes);
    let mut document = Document::load_mem(bytes)
        .map_err(|e| AosError::Validation(format!("Failed to parse PDF {source_name}: {e}")))?;

    if document.is_encrypted() {
        document
            .decrypt("")
            .map_err(|_| AosError::Validation("Encrypted PDFs are not supported".to_string()))?;
    }

    let pages = document.get_pages();
    if pages.is_empty() {
        return Err(AosError::Validation(format!(
            "PDF {source_name} contains no pages"
        )));
    }

    let mut all_chunks = Vec::new();
    for (page_number, _object_id) in pages.iter() {
        let text = document.extract_text(&[*page_number]).map_err(|e| {
            AosError::Validation(format!(
                "Failed to extract text from page {} of {}: {e}",
                page_number, source_name
            ))
        })?;
        let normalized = normalize_whitespace(&text);
        if normalized.trim().is_empty() {
            continue;
        }
        let mut page_chunks = chunker.chunk(&normalized, Some(*page_number))?;
        all_chunks.append(&mut page_chunks);
    }

    let chunks = finalize_chunks(all_chunks);

    Ok(IngestedDocument {
        source: DocumentSource::Pdf,
        source_name: source_name.to_string(),
        source_path,
        doc_hash,
        byte_len: bytes.len(),
        page_count: Some(pages.len()),
        chunks,
    })
}

/// Ingest PDF with per-page error recovery.
/// Unlike `ingest_pdf_bytes`, this continues processing even if some pages fail.
pub fn ingest_pdf_bytes_resilient(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocumentWithErrors> {
    let doc_hash = B3Hash::hash(bytes);
    let mut document = Document::load_mem(bytes)
        .map_err(|e| AosError::Validation(format!("Failed to parse PDF {}: {}", source_name, e)))?;

    // Handle encryption
    if document.is_encrypted() {
        document.decrypt("").map_err(|_| {
            AosError::Validation(format!("Encrypted PDF {} requires password", source_name))
        })?;
    }

    let pages = document.get_pages();
    if pages.is_empty() {
        return Err(AosError::Validation(format!(
            "PDF {} contains no pages",
            source_name
        )));
    }

    let total_pages = pages.len();
    let mut all_chunks = Vec::new();
    let mut page_errors = Vec::new();
    let mut successful_pages = 0;

    for (page_number, _object_id) in pages.iter() {
        match document.extract_text(&[*page_number]) {
            Ok(text) => {
                let normalized = normalize_whitespace(&text);
                if normalized.trim().is_empty() {
                    // Empty page - not an error, just skip
                    page_errors.push(PageExtractionResult {
                        page_number: *page_number,
                        text: None,
                        error: Some("Empty page".to_string()),
                    });
                    continue;
                }

                match chunker.chunk(&normalized, Some(*page_number)) {
                    Ok(mut page_chunks) => {
                        all_chunks.append(&mut page_chunks);
                        successful_pages += 1;
                        page_errors.push(PageExtractionResult {
                            page_number: *page_number,
                            text: Some(normalized),
                            error: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            page = page_number,
                            error = %e,
                            source = source_name,
                            "Failed to chunk page content, skipping"
                        );
                        page_errors.push(PageExtractionResult {
                            page_number: *page_number,
                            text: Some(normalized),
                            error: Some(format!("Chunking failed: {}", e)),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    page = page_number,
                    error = %e,
                    source = source_name,
                    "Failed to extract text from page, skipping"
                );
                page_errors.push(PageExtractionResult {
                    page_number: *page_number,
                    text: None,
                    error: Some(format!("Text extraction failed: {}", e)),
                });
            }
        }
    }

    // Require at least one successful page
    if successful_pages == 0 {
        return Err(AosError::Validation(format!(
            "No pages could be extracted from PDF {}",
            source_name
        )));
    }

    // Log summary
    if successful_pages < total_pages {
        tracing::info!(
            source = source_name,
            total_pages = total_pages,
            successful_pages = successful_pages,
            failed_pages = total_pages - successful_pages,
            "PDF partially processed with some page failures"
        );
    }

    let chunks = finalize_chunks(all_chunks);

    Ok(IngestedDocumentWithErrors {
        document: IngestedDocument {
            source: DocumentSource::Pdf,
            source_name: source_name.to_string(),
            source_path,
            doc_hash,
            byte_len: bytes.len(),
            page_count: Some(total_pages),
            chunks,
        },
        page_errors,
        total_pages,
        successful_pages,
    })
}
