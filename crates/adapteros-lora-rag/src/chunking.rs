//! Semantic code chunking for vector search
//!
//! Uses tree-sitter to chunk code by semantic boundaries (functions, classes, modules)
//! while including context (imports, surrounding code).

use adapteros_codegraph::types::{Language, SymbolKind, SymbolNode};
use adapteros_core::B3Hash;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

/// Builder for creating symbol chunks with complex parameters
#[derive(Debug)]
pub struct SymbolChunkBuilder<'a> {
    parent: Option<&'a SymbolNode>,
    children: Option<&'a [SymbolNode]>,
    lines: Option<&'a [&'a str]>,
    file_path: Option<&'a Path>,
    repo_id: Option<&'a str>,
    commit_sha: Option<&'a str>,
    language: Option<&'a str>,
    file_context: Option<&'a ChunkContext>,
}

impl<'a> SymbolChunkBuilder<'a> {
    pub fn new() -> Self {
        Self {
            parent: None,
            children: None,
            lines: None,
            file_path: None,
            repo_id: None,
            commit_sha: None,
            language: None,
            file_context: None,
        }
    }

    pub fn parent(mut self, parent: &'a SymbolNode) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn children(mut self, children: &'a [SymbolNode]) -> Self {
        self.children = Some(children);
        self
    }

    pub fn lines(mut self, lines: &'a [&'a str]) -> Self {
        self.lines = Some(lines);
        self
    }

    pub fn file_path(mut self, file_path: &'a Path) -> Self {
        self.file_path = Some(file_path);
        self
    }

    pub fn repo_id(mut self, repo_id: &'a str) -> Self {
        self.repo_id = Some(repo_id);
        self
    }

    pub fn commit_sha(mut self, commit_sha: &'a str) -> Self {
        self.commit_sha = Some(commit_sha);
        self
    }

    pub fn language(mut self, language: &'a str) -> Self {
        self.language = Some(language);
        self
    }

    pub fn file_context(mut self, file_context: &'a ChunkContext) -> Self {
        self.file_context = Some(file_context);
        self
    }

    pub fn build(self) -> Result<SymbolChunkParams<'a>> {
        Ok(SymbolChunkParams {
            parent: self
                .parent
                .ok_or_else(|| anyhow::anyhow!("parent is required"))?,
            children: self.children.unwrap_or(&[]),
            lines: self
                .lines
                .ok_or_else(|| anyhow::anyhow!("lines is required"))?,
            file_path: self
                .file_path
                .ok_or_else(|| anyhow::anyhow!("file_path is required"))?,
            repo_id: self
                .repo_id
                .ok_or_else(|| anyhow::anyhow!("repo_id is required"))?,
            commit_sha: self
                .commit_sha
                .ok_or_else(|| anyhow::anyhow!("commit_sha is required"))?,
            language: self
                .language
                .ok_or_else(|| anyhow::anyhow!("language is required"))?,
            file_context: self
                .file_context
                .ok_or_else(|| anyhow::anyhow!("file_context is required"))?,
        })
    }
}

/// Parameters for symbol chunk creation
#[derive(Debug)]
pub struct SymbolChunkParams<'a> {
    pub parent: &'a SymbolNode,
    pub children: &'a [SymbolNode],
    pub lines: &'a [&'a str],
    pub file_path: &'a Path,
    pub repo_id: &'a str,
    pub commit_sha: &'a str,
    pub language: &'a str,
    pub file_context: &'a ChunkContext,
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

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(ChunkConfig::default())
    }

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
            let params = SymbolChunkBuilder::new()
                .parent(&parent_symbol)
                .children(&child_symbols)
                .lines(&lines)
                .file_path(file_path)
                .repo_id(repo_id)
                .commit_sha(commit_sha)
                .language(&language)
                .file_context(&context)
                .build()?;
            let chunk = self.create_symbol_chunk(params)?;

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

    /// Create a chunk from a symbol and its children using parameter struct
    fn create_symbol_chunk(&self, params: SymbolChunkParams) -> Result<CodeChunk> {
        let parent = params.parent;
        let _children = params.children;
        let lines = params.lines;
        let file_path = params.file_path;
        let repo_id = params.repo_id;
        let commit_sha = params.commit_sha;
        let language = params.language;
        let file_context = params.file_context;
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

        // Simple regex-based import extraction
        // TODO: Use tree-sitter for more accurate extraction
        match language {
            "rust" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("use ") {
                        context.imports.push(trimmed.to_string());
                    }
                }
            }
            "python" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                        context.imports.push(trimmed.to_string());
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
