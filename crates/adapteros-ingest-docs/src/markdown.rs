use crate::chunker::DocumentChunker;
use crate::types::{DocumentSource, IngestedDocument};
use crate::utils::{finalize_chunks, normalize_whitespace};
use adapteros_core::{AosError, B3Hash, Result};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::path::{Path, PathBuf};

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
    let markdown = std::str::from_utf8(bytes).map_err(|e| {
        AosError::Validation(format!(
            "Markdown document is not valid UTF-8 ({} bytes): {e}",
            bytes.len()
        ))
    })?;

    let rendered = render_markdown(markdown);
    let normalized = normalize_whitespace(&rendered);
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

fn render_markdown(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut buffer = String::new();

    for event in parser {
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
            }
            Event::End(Tag::Heading(..)) | Event::End(Tag::Paragraph) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
            }
            Event::Start(Tag::List(_)) | Event::End(Tag::List(_)) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
                buffer.push_str("- ");
            }
            Event::End(Tag::Item) => {
                if !buffer.ends_with('\n') {
                    buffer.push('\n');
                }
            }
            _ => {}
        }
    }

    buffer
}
