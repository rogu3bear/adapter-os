//! Tree-sitter based code parser
//!
//! Provides deterministic parsing of Rust source code using Tree-sitter,
//! with support for conditional compilation and semantic analysis.

use crate::types::{Span, SymbolKind, SymbolNode, SymbolId, Visibility};
use adapteros_core::{AosError, Result};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::mem;
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Parser, Query, QueryCursor};
use walkdir::WalkDir;

/// Rust language definition
fn language_from_ptr(ptr: *const c_void) -> Language {
    assert!(!ptr.is_null(), "tree_sitter_rust returned null language");
    // SAFETY: Language is a wrapper around a raw pointer to a tree-sitter language.
    // The tree_sitter_rust crate guarantees that the pointer returned from language()
    // points to a valid, static language definition. The assert above ensures non-null.
    unsafe { mem::transmute::<*const c_void, Language>(ptr) }
}

fn rust_language() -> Language {
    let lang = tree_sitter_rust::language();
    // SAFETY: tree_sitter::Language is a newtype wrapper around a raw pointer.
    // This roundtrip through c_void is to handle the tree-sitter API which uses
    // opaque pointers. The Language type has the same layout as a pointer.
    let raw = unsafe { mem::transmute::<_, *const c_void>(lang) };
    language_from_ptr(raw)
}

/// Result of parsing a source file
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// File path
    pub file_path: PathBuf,
    /// Parsed symbols
    pub symbols: Vec<SymbolNode>,
    /// Parse errors
    pub errors: Vec<String>,
    /// Content hash for determinism
    pub content_hash: adapteros_core::B3Hash,
}

/// Tree-sitter based code parser
pub struct CodeParser {
    /// Tree-sitter parser
    parser: Parser,
    /// Rust language
    rust_lang: Language,
    /// Query for function definitions
    function_query: Query,
    /// Query for struct definitions
    struct_query: Query,
    /// Query for trait definitions
    trait_query: Query,
    /// Query for impl blocks
    impl_query: Query,
}

impl CodeParser {
    /// Create a new code parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let rust_lang = rust_language();
        
        parser.set_language(rust_lang)
            .map_err(|e| AosError::Parsing(format!("Failed to set Rust language: {}", e)))?;

        // Define queries for different symbol types
        let function_query = Query::new(
            rust_lang,
            r#"
            (function_item
                (visibility_modifier)? @visibility
                name: (identifier) @name
                parameters: (parameters) @params
                return_type: (type_identifier)? @return_type
            ) @function
            "#
        ).map_err(|e| AosError::Parsing(format!("Failed to create function query: {}", e)))?;

        let struct_query = Query::new(
            rust_lang,
            r#"
            (struct_item
                (visibility_modifier)? @visibility
                name: (type_identifier) @name
            ) @struct
            "#
        ).map_err(|e| AosError::Parsing(format!("Failed to create struct query: {}", e)))?;

        let trait_query = Query::new(
            rust_lang,
            r#"
            (trait_item
                (visibility_modifier)? @visibility
                name: (type_identifier) @name
            ) @trait
            "#
        ).map_err(|e| AosError::Parsing(format!("Failed to create trait query: {}", e)))?;

        let impl_query = Query::new(
            rust_lang,
            r#"
            (impl_item
                trait: (type_identifier)? @trait_name
                type: (type_identifier) @type_name
            ) @impl
            "#
        ).map_err(|e| AosError::Parsing(format!("Failed to create impl query: {}", e)))?;

        Ok(Self {
            parser,
            rust_lang,
            function_query,
            struct_query,
            trait_query,
            impl_query,
        })
    }

    /// Parse a single Rust source file
    pub fn parse_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<ParseResult> {
        let file_path = file_path.as_ref();
        let source_code = std::fs::read_to_string(file_path)
            .map_err(|e| AosError::Io(format!("Failed to read file {}: {}", file_path.display(), e)))?;

        // Parse the source code
        let tree = self.parser.parse(&source_code, None)
            .ok_or_else(|| AosError::Parsing("Failed to parse source code".to_string()))?;

        let mut symbols = Vec::new();
        let mut errors = Vec::new();

        // Extract symbols using queries
        self.extract_functions(&tree, &source_code, &file_path, &mut symbols)?;
        self.extract_structs(&tree, &source_code, &file_path, &mut symbols)?;
        self.extract_traits(&tree, &source_code, &file_path, &mut symbols)?;
        self.extract_impls(&tree, &source_code, &file_path, &mut symbols)?;

        // Compute content hash for determinism
        let content_hash = adapteros_core::B3Hash::hash(source_code.as_bytes());

        Ok(ParseResult {
            file_path: file_path.to_path_buf(),
            symbols,
            errors,
            content_hash,
        })
    }

    /// Parse all Rust files in a directory
    pub async fn parse_directory<P: AsRef<Path>>(&mut self, dir: P) -> Result<Vec<ParseResult>> {
        let mut results = Vec::new();
        
        for entry in WalkDir::new(dir) {
            let entry = entry.map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                match self.parse_file(path) {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {}", path.display(), e);
                        // Continue with other files
                    }
                }
            }
        }
        
        Ok(results)
    }

    /// Extract function definitions
    fn extract_functions(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.function_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;
            let mut params = None;
            let mut return_type = None;

            for capture in mat.captures {
                let text = &source[capture.node.byte_range()];
                match capture.index {
                    0 => visibility = self.parse_visibility(text),
                    1 => {
                        name = Some(text.to_string());
                        name_node = Some(capture.node);
                    }
                    2 => params = Some(text.to_string()),
                    3 => return_type = Some(text.to_string()),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = self.node_to_span(name_node.unwrap_or(mat.captures[0].node));
                let id = SymbolId::new(
                    &file_path.to_string_lossy(),
                    &span.to_string(),
                    &name,
                );

                let mut symbol = SymbolNode::new(
                    id,
                    name,
                    SymbolKind::Function,
                    span,
                    file_path.to_string_lossy().to_string(),
                ).with_visibility(visibility);

                if let Some(params) = params {
                    symbol = symbol.with_signature(params);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract struct definitions
    fn extract_structs(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.struct_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = &source[capture.node.byte_range()];
                match capture.index {
                    0 => visibility = self.parse_visibility(text),
                    1 => {
                        name = Some(text.to_string());
                        name_node = Some(capture.node);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = self.node_to_span(name_node.unwrap_or(mat.captures[0].node));
                let id = SymbolId::new(
                    &file_path.to_string_lossy(),
                    &span.to_string(),
                    &name,
                );

                let symbol = SymbolNode::new(
                    id,
                    name,
                    SymbolKind::Struct,
                    span,
                    file_path.to_string_lossy().to_string(),
                ).with_visibility(visibility);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract trait definitions
    fn extract_traits(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.trait_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = &source[capture.node.byte_range()];
                match capture.index {
                    0 => visibility = self.parse_visibility(text),
                    1 => {
                        name = Some(text.to_string());
                        name_node = Some(capture.node);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = self.node_to_span(name_node.unwrap_or(mat.captures[0].node));
                let id = SymbolId::new(
                    &file_path.to_string_lossy(),
                    &span.to_string(),
                    &name,
                );

                let symbol = SymbolNode::new(
                    id,
                    name,
                    SymbolKind::Trait,
                    span,
                    file_path.to_string_lossy().to_string(),
                ).with_visibility(visibility);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract impl blocks
    fn extract_impls(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.impl_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut trait_name = None;
            let mut type_name = None;

            for capture in mat.captures {
                let text = &source[capture.node.byte_range()];
                match capture.index {
                    0 => trait_name = Some(text.to_string()),
                    1 => type_name = Some(text.to_string()),
                    _ => {}
                }
            }

            if let Some(type_name) = type_name {
                let span = self.node_to_span(mat.captures[0].node);
                let name = if let Some(trait_name) = trait_name {
                    format!("impl {} for {}", trait_name, type_name)
                } else {
                    format!("impl {}", type_name)
                };

                let id = SymbolId::new(
                    &file_path.to_string_lossy(),
                    &span.to_string(),
                    &name,
                );

                let symbol = SymbolNode::new(
                    id,
                    name,
                    SymbolKind::Impl,
                    span,
                    file_path.to_string_lossy().to_string(),
                );

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Convert tree-sitter node to span
    fn node_to_span(&self, node: tree_sitter::Node) -> Span {
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

    /// Parse visibility modifier
    fn parse_visibility(&self, text: &str) -> Visibility {
        match text {
            "pub" => Visibility::Public,
            "pub(crate)" => Visibility::Crate,
            "pub(super)" => Visibility::Super,
            _ if text.starts_with("pub(in ") => {
                let path = text.strip_prefix("pub(in ")
                    .and_then(|s| s.strip_suffix(")"))
                    .unwrap_or("");
                Visibility::InPath(path.to_string())
            }
            _ => Visibility::Private,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{Builder, TempDir};

    fn new_test_tempdir() -> TempDir {
        let root = adapteros_core::resolve_var_dir().join("tmp");
        std::fs::create_dir_all(&root).expect("create var tmp");
        Builder::new()
            .prefix("aos-test-")
            .tempdir_in(&root)
            .expect("Test temp directory creation should succeed")
    }

    #[test]
    fn test_parser_creation() {
        let parser = CodeParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = CodeParser::new()
            .expect("CodeParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");
        
        std::fs::write(&test_file, "pub fn test_function() -> i32 { 42 }")
            .expect("Writing test file should succeed");
        
        let result = parser.parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);
        
        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_struct() {
        let mut parser = CodeParser::new()
            .expect("CodeParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");
        
        std::fs::write(&test_file, "struct TestStruct { field: i32 }")
            .expect("Writing test file should succeed");
        
        let result = parser.parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);
        
        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestStruct");
        assert_eq!(symbol.kind, SymbolKind::Struct);
        assert_eq!(symbol.visibility, Visibility::Private);
    }
}
