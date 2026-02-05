use crate::chunker::DocumentChunker;
use crate::types::{DocumentSource, IngestedDocument};
use crate::utils::{finalize_chunks, normalize_whitespace};
use adapteros_core::{AosError, B3Hash, Result};
use std::path::{Path, PathBuf};

const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024; // 5 MiB input cap
const MAX_TEXT_CHARS: usize = 2_000_000; // guard against extreme expansion

pub fn ingest_text_path(path: &Path, chunker: &DocumentChunker) -> Result<IngestedDocument> {
    let bytes = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read text file {}: {e}", path.display())))?;
    let name = crate::source_name_from_path(path);
    ingest_text_bytes(&bytes, &name, Some(path.to_path_buf()), chunker)
}

pub fn ingest_text_bytes(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocument> {
    let doc_hash = B3Hash::hash(bytes);

    if bytes.is_empty() {
        return Err(AosError::Validation(format!(
            "Text document {} is empty",
            source_name
        )));
    }

    if bytes.len() > MAX_TEXT_BYTES {
        return Err(AosError::Validation(format!(
            "Text document {} is too large ({} bytes > {} limit)",
            source_name,
            bytes.len(),
            MAX_TEXT_BYTES
        )));
    }

    let content = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    };

    let normalized = normalize_whitespace(&content);
    if normalized.is_empty() {
        return Err(AosError::Validation(format!(
            "Text document {} contains no renderable text",
            source_name
        )));
    }
    if normalized.chars().count() > MAX_TEXT_CHARS {
        return Err(AosError::Validation(format!(
            "Text document {} exceeds character limit (>{})",
            source_name, MAX_TEXT_CHARS
        )));
    }

    let normalized_text_hash = B3Hash::hash(normalized.as_bytes());
    let normalized_text_len = normalized.chars().count();
    let chunks = finalize_chunks(chunker.chunk(&normalized, None)?);

    Ok(IngestedDocument {
        source: DocumentSource::Text,
        source_name: source_name.to_string(),
        source_path,
        doc_hash,
        normalized_text_hash: Some(normalized_text_hash),
        normalized_text_len: Some(normalized_text_len),
        byte_len: bytes.len(),
        page_count: None,
        chunks,
    })
}
