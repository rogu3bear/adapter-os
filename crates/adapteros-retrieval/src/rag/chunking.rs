//! Semantic code chunking for vector search
//!
//! Uses tree-sitter to chunk code by semantic boundaries (functions, classes, modules)
//! while including context (imports, surrounding code).

use adapteros_core::B3Hash;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

// Re-export types from codegraph for convenience
pub use crate::codegraph::types::{
    Language, Span, SymbolId, SymbolKind, SymbolNode, Visibility,
};

/// Chunking configuration
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Target chunk size in characters
    pub target_size: usize,
    /// Maximum chunk size in characters
    pub max_size: usize,
    /// Overlap between chunks in characters
    pub overlap: usize,
    /// Include imports and context
    pub include_context: bool,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_size: 1000,
            max_size: 2000,
            overlap: 200,
            include_context: true,
        }
    }
}

/// Context for a code chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkContext {
    /// Imports used in this chunk
    pub imports: Vec<String>,
    /// Parent symbols (class, module, etc.)
    pub parent_symbols: Vec<String>,
    /// Neighboring symbol names
    pub neighbors: Vec<String>,
}

impl ChunkContext {
    pub fn new() -> Self {
        Self {
            imports: Vec::new(),
            parent_symbols: Vec::new(),
            neighbors: Vec::new(),
        }
    }
}

impl Default for ChunkContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A semantically-bounded code chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Unique chunk identifier
    pub chunk_id: String,
    /// Repository ID
    pub repo_id: String,
    /// File path
    pub file_path: String,
    /// Start line (1-indexed)
    pub line_start: usize,
    /// End line (1-indexed)
    pub line_end: usize,
    /// Chunk content
    pub content: String,
    /// Programming language
    pub language: String,
    /// Chunk type (function, class, module, etc.)
    pub chunk_type: String,
    /// Context information
    pub context: ChunkContext,
    /// Commit SHA
    pub commit_sha: String,
}

impl CodeChunk {
    /// Generate a chunk ID from components
    pub fn generate_id(
        repo_id: &str,
        file_path: &str,
        line_start: usize,
        line_end: usize,
    ) -> String {
        let data = format!("{}:{}:{}:{}", repo_id, file_path, line_start, line_end);
        B3Hash::hash(data.as_bytes()).to_hex()
    }

    /// Create new code chunk
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo_id: String,
        file_path: String,
        line_start: usize,
        line_end: usize,
        content: String,
        language: String,
        chunk_type: String,
        commit_sha: String,
    ) -> Self {
        let chunk_id = Self::generate_id(&repo_id, &file_path, line_start, line_end);
        Self {
            chunk_id,
            repo_id,
            file_path,
            line_start,
            line_end,
            content,
            language,
            chunk_type,
            context: ChunkContext::new(),
            commit_sha,
        }
    }

    /// Set context
    pub fn with_context(mut self, context: ChunkContext) -> Self {
        self.context = context;
        self
    }
}

/// Code chunker
pub struct CodeChunker {
    config: ChunkConfig,
}

impl CodeChunker {
    /// Create a new code chunker
    pub fn new(config: ChunkConfig) -> Self {
        Self { config }
    }
}

impl Default for CodeChunker {
    fn default() -> Self {
        Self::new(ChunkConfig::default())
    }
}

impl CodeChunker {
    /// Chunk a file based on its symbols
    pub fn chunk_file(
        &self,
        file_path: &Path,
        content: &str,
        symbols: &[SymbolNode],
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<Vec<CodeChunk>> {
        let language = Language::from_path(file_path)
            .map(|l| l.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Extract imports/context from file
        let context = if self.config.include_context {
            self.extract_file_context(content, &language)?
        } else {
            ChunkContext::new()
        };

        // Group symbols by top-level definitions
        let top_level_symbols = self.group_top_level_symbols(symbols);

        for (parent_symbol, child_symbols) in top_level_symbols {
            let chunk = self.create_symbol_chunk(
                &parent_symbol,
                &child_symbols,
                &lines,
                file_path,
                repo_id,
                commit_sha,
                &language,
                &context,
            )?;

            // Check if chunk exceeds max size and needs splitting
            if chunk.content.len() > self.config.max_size {
                let split_chunks = self.split_large_chunk(chunk)?;
                chunks.extend(split_chunks);
            } else {
                chunks.push(chunk);
            }
        }

        // Handle any remaining code not covered by symbols (if needed)
        // This could be module-level code, comments, etc.

        Ok(chunks)
    }

    /// Group symbols into top-level definitions with their children
    fn group_top_level_symbols(
        &self,
        symbols: &[SymbolNode],
    ) -> Vec<(SymbolNode, Vec<SymbolNode>)> {
        let mut groups = Vec::new();
        let mut used = vec![false; symbols.len()];

        // First pass: identify top-level symbols (functions, structs, etc.)
        for (i, symbol) in symbols.iter().enumerate() {
            if used[i] {
                continue;
            }

            // Check if this is a container symbol (struct, impl, trait, etc.)
            let is_container = matches!(
                symbol.kind,
                SymbolKind::Struct
                    | SymbolKind::Enum
                    | SymbolKind::Trait
                    | SymbolKind::Impl
                    | SymbolKind::Module
            );

            if is_container {
                // Find children (methods, fields, etc.) within this symbol's span
                let mut children = Vec::new();
                for (j, child) in symbols.iter().enumerate() {
                    if i != j
                        && !used[j]
                        && child.span.start_line >= symbol.span.start_line
                        && child.span.end_line <= symbol.span.end_line
                    {
                        children.push(child.clone());
                        used[j] = true;
                    }
                }
                groups.push((symbol.clone(), children));
                used[i] = true;
            } else if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                // Top-level function
                groups.push((symbol.clone(), vec![]));
                used[i] = true;
            }
        }

        // Handle remaining symbols (consts, statics, etc.)
        for (i, symbol) in symbols.iter().enumerate() {
            if !used[i] {
                groups.push((symbol.clone(), vec![]));
            }
        }

        groups
    }

    /// Create a chunk from a symbol and its children
    #[allow(clippy::too_many_arguments)]
    fn create_symbol_chunk(
        &self,
        parent: &SymbolNode,
        _children: &[SymbolNode],
        lines: &[&str],
        file_path: &Path,
        repo_id: &str,
        commit_sha: &str,
        language: &str,
        file_context: &ChunkContext,
    ) -> Result<CodeChunk> {
        let line_start = parent.span.start_line as usize;
        let line_end = parent.span.end_line as usize;

        // Extract content with overlap
        let overlap_start = if self.config.overlap > 0 && line_start > 1 {
            line_start.saturating_sub(self.config.overlap / 50) // Approximate lines from overlap
        } else {
            line_start
        };

        let overlap_end = if self.config.overlap > 0 {
            (line_end + self.config.overlap / 50).min(lines.len())
        } else {
            line_end
        };

        let content = lines[overlap_start.saturating_sub(1)..overlap_end].join("\n");

        let chunk_type = parent.kind.to_string();

        // Build context
        let mut context = file_context.clone();
        context.parent_symbols.push(parent.qualified_name());

        let chunk = CodeChunk::new(
            repo_id.to_string(),
            file_path.display().to_string(),
            line_start,
            line_end,
            content,
            language.to_string(),
            chunk_type,
            commit_sha.to_string(),
        )
        .with_context(context);

        Ok(chunk)
    }

    /// Split a large chunk into smaller pieces
    fn split_large_chunk(&self, chunk: CodeChunk) -> Result<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = chunk.content.lines().collect();
        let target_lines = self.config.target_size / 50; // Approximate lines per chunk
        let overlap_lines = self.config.overlap / 50;

        let mut current_line = 0;
        let mut chunk_num = 0;

        while current_line < lines.len() {
            let end_line = (current_line + target_lines).min(lines.len());
            let content = lines[current_line..end_line].join("\n");

            let line_start = chunk.line_start + current_line;
            let line_end = chunk.line_start + end_line - 1;

            let sub_chunk = CodeChunk::new(
                chunk.repo_id.clone(),
                chunk.file_path.clone(),
                line_start,
                line_end,
                content,
                chunk.language.clone(),
                format!("{}_part{}", chunk.chunk_type, chunk_num),
                chunk.commit_sha.clone(),
            )
            .with_context(chunk.context.clone());

            chunks.push(sub_chunk);

            // Move to next chunk with overlap
            current_line = end_line.saturating_sub(overlap_lines);
            if current_line == end_line {
                break; // Prevent infinite loop
            }
            chunk_num += 1;
        }

        Ok(chunks)
    }

    /// Extract imports and file-level context
    fn extract_file_context(&self, content: &str, language: &str) -> Result<ChunkContext> {
        let mut context = ChunkContext::new();

        match language {
            "rust" => {
                // Use line-based extraction for Rust (simple and accurate for use statements)
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("use ") {
                        context.imports.push(trimmed.to_string());
                    }
                }
            }
            "python" => {
                // Extract Python imports using line-based parsing with better accuracy
                // Handles: import x, from x import y, multi-line imports
                let mut in_multiline_import = false;
                let mut current_import = String::new();

                for line in content.lines() {
                    let trimmed = line.trim();

                    if in_multiline_import {
                        current_import.push(' ');
                        current_import.push_str(trimmed);
                        if trimmed.contains(')') {
                            context.imports.push(current_import.clone());
                            current_import.clear();
                            in_multiline_import = false;
                        }
                    } else if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                        if trimmed.contains('(') && !trimmed.contains(')') {
                            // Start of multi-line import
                            in_multiline_import = true;
                            current_import = trimmed.to_string();
                        } else {
                            context.imports.push(trimmed.to_string());
                        }
                    } else if !trimmed.is_empty()
                        && !trimmed.starts_with('#')
                        && !trimmed.starts_with("\"\"\"")
                        && !trimmed.starts_with("'''")
                    {
                        // Stop at first non-import, non-comment, non-docstring line
                        // (imports should be at the top of the file)
                        if !context.imports.is_empty() {
                            break;
                        }
                    }
                }
            }
            "typescript" | "javascript" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("import ") {
                        context.imports.push(trimmed.to_string());
                    }
                }
            }
            _ => {}
        }

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_id_generation() {
        let id1 = CodeChunk::generate_id("repo1", "src/main.rs", 10, 20);
        let id2 = CodeChunk::generate_id("repo1", "src/main.rs", 10, 20);
        let id3 = CodeChunk::generate_id("repo1", "src/main.rs", 10, 21);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_code_chunk_creation() {
        let chunk = CodeChunk::new(
            "repo1".to_string(),
            "src/test.rs".to_string(),
            10,
            20,
            "fn test() {}".to_string(),
            "rust".to_string(),
            "function".to_string(),
            "abc123".to_string(),
        );

        assert_eq!(chunk.repo_id, "repo1");
        assert_eq!(chunk.line_start, 10);
        assert_eq!(chunk.line_end, 20);
        assert!(!chunk.chunk_id.is_empty());
    }

    #[test]
    fn test_extract_rust_imports() {
        let chunker = CodeChunker::default();
        let content = r#"
use std::collections::HashMap;
use anyhow::Result;

fn main() {
    println!("Hello");
}
"#;

        let context = chunker.extract_file_context(content, "rust").unwrap();
        assert_eq!(context.imports.len(), 2);
        assert!(context
            .imports
            .contains(&"use std::collections::HashMap;".to_string()));
        assert!(context.imports.contains(&"use anyhow::Result;".to_string()));
    }
}
