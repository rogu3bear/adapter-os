use crate::chunker::DocumentChunker;
use crate::types::{DocumentSource, IngestedDocument};
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
