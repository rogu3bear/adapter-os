//! Multi-language parser module
//!
//! Provides a unified interface for parsing multiple programming languages
//! using tree-sitter, with language detection and symbol extraction.
//!
//! # Determinism
//!
//! This module ensures deterministic output by:
//! - Sorting file entries using [`adapteros_core::compare_paths_deterministic`]
//! - Using cross-platform path normalization for consistent ordering
//!
//! The algorithm version is tracked by [`PARSER_ALGORITHM_VERSION`] in
//! `adapteros_core::version`.

use crate::types::{Language, ParseResult};
use adapteros_core::{compare_paths_deterministic, AosError, Result};
use std::collections::HashMap;
use std::path::Path;

pub mod go;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod test_utils;
pub mod typescript;

/// Trait for language-specific parsers
pub trait LanguageParser {
    /// Get the language this parser handles
    fn language(&self) -> Language;

    /// Parse a single source file
    fn parse_file(&mut self, path: &Path) -> Result<ParseResult>;

    /// Get supported file extensions
    fn supported_extensions(&self) -> &[&str];

    /// Check if this parser can handle the given file
    fn can_parse(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.supported_extensions().contains(&ext))
            .unwrap_or(false)
    }
}

/// Factory for creating language-specific parsers
pub struct ParserFactory;

impl ParserFactory {
    /// Create a parser for the given language
    pub fn create_parser(language: Language) -> Result<Box<dyn LanguageParser>> {
        match language {
            Language::Rust => Ok(Box::new(rust::RustParser::new()?)),
            Language::Python => Ok(Box::new(python::PythonParser::new()?)),
            Language::TypeScript => Ok(Box::new(typescript::TypeScriptParser::new()?)),
            Language::JavaScript => Ok(Box::new(javascript::JavaScriptParser::new()?)),
            Language::Go => Ok(Box::new(go::GoParser::new()?)),
        }
    }

    /// Create a parser for the given file path
    pub fn create_parser_for_file(path: &Path) -> Result<Box<dyn LanguageParser>> {
        let language = Language::from_path(path)
            .ok_or_else(|| AosError::Parse(format!("Unsupported file type: {}", path.display())))?;
        Self::create_parser(language)
    }

    /// Create parsers for all supported languages
    pub fn create_all_parsers() -> Result<HashMap<Language, Box<dyn LanguageParser>>> {
        let mut parsers = HashMap::new();

        for language in [
            Language::Rust,
            Language::Python,
            Language::TypeScript,
            Language::JavaScript,
            Language::Go,
        ] {
            parsers.insert(language.clone(), Self::create_parser(language)?);
        }

        Ok(parsers)
    }
}

/// Detect programming language from file path
pub fn detect_language(path: &Path) -> Option<Language> {
    Language::from_path(path)
}

/// Parse a file using the appropriate language parser
pub fn parse_file(path: &Path) -> Result<ParseResult> {
    let mut parser = ParserFactory::create_parser_for_file(path)?;
    parser.parse_file(path)
}

/// Parse all supported files in a directory
///
/// DETERMINISM: Results are sorted by file path to ensure consistent ordering
/// across different runs and file systems. This is important for reproducible
/// dataset generation and training.
pub async fn parse_directory(dir: &Path) -> Result<Vec<ParseResult>> {
    let mut results = Vec::new();
    let parsers = ParserFactory::create_all_parsers()?;

    // DETERMINISM: Collect all entries first, then sort by path to ensure
    // consistent ordering regardless of filesystem traversal order.
    // Uses cross-platform path normalization for consistent sorting on all OSes.
    let mut entries: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
    entries.sort_by(|a, b| compare_paths_deterministic(a.path(), b.path()));

    for entry in entries {
        let path = entry.path();

        if let Some(language) = detect_language(path) {
            if let Some(_parser) = parsers.get(&language) {
                // We need to clone the parser trait object, which requires some workaround
                // For now, create a new parser for each file
                let mut file_parser = ParserFactory::create_parser(language)?;
                match file_parser.parse_file(path) {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {}", path.display(), e);
                        // Continue with other files
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Common utilities for tree-sitter parsing
pub mod utils {
    use crate::types::{Language, Span, SymbolId, SymbolKind, SymbolNode, Visibility};

    use std::path::Path;
    use tree_sitter::Node;

    /// Convert tree-sitter node to span
    pub fn node_to_span(node: Node) -> Span {
        let start_point = node.start_position();
        let end_point = node.end_position();

        Span::new(
            start_point.row as u32 + 1, // Convert to 1-indexed
            start_point.column as u32 + 1,
            end_point.row as u32 + 1,
            end_point.column as u32 + 1,
            node.start_byte(),
            node.end_byte() - node.start_byte(),
        )
    }

    /// Create a symbol node from basic information
    pub fn create_symbol_node(
        name: String,
        kind: SymbolKind,
        language: Language,
        span: Span,
        file_path: &Path,
    ) -> SymbolNode {
        let id = SymbolId::new(&file_path.to_string_lossy(), &span.to_string(), &name);

        SymbolNode::new(
            id,
            name,
            kind,
            language,
            span,
            file_path.to_string_lossy().to_string(),
        )
    }

    /// Extract text from a tree-sitter node
    pub fn extract_text(node: Node, source: &str) -> String {
        source[node.byte_range()].to_string()
    }

    /// Check if a node has a specific child by type
    pub fn has_child_of_type<'a>(node: Node<'a>, child_type: &str) -> bool {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children.iter().any(|child| child.kind() == child_type)
    }

    /// Find the first child of a specific type
    pub fn find_child_of_type<'a>(node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find(|child| child.kind() == child_type)
    }

    /// Find all children of a specific type
    pub fn find_children_of_type<'a>(node: Node<'a>, child_type: &str) -> Vec<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .filter(|child| child.kind() == child_type)
            .collect()
    }

    /// Parse visibility from text (language-specific)
    pub fn parse_visibility(text: &str, language: Language) -> Visibility {
        match language {
            Language::Rust => parse_rust_visibility(text),
            Language::Python => parse_python_visibility(text),
            Language::TypeScript | Language::JavaScript => parse_js_visibility(text),
            Language::Go => parse_go_visibility(text),
        }
    }

    fn parse_rust_visibility(text: &str) -> Visibility {
        match text {
            "pub" => Visibility::Public,
            "pub(crate)" => Visibility::Crate,
            "pub(super)" => Visibility::Super,
            _ if text.starts_with("pub(in ") => {
                let path = text
                    .strip_prefix("pub(in ")
                    .and_then(|s| s.strip_suffix(")"))
                    .unwrap_or("");
                Visibility::InPath(path.to_string())
            }
            _ => Visibility::Private,
        }
    }

    fn parse_python_visibility(text: &str) -> Visibility {
        // Python doesn't have explicit visibility modifiers
        // Functions/classes starting with underscore are considered private
        if text.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Public
        }
    }

    fn parse_js_visibility(text: &str) -> Visibility {
        // JavaScript/TypeScript visibility is based on export keywords
        match text {
            "export" | "public" => Visibility::Public,
            "private" => Visibility::Private,
            "protected" => Visibility::Super, // Map protected to Super
            _ => Visibility::Public,          // Default to public in JS/TS
        }
    }

    fn parse_go_visibility(text: &str) -> Visibility {
        // Go visibility is based on capitalization
        if text.chars().next().is_some_and(|c| c.is_uppercase()) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("create temp dir")
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(detect_language(Path::new("test.rs")), Some(Language::Rust));
        assert_eq!(
            detect_language(Path::new("test.py")),
            Some(Language::Python)
        );
        assert_eq!(
            detect_language(Path::new("test.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            detect_language(Path::new("test.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(detect_language(Path::new("test.go")), Some(Language::Go));
        assert_eq!(detect_language(Path::new("test.txt")), None);
    }

    #[test]
    fn test_parser_factory() {
        let rust_parser = ParserFactory::create_parser(Language::Rust);
        assert!(rust_parser.is_ok());

        let python_parser = ParserFactory::create_parser(Language::Python);
        assert!(python_parser.is_ok());

        let all_parsers = ParserFactory::create_all_parsers();
        assert!(all_parsers.is_ok());
        assert_eq!(all_parsers.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn test_parse_directory() {
        let temp_dir = new_test_tempdir();

        // Create test files
        fs::write(temp_dir.path().join("test.rs"), "fn test() {}").unwrap();
        fs::write(temp_dir.path().join("test.py"), "def test(): pass").unwrap();
        fs::write(temp_dir.path().join("test.ts"), "function test() {}").unwrap();

        let results = parse_directory(temp_dir.path()).await.unwrap();
        assert_eq!(results.len(), 3);

        // Check that each result has the correct language
        let languages: std::collections::HashSet<_> = results
            .iter()
            .map(|r| r.file_path.extension().unwrap().to_str().unwrap())
            .collect();
        assert!(languages.contains("rs"));
        assert!(languages.contains("py"));
        assert!(languages.contains("ts"));
    }
}
