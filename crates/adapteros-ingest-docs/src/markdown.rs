use crate::chunker::DocumentChunker;
use crate::types::{DocumentSource, IngestedDocument};
use crate::utils::{finalize_chunks, normalize_whitespace};
use adapteros_core::{AosError, B3Hash, Result};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::path::{Path, PathBuf};

const MAX_MARKDOWN_BYTES: usize = 5 * 1024 * 1024; // 5 MiB input cap
const MAX_RENDERED_CHARS: usize = 1_000_000; // guard against runaway expansion
const MAX_MARKDOWN_DEPTH: usize = 64; // nesting protection
const MAX_MARKDOWN_EVENTS: usize = 200_000; // sanity limit on tokens

pub fn ingest_markdown_path(path: &Path, chunker: &DocumentChunker) -> Result<IngestedDocument> {
    let bytes = std::fs::read(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read markdown file {}: {e}",
            path.display()
        ))
    })?;
    let name = crate::source_name_from_path(path);
    ingest_markdown_bytes(&bytes, &name, Some(path.to_path_buf()), chunker)
}

pub fn ingest_markdown_bytes(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocument> {
    let doc_hash = B3Hash::hash(bytes);

    if bytes.is_empty() {
        return Err(AosError::Validation(format!(
            "Markdown document {} is empty",
            source_name
        )));
    }

    if bytes.len() > MAX_MARKDOWN_BYTES {
        return Err(AosError::Validation(format!(
            "Markdown document {} is too large ({} bytes > {} limit)",
            source_name,
            bytes.len(),
            MAX_MARKDOWN_BYTES
        )));
    }

    let markdown = std::str::from_utf8(bytes).map_err(|e| {
        AosError::Validation(format!(
            "Markdown document is not valid UTF-8 ({} bytes): {e}",
            bytes.len()
        ))
    })?;

    let rendered = render_markdown(markdown)?;
    let normalized = normalize_whitespace(&rendered);
    if normalized.is_empty() {
        return Err(AosError::Validation(format!(
            "Markdown document {} contains no renderable text",
            source_name
        )));
    }
    let chunks = finalize_chunks(chunker.chunk(&normalized, None)?);

    Ok(IngestedDocument {
        source: DocumentSource::Markdown,
        source_name: source_name.to_string(),
        source_path,
        doc_hash,
        byte_len: bytes.len(),
        page_count: None,
        chunks,
    })
}

fn render_markdown(markdown: &str) -> Result<String> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut buffer = String::new();
    let mut depth = 0usize;
    let mut event_count = 0usize;

    for event in parser {
        event_count += 1;
        if event_count > MAX_MARKDOWN_EVENTS {
            return Err(AosError::Validation(format!(
                "Markdown rendering exceeded event limit of {}",
                MAX_MARKDOWN_EVENTS
            )));
        }

        match event {
            Event::Text(text) | Event::Code(text) => {
                if !buffer.is_empty() {
                    buffer.push(' ');
                }
                buffer.push_str(text.trim());
            }
            Event::SoftBreak | Event::HardBreak => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
            }
            Event::Start(Tag::Heading(..)) | Event::Start(Tag::Paragraph) => {
                if !buffer.is_empty() && !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                depth += 1;
                if depth > MAX_MARKDOWN_DEPTH {
                    return Err(AosError::Validation(format!(
                        "Markdown nesting exceeds limit of {}",
                        MAX_MARKDOWN_DEPTH
                    )));
                }
            }
            Event::End(Tag::Heading(..)) | Event::End(Tag::Paragraph) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                depth = depth.saturating_sub(1);
            }
            Event::Start(Tag::List(_)) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                depth += 1;
                if depth > MAX_MARKDOWN_DEPTH {
                    return Err(AosError::Validation(format!(
                        "Markdown nesting exceeds limit of {}",
                        MAX_MARKDOWN_DEPTH
                    )));
                }
            }
            Event::End(Tag::List(_)) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                depth = depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                buffer.push_str("- ");
                depth += 1;
                if depth > MAX_MARKDOWN_DEPTH {
                    return Err(AosError::Validation(format!(
                        "Markdown nesting exceeds limit of {}",
                        MAX_MARKDOWN_DEPTH
                    )));
                }
            }
            Event::End(Tag::Item) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }

        if buffer.len() > MAX_RENDERED_CHARS {
            return Err(AosError::Validation(format!(
                "Markdown rendering exceeded character limit ({} > {})",
                buffer.len(),
                MAX_RENDERED_CHARS
            )));
        }
    }

    Ok(buffer)
}
