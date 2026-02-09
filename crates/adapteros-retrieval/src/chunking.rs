//! Hybrid chunking strategies (token + semantic)
//!
//! This module provides chunking strategies for both documents and code:
//! - Token-based chunking for documents (configurable size and overlap)
//! - Semantic chunking for code (function/class boundary detection)
//! - Auto-detection based on file extension

use crate::corpus::{Chunk, ChunkType, ChunkingConfig};
use adapteros_core::error::Result;
use std::path::Path;

// Token estimation constants removed; now using configurable config.chars_per_token

/// Chunk a document into token-based chunks with overlap.
///
/// Uses character approximation (4 chars ≈ 1 token) to estimate token boundaries.
/// Chunks are created with configurable size and overlap from `ChunkingConfig`.
///
/// # Arguments
/// * `content` - The document content to chunk
/// * `source_path` - Path to the source file (used for chunk IDs)
/// * `config` - Chunking configuration with token_chunk_size and token_overlap
///
/// # Returns
/// A vector of `Chunk` with deterministic IDs based on source path and offsets
pub fn chunk_document(
    content: &str,
    source_path: &str,
    config: &ChunkingConfig,
) -> Result<Vec<Chunk>> {
    if content.is_empty() {
        return Ok(vec![]);
    }

    // Convert token counts to character counts using configurable heuristic
    let chunk_chars = (config.token_chunk_size as f32 * config.chars_per_token).round() as usize;
    let overlap_chars = (config.token_overlap as f32 * config.chars_per_token).round() as usize;

    // Ensure we have valid step size
    let step = chunk_chars.saturating_sub(overlap_chars).max(1);

    let mut chunks = Vec::new();
    let content_len = content.len();
    let mut start = 0;

    while start < content_len {
        let end = (start + chunk_chars).min(content_len);

        // Try to break at the best semantic boundary if not at end
        let actual_end = if end < content_len {
            find_best_boundary(content, end)
        } else {
            end
        };

        let chunk_content = &content[start..actual_end];

        // Detect format from extension or default to plain
        let format = detect_document_format(source_path);

        chunks.push(Chunk::new(
            source_path.to_string(),
            chunk_content.to_string(),
            start,
            actual_end,
            ChunkType::Document { format },
        ));

        // Move to next chunk position
        start += step;

        // Avoid creating tiny trailing chunks
        // If we have less than 1/8th of a chunk left, include it in the last one
        if start + (chunk_chars / 8) >= content_len && start < content_len {
            break;
        }
    }

    Ok(chunks)
}

/// Chunk code using semantic boundaries (function/class detection).
///
/// This implementation uses regex-based detection to identify function and class
/// boundaries. For production use, tree-sitter integration would provide more
/// accurate parsing.
///
/// # Arguments
/// * `content` - The code content to chunk
/// * `source_path` - Path to the source file
/// * `language` - Programming language (e.g., "rs", "py", "js")
/// * `config` - Chunking configuration with code_target_size and code_max_size
///
/// # Returns
/// A vector of `Chunk` with semantic type annotations
pub fn chunk_code(
    content: &str,
    source_path: &str,
    language: &str,
    config: &ChunkingConfig,
) -> Result<Vec<Chunk>> {
    if content.is_empty() {
        return Ok(vec![]);
    }

    // Find semantic boundaries based on language
    let boundaries = find_code_boundaries(content, language);

    if boundaries.is_empty() {
        // No semantic boundaries found - fall back to line-based chunking
        return chunk_code_by_lines(content, source_path, language, config);
    }

    let mut chunks = Vec::new();
    let mut current_start = 0;
    let mut current_type = "module".to_string();

    for boundary in &boundaries {
        // Create chunk for content before this boundary if non-trivial
        if boundary.start > current_start {
            let segment = &content[current_start..boundary.start];
            if !segment.trim().is_empty() {
                // Check if segment exceeds max size
                if segment.len() > config.code_max_size {
                    // Split oversized segment by lines
                    let sub_chunks = split_by_size(
                        segment,
                        source_path,
                        language,
                        &current_type,
                        current_start,
                        config,
                    );
                    chunks.extend(sub_chunks);
                } else {
                    chunks.push(Chunk::new(
                        source_path.to_string(),
                        segment.to_string(),
                        current_start,
                        boundary.start,
                        ChunkType::Code {
                            language: language.to_string(),
                            semantic_type: current_type.clone(),
                        },
                    ));
                }
            }
        }

        // Update tracking
        current_start = boundary.start;
        current_type = boundary.semantic_type.clone();
    }

    // Handle remaining content after last boundary
    if current_start < content.len() {
        let segment = &content[current_start..];
        if !segment.trim().is_empty() {
            if segment.len() > config.code_max_size {
                let sub_chunks = split_by_size(
                    segment,
                    source_path,
                    language,
                    &current_type,
                    current_start,
                    config,
                );
                chunks.extend(sub_chunks);
            } else {
                chunks.push(Chunk::new(
                    source_path.to_string(),
                    segment.to_string(),
                    current_start,
                    content.len(),
                    ChunkType::Code {
                        language: language.to_string(),
                        semantic_type: current_type,
                    },
                ));
            }
        }
    }

    // Merge small adjacent chunks of the same type
    let merged = merge_small_chunks(chunks, config.code_target_size);

    Ok(merged)
}

/// Auto-detect file type and chunk appropriately.
///
/// Examines the file extension to determine whether to use document or code
/// chunking strategies.
///
/// # Arguments
/// * `content` - The file content to chunk
/// * `source_path` - Path to the source file
/// * `config` - Chunking configuration
///
/// # Returns
/// A vector of `Chunk` with appropriate types based on file extension
pub fn chunk_file(content: &str, source_path: &str, config: &ChunkingConfig) -> Result<Vec<Chunk>> {
    let ext = Path::new(source_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        // Code files
        "rs" | "py" | "js" | "ts" | "go" | "java" | "cpp" | "c" | "h" | "hpp" | "rb" | "php"
        | "swift" | "kt" | "scala" | "zig" | "hs" | "ml" | "ex" | "exs" | "clj" | "lua" => {
            chunk_code(content, source_path, ext, config)
        }
        // Document files
        _ => chunk_document(content, source_path, config),
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Find the nearest word boundary at or before the given position.
/// Find the best semantic boundary at or before the given position.
///
/// Preference order:
/// 1. Paragraph (double newline)
/// 2. Code block boundaries or significant list items (newline)
/// 3. Sentence boundaries (. ! ?)
/// 4. Word boundaries (whitespace)
fn find_best_boundary(content: &str, pos: usize) -> usize {
    if pos >= content.len() {
        return content.len();
    }

    // Context look-back window size
    let window = 120;
    let search_start = pos.saturating_sub(window);
    let search_area = &content[search_start..=pos];
    // Don't prefer structural boundaries that are far away from the target position,
    // otherwise a single early paragraph break can dominate all later chunking.
    let max_structural_distance = 24;

    // 1. Try paragraph boundary (\n\n)
    if let Some(offset) = search_area.rfind("\n\n") {
        let boundary = search_start + offset + 2;
        if pos.saturating_sub(boundary) <= max_structural_distance {
            return boundary;
        }
    }

    // 2. Try single newline (at least preserves line context)
    if let Some(offset) = search_area.rfind('\n') {
        let boundary = search_start + offset + 1;
        if pos.saturating_sub(boundary) <= max_structural_distance {
            return boundary;
        }
    }

    // 3. Try sentence endings (heuristic: punctuation followed by space)
    for punc in [". ", "! ", "? "] {
        if let Some(offset) = search_area.rfind(punc) {
            return search_start + offset + 2;
        }
    }

    // 4. Fallback to word boundary (space/tab)
    for i in (search_start..=pos).rev() {
        if content.is_char_boundary(i) {
            let c = content[i..].chars().next();
            if matches!(c, Some(' ') | Some('\t') | Some('\r')) {
                return i + 1;
            }
        }
    }

    // 5. Hard fallback: ensure we are at a valid character boundary
    if content.is_char_boundary(pos) {
        pos
    } else {
        (pos..content.len())
            .find(|&i| content.is_char_boundary(i))
            .unwrap_or(content.len())
    }
}

/// Detect document format from file extension.
fn detect_document_format(source_path: &str) -> String {
    let ext = Path::new(source_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "md" | "markdown" => "markdown".to_string(),
        "html" | "htm" => "html".to_string(),
        "txt" => "plain".to_string(),
        "json" => "json".to_string(),
        "yaml" | "yml" => "yaml".to_string(),
        "toml" => "toml".to_string(),
        "xml" => "xml".to_string(),
        "rst" => "restructuredtext".to_string(),
        _ => "plain".to_string(),
    }
}

/// A detected code boundary with position and semantic type.
#[derive(Debug, Clone)]
struct CodeBoundary {
    start: usize,
    semantic_type: String,
}

/// Find semantic boundaries in code based on language patterns.
fn find_code_boundaries(content: &str, language: &str) -> Vec<CodeBoundary> {
    let mut boundaries = Vec::new();

    // Language-specific patterns for function/class detection
    let patterns: Vec<(&str, &str)> = match language {
        "rs" => vec![
            (r"(?m)^pub\s+(async\s+)?fn\s+", "function"),
            (r"(?m)^(async\s+)?fn\s+", "function"),
            (r"(?m)^pub\s+struct\s+", "struct"),
            (r"(?m)^struct\s+", "struct"),
            (r"(?m)^pub\s+enum\s+", "enum"),
            (r"(?m)^enum\s+", "enum"),
            (r"(?m)^impl\s+", "impl"),
            (r"(?m)^pub\s+trait\s+", "trait"),
            (r"(?m)^trait\s+", "trait"),
            (r"(?m)^pub\s+mod\s+", "module"),
            (r"(?m)^mod\s+", "module"),
        ],
        "py" => vec![
            (r"(?m)^def\s+", "function"),
            (r"(?m)^async\s+def\s+", "function"),
            (r"(?m)^class\s+", "class"),
        ],
        "js" | "ts" => vec![
            (r"(?m)^function\s+", "function"),
            (r"(?m)^async\s+function\s+", "function"),
            (r"(?m)^(export\s+)?(default\s+)?class\s+", "class"),
            (
                r"(?m)^(export\s+)?(const|let|var)\s+\w+\s*=\s*(async\s+)?\(",
                "function",
            ),
            (
                r"(?m)^(export\s+)?(const|let|var)\s+\w+\s*=\s*(async\s+)?function",
                "function",
            ),
        ],
        "go" => vec![
            (r"(?m)^func\s+", "function"),
            (r"(?m)^type\s+\w+\s+struct\s*\{", "struct"),
            (r"(?m)^type\s+\w+\s+interface\s*\{", "interface"),
        ],
        "java" | "kt" | "scala" => vec![
            (
                r"(?m)^\s*(public|private|protected)?\s*(static\s+)?(void|int|String|boolean|\w+)\s+\w+\s*\(",
                "method",
            ),
            (
                r"(?m)^(public\s+)?(abstract\s+)?(class|interface|enum)\s+",
                "class",
            ),
        ],
        "rb" => vec![
            (r"(?m)^def\s+", "method"),
            (r"(?m)^class\s+", "class"),
            (r"(?m)^module\s+", "module"),
        ],
        "php" => vec![
            (
                r"(?m)^(public|private|protected)?\s*(static\s+)?function\s+",
                "function",
            ),
            (r"(?m)^(abstract\s+)?class\s+", "class"),
            (r"(?m)^interface\s+", "interface"),
            (r"(?m)^trait\s+", "trait"),
        ],
        _ => vec![
            // Generic patterns that work across many languages
            (r"(?m)^(pub\s+)?(fn|func|function|def)\s+", "function"),
            (r"(?m)^(pub\s+)?(class|struct|type)\s+", "type"),
        ],
    };

    // Compile and match patterns
    for (pattern, semantic_type) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            for mat in re.find_iter(content) {
                // Find the start of the line
                let line_start = content[..mat.start()]
                    .rfind('\n')
                    .map(|p| p + 1)
                    .unwrap_or(0);

                boundaries.push(CodeBoundary {
                    start: line_start,
                    semantic_type: semantic_type.to_string(),
                });
            }
        }
    }

    // Sort by position and deduplicate overlapping boundaries
    boundaries.sort_by_key(|b| b.start);
    boundaries.dedup_by_key(|b| b.start);

    boundaries
}

/// Fall back to line-based chunking when no semantic boundaries are found.
fn chunk_code_by_lines(
    content: &str,
    source_path: &str,
    language: &str,
    config: &ChunkingConfig,
) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut current_size = 0;
    let mut chunk_start_offset = 0;

    for (i, line) in lines.iter().enumerate() {
        let line_len = line.len() + 1; // +1 for newline
        current_size += line_len;

        // Check if we should create a chunk
        if current_size >= config.code_target_size || i == lines.len() - 1 {
            let end_offset = if i == lines.len() - 1 {
                content.len()
            } else {
                chunk_start_offset + current_size
            };

            let chunk_content = &content[chunk_start_offset..end_offset.min(content.len())];

            if !chunk_content.trim().is_empty() {
                chunks.push(Chunk::new(
                    source_path.to_string(),
                    chunk_content.to_string(),
                    chunk_start_offset,
                    end_offset.min(content.len()),
                    ChunkType::Code {
                        language: language.to_string(),
                        semantic_type: "block".to_string(),
                    },
                ));
            }

            chunk_start_offset = end_offset;
            current_size = 0;
        }
    }

    Ok(chunks)
}

/// Split oversized content into smaller chunks.
fn split_by_size(
    content: &str,
    source_path: &str,
    language: &str,
    semantic_type: &str,
    base_offset: usize,
    config: &ChunkingConfig,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut current_content = String::new();
    let mut chunk_start = 0;

    for line in lines {
        let would_exceed = current_content.len() + line.len() + 1 > config.code_max_size;

        if would_exceed && !current_content.is_empty() {
            // Create chunk from accumulated content
            let start_offset = base_offset + chunk_start;
            let end_offset = start_offset + current_content.len();

            chunks.push(Chunk::new(
                source_path.to_string(),
                current_content.clone(),
                start_offset,
                end_offset,
                ChunkType::Code {
                    language: language.to_string(),
                    semantic_type: semantic_type.to_string(),
                },
            ));

            chunk_start += current_content.len();
            current_content.clear();
        }

        if !current_content.is_empty() {
            current_content.push('\n');
        }
        current_content.push_str(line);
    }

    // Handle remaining content
    if !current_content.trim().is_empty() {
        let start_offset = base_offset + chunk_start;
        let end_offset = start_offset + current_content.len();

        chunks.push(Chunk::new(
            source_path.to_string(),
            current_content,
            start_offset,
            end_offset,
            ChunkType::Code {
                language: language.to_string(),
                semantic_type: semantic_type.to_string(),
            },
        ));
    }

    chunks
}

/// Merge small adjacent chunks of the same semantic type.
fn merge_small_chunks(chunks: Vec<Chunk>, target_size: usize) -> Vec<Chunk> {
    if chunks.is_empty() {
        return chunks;
    }

    let mut merged = Vec::new();
    let mut accumulator: Option<Chunk> = None;

    for chunk in chunks {
        match &mut accumulator {
            None => {
                accumulator = Some(chunk);
            }
            Some(acc) => {
                // Check if we should merge
                let same_type = match (&acc.chunk_type, &chunk.chunk_type) {
                    (
                        ChunkType::Code {
                            language: l1,
                            semantic_type: s1,
                        },
                        ChunkType::Code {
                            language: l2,
                            semantic_type: s2,
                        },
                    ) => l1 == l2 && s1 == s2,
                    (ChunkType::Document { format: f1 }, ChunkType::Document { format: f2 }) => {
                        f1 == f2
                    }
                    _ => false,
                };

                let combined_size = acc.content.len() + chunk.content.len();
                let both_small =
                    acc.content.len() < target_size && chunk.content.len() < target_size;

                if same_type && both_small && combined_size <= target_size * 2 {
                    // Merge chunks
                    acc.content.push('\n');
                    acc.content.push_str(&chunk.content);
                    acc.end_offset = chunk.end_offset;
                    // Regenerate chunk ID for merged chunk
                    acc.chunk_id = Chunk::generate_id(
                        &acc.source_path,
                        acc.start_offset,
                        acc.end_offset - acc.start_offset,
                    );
                    acc.content_hash = adapteros_core::B3Hash::hash(acc.content.as_bytes());
                } else {
                    // Push accumulated chunk and start new accumulation
                    if let Some(acc) = accumulator.take() {
                        merged.push(acc);
                    }
                    accumulator = Some(chunk);
                }
            }
        }
    }

    // Push final accumulated chunk
    if let Some(acc) = accumulator {
        merged.push(acc);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_document_basic() {
        let content = "This is a test document with some content that should be chunked.";
        let config = ChunkingConfig {
            token_chunk_size: 10,
            token_overlap: 2,
            ..Default::default()
        };

        let chunks = chunk_document(content, "test.txt", &config).unwrap();

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            assert_eq!(chunk.source_path, "test.txt");
            matches!(chunk.chunk_type, ChunkType::Document { .. });
        }
    }

    #[test]
    fn test_chunk_document_overlap() {
        let content = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let config = ChunkingConfig {
            token_chunk_size: 5,
            token_overlap: 2,
            ..Default::default()
        };

        let chunks = chunk_document(content, "test.txt", &config).unwrap();

        // With overlap, content from one chunk should appear in the next
        if chunks.len() >= 2 {
            // The chunks should have some overlap due to the overlap setting
            let first_end = chunks[0].end_offset;
            let second_start = chunks[1].start_offset;
            // With overlap, second chunk should start before first ends
            // (accounting for the step = chunk_size - overlap)
            assert!(
                second_start < first_end || chunks.len() <= 2,
                "Expected overlap between chunks"
            );
        }
    }

    #[test]
    fn test_chunk_document_empty() {
        let config = ChunkingConfig::default();
        let chunks = chunk_document("", "test.txt", &config).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_code_rust() {
        let content = r#"
pub fn hello() {
    println!("Hello");
}

pub fn world() {
    println!("World");
}

pub struct MyStruct {
    field: i32,
}
"#;
        let config = ChunkingConfig::default();

        let chunks = chunk_code(content, "test.rs", "rs", &config).unwrap();

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            match &chunk.chunk_type {
                ChunkType::Code {
                    language,
                    semantic_type,
                } => {
                    assert_eq!(language, "rs");
                    // Should detect functions and structs
                    assert!(
                        ["function", "struct", "module", "block"].contains(&semantic_type.as_str()),
                        "Unexpected semantic type: {}",
                        semantic_type
                    );
                }
                _ => panic!("Expected Code chunk type"),
            }
        }
    }

    #[test]
    fn test_chunk_code_python() {
        let content = r#"
def greet(name):
    print(f"Hello, {name}")

class Person:
    def __init__(self, name):
        self.name = name
"#;
        let config = ChunkingConfig::default();

        let chunks = chunk_code(content, "test.py", "py", &config).unwrap();

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            match &chunk.chunk_type {
                ChunkType::Code { language, .. } => {
                    assert_eq!(language, "py");
                }
                _ => panic!("Expected Code chunk type"),
            }
        }
    }

    #[test]
    fn test_chunk_file_autodetect() {
        let rust_content = "fn main() {}";
        let md_content = "# Hello World";

        let config = ChunkingConfig::default();

        let rust_chunks = chunk_file(rust_content, "main.rs", &config).unwrap();
        let md_chunks = chunk_file(md_content, "README.md", &config).unwrap();

        // Rust file should produce Code chunks
        assert!(matches!(
            rust_chunks.first().unwrap().chunk_type,
            ChunkType::Code { .. }
        ));

        // Markdown file should produce Document chunks
        assert!(matches!(
            md_chunks.first().unwrap().chunk_type,
            ChunkType::Document { .. }
        ));
    }

    #[test]
    fn test_chunk_ids_deterministic() {
        let content = "This is test content for determinism verification.";
        let config = ChunkingConfig::default();

        let chunks1 = chunk_document(content, "test.txt", &config).unwrap();
        let chunks2 = chunk_document(content, "test.txt", &config).unwrap();

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(
                c1.chunk_id, c2.chunk_id,
                "Chunk IDs should be deterministic"
            );
            assert_eq!(
                c1.content_hash, c2.content_hash,
                "Content hashes should be deterministic"
            );
        }
    }

    #[test]
    fn test_chunk_document_format_detection() {
        let config = ChunkingConfig::default();

        let md_chunks = chunk_document("# Test", "doc.md", &config).unwrap();
        let html_chunks = chunk_document("<h1>Test</h1>", "page.html", &config).unwrap();
        let txt_chunks = chunk_document("Plain text", "file.txt", &config).unwrap();

        match &md_chunks[0].chunk_type {
            ChunkType::Document { format } => assert_eq!(format, "markdown"),
            _ => panic!("Expected Document type"),
        }

        match &html_chunks[0].chunk_type {
            ChunkType::Document { format } => assert_eq!(format, "html"),
            _ => panic!("Expected Document type"),
        }

        match &txt_chunks[0].chunk_type {
            ChunkType::Document { format } => assert_eq!(format, "plain"),
            _ => panic!("Expected Document type"),
        }
    }

    #[test]
    fn test_semantic_boundary_detection() {
        let content = "Paragraph 1.\n\nParagraph 2 consists of multiple sentences. Here is the second sentence.";

        // Should prefer paragraph boundary (\n\n)
        let boundary = find_best_boundary(content, 20);
        assert_eq!(
            &content[..boundary],
            "Paragraph 1.\n\n",
            "Expected paragraph boundary preference"
        );

        // Should prefer sentence boundary (. )
        let boundary = find_best_boundary(content, 70);
        assert!(
            content[..boundary].ends_with(". "),
            "Expected sentence boundary preference"
        );

        // Word boundary fallback
        let simple_content = "hello world";
        let boundary = find_best_boundary(simple_content, 8);
        assert_eq!(boundary, 6, "Expected word boundary fallback");
    }

    #[test]
    fn test_code_boundary_detection_rust() {
        let content = r#"
fn foo() {}
pub fn bar() {}
struct Baz {}
"#;
        let boundaries = find_code_boundaries(content, "rs");

        assert!(
            boundaries.len() >= 2,
            "Should detect at least 2 boundaries, found {}",
            boundaries.len()
        );
    }

    #[test]
    fn test_chunk_oversized_code() {
        let mut large_function = String::from("fn large_function() {\n");
        for i in 0..500 {
            large_function.push_str(&format!("    let x{} = {};\n", i, i));
        }
        large_function.push_str("}\n");

        let config = ChunkingConfig {
            code_target_size: 500,
            code_max_size: 1000,
            ..Default::default()
        };

        let chunks = chunk_code(&large_function, "test.rs", "rs", &config).unwrap();

        // Should split into multiple chunks due to size limits
        assert!(
            chunks.len() > 1,
            "Large function should be split into multiple chunks"
        );

        // Each chunk should respect max size (with some tolerance for line boundaries)
        for chunk in &chunks {
            assert!(
                chunk.content.len() <= config.code_max_size * 2,
                "Chunk size {} exceeds limit",
                chunk.content.len()
            );
        }
    }

    #[test]
    fn test_merge_small_chunks() {
        let chunks = vec![
            Chunk::new(
                "test.rs".to_string(),
                "let a = 1;".to_string(),
                0,
                10,
                ChunkType::Code {
                    language: "rs".to_string(),
                    semantic_type: "block".to_string(),
                },
            ),
            Chunk::new(
                "test.rs".to_string(),
                "let b = 2;".to_string(),
                10,
                20,
                ChunkType::Code {
                    language: "rs".to_string(),
                    semantic_type: "block".to_string(),
                },
            ),
        ];

        let merged = merge_small_chunks(chunks, 1000);

        // Small chunks of the same type should be merged
        assert_eq!(merged.len(), 1, "Small chunks should be merged");
        assert!(merged[0].content.contains("let a = 1;"));
        assert!(merged[0].content.contains("let b = 2;"));
    }
}
