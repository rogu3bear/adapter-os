use crate::types::DocumentChunk;
use adapteros_core::{AosError, Result};
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Chunking behavior configuration
#[derive(Debug, Clone)]
pub struct ChunkingOptions {
    pub chunk_tokens: usize,
    pub overlap_tokens: usize,
    pub min_chunk_chars: usize,
}

impl Default for ChunkingOptions {
    fn default() -> Self {
        Self {
            chunk_tokens: 512,
            overlap_tokens: 128,
            min_chunk_chars: 160,
        }
    }
}

/// Responsible for splitting normalized text into deterministic chunks
#[derive(Clone)]
pub struct DocumentChunker {
    tokenizer: Option<Arc<Tokenizer>>,
    options: ChunkingOptions,
}

impl DocumentChunker {
    pub fn new(options: ChunkingOptions, tokenizer: Option<Arc<Tokenizer>>) -> Self {
        Self { tokenizer, options }
    }

    pub fn chunk(&self, text: &str, page_number: Option<u32>) -> Result<Vec<DocumentChunk>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let spans = if let Some(tokenizer) = &self.tokenizer {
            self.chunk_with_tokenizer(tokenizer, text)?
        } else {
            self.chunk_by_chars(text)
        };

        let total_chunks = spans.len();
        let mut chunks = Vec::with_capacity(total_chunks);
        for (idx, (start, end)) in spans.into_iter().enumerate() {
            if start >= end || end > text.len() {
                continue;
            }
            let raw = &text[start..end];
            let chunk_text = raw.trim();
            if chunk_text.len() < self.options.min_chunk_chars && !chunks.is_empty() {
                continue;
            }
            if chunk_text.is_empty() {
                continue;
            }
            let span_start = start + raw.find(chunk_text).unwrap_or(0);
            let span_end = span_start + chunk_text.len();
            chunks.push(
                DocumentChunk::new(
                    idx,
                    page_number,
                    span_start,
                    span_end,
                    chunk_text.to_string(),
                )
                .with_total(total_chunks),
            );
        }

        Ok(chunks)
    }

    fn chunk_with_tokenizer(
        &self,
        tokenizer: &Tokenizer,
        text: &str,
    ) -> Result<Vec<(usize, usize)>> {
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| AosError::Validation(format!("Failed to tokenize document: {e}")))?;
        let offsets = encoding.get_offsets();
        if offsets.is_empty() {
            return Ok(vec![(0, text.len())]);
        }

        let target_tokens = self.options.chunk_tokens.max(1);
        let overlap = self
            .options
            .overlap_tokens
            .min(target_tokens.saturating_sub(1));

        let mut spans = Vec::new();
        let mut start_idx = 0usize;
        while start_idx < offsets.len() {
            let end_idx = (start_idx + target_tokens).min(offsets.len());
            let start_byte = offsets[start_idx].0;
            let end_byte = offsets[end_idx - 1].1;
            if end_byte > start_byte {
                spans.push((start_byte, end_byte));
            }

            if end_idx == offsets.len() {
                break;
            }

            let mut next_start = end_idx.saturating_sub(overlap);
            if next_start == start_idx {
                next_start += 1;
            }
            start_idx = next_start;
        }

        Ok(spans)
    }

    fn chunk_by_chars(&self, text: &str) -> Vec<(usize, usize)> {
        let approx_chars_per_token = 4usize;
        let chunk_char_budget = self.options.chunk_tokens * approx_chars_per_token;
        let overlap_char_budget = self.options.overlap_tokens * approx_chars_per_token / 2;

        if chunk_char_budget == 0 {
            return vec![(0, text.len())];
        }

        let mut spans = Vec::new();
        let mut start = 0usize;
        let text_len = text.len();

        while start < text_len {
            let mut end = start.saturating_add(chunk_char_budget);
            if end >= text_len {
                end = text_len;
            } else {
                end = self.advance_to_char_boundary(text, end);
            }
            if end <= start {
                break;
            }

            spans.push((start, end));

            if end == text_len {
                break;
            }

            let mut next_start = end.saturating_sub(overlap_char_budget.max(1));
            next_start = self.advance_to_char_boundary(text, next_start);
            if next_start == start {
                next_start = self.advance_to_char_boundary(text, start + 1);
            }
            start = next_start;
        }

        spans
    }

    fn advance_to_char_boundary(&self, text: &str, mut idx: usize) -> usize {
        if idx >= text.len() {
            return text.len();
        }
        while idx < text.len() && !text.is_char_boundary(idx) {
            idx += 1;
        }
        idx
    }
}
