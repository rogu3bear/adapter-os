//! Python language parser
//!
//! Provides tree-sitter based parsing for Python source code,
//! including symbol extraction and deterministic hashing.

use crate::parsers::{utils, LanguageParser};
use crate::types::{Language, ParseResult, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Parser, Query, QueryCursor};

fn python_language() -> TSLanguage {
    tree_sitter::Language::new(tree_sitter_python::LANGUAGE)
}

/// Python-specific parser implementation
pub struct PythonParser {
    /// Tree-sitter parser
    parser: Parser,
    /// Python language
    #[allow(dead_code)]
    python_lang: TSLanguage,
    /// Query for function definitions
    function_query: Query,
    /// Query for class definitions
    class_query: Query,
    /// Query for import statements
    import_query: Query,
    /// Query for import from statements
    import_from_query: Query,
    /// Query for variable assignments
    assignment_query: Query,
    /// Query for method definitions (functions within classes)
    method_query: Query,
}

impl PythonParser {
    /// Create a new Python parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let python_lang = python_language();

        parser
            .set_language(&python_lang)
            .map_err(|e| AosError::Parse(format!("Failed to set Python language: {}", e)))?;
        let function_query = Query::new(
            &python_lang,
            r#"
            (function_definition
                ("async")? @async
                name: (identifier) @name
                parameters: (parameters) @params
                return_type: (type)? @return_type
            ) @function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create function query: {}", e)))?;

        let class_query = Query::new(
            &python_lang,
            r#"
            (class_definition
                name: (identifier) @name
                superclasses: (argument_list)? @superclasses
            ) @class
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create class query: {}", e)))?;

        let import_query = Query::new(
            &python_lang,
            r#"
            (import_statement
                (dotted_name) @module_name
            ) @import
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create import query: {}", e)))?;

        let import_from_query = Query::new(
            &python_lang,
            r#"
            (import_from_statement
                module_name: (dotted_name) @module_name
                name: (dotted_name) @imported_name
            ) @import_from
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create import from query: {}", e)))?;

        let assignment_query = Query::new(
            &python_lang,
            r#"
            (assignment
                left: (identifier) @name
                right: (expression) @value
            ) @assignment
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create assignment query: {}", e)))?;

        let method_query = Query::new(
            &python_lang,
            r#"
            (class_definition
                name: (identifier) @class_name
                body: (block
                    (function_definition
                        name: (identifier) @method_name
                        parameters: (parameters) @params
                    ) @method
                )
            ) @class_with_methods
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create method query: {}", e)))?;

        Ok(Self {
            parser,
            python_lang,
            function_query,
            class_query,
            import_query,
            import_from_query,
            assignment_query,
            method_query,
        })
    }

    /// Parse a single Python source file
    pub fn parse_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<ParseResult> {
        let file_path = file_path.as_ref();
        let source_code = std::fs::read_to_string(file_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        // Parse the source code
        let tree = self
            .parser
            .parse(&source_code, None)
            .ok_or_else(|| AosError::Parse("Failed to parse source code".to_string()))?;

        let mut symbols = Vec::new();

        // Extract symbols using queries
        self.extract_functions(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_classes(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_methods(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_imports(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_assignments(&tree, &source_code, file_path, &mut symbols)?;

        // Compute content hash for determinism
        let _content_hash = adapteros_core::B3Hash::hash(source_code.as_bytes());

        Ok(ParseResult {
            file_path: file_path.to_path_buf(),
            symbols,
        })
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
        let mut matches = cursor.matches(&self.function_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut params = None;
            let mut _return_type = None;
            let mut is_async = false;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => is_async = true,
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    2 => params = Some(text),
                    3 => _return_type = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let visibility = utils::parse_visibility(&name, Language::Python);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Function,
                    Language::Python,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if is_async {
                    symbol = symbol.mark_async();
                }

                if let Some(params) = params {
                    symbol = symbol.with_signature(params);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract class definitions
    fn extract_classes(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.class_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut superclasses = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => superclasses = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Python);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Struct, // Map Python classes to Struct kind
                    Language::Python,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(superclasses) = superclasses {
                    symbol = symbol.with_signature(superclasses);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract method definitions (functions within classes)
    fn extract_methods(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.method_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut class_name = None;
            let mut method_name = None;
            let mut params = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => class_name = Some(text),
                    1 => method_name = Some(text),
                    2 => params = Some(text),
                    _ => {}
                }
            }

            if let (Some(class_name), Some(method_name)) = (class_name, method_name) {
                let span = utils::node_to_span(mat.captures[1].node); // Use method node span
                let visibility = utils::parse_visibility(&method_name, Language::Python);
                let mut symbol = utils::create_symbol_node(
                    format!("{}.{}", class_name, method_name),
                    SymbolKind::Method,
                    Language::Python,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(params) = params {
                    symbol = symbol.with_signature(params);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract import statements
    fn extract_imports(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();

        // Extract regular imports
        let mut import_matches =
            cursor.matches(&self.import_query, tree.root_node(), source.as_bytes());
        while let Some(mat) = import_matches.next() {
            let mut module_name = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                if capture.index == 0 {
                    module_name = Some(text)
                }
            }

            if let Some(module_name) = module_name {
                let span = utils::node_to_span(mat.captures[0].node);
                let symbol = utils::create_symbol_node(
                    format!("import {}", module_name),
                    SymbolKind::Module,
                    Language::Python,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        // Extract import from statements
        let mut import_from_matches =
            cursor.matches(&self.import_from_query, tree.root_node(), source.as_bytes());
        while let Some(mat) = import_from_matches.next() {
            let mut module_name = None;
            let mut imported_name = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => module_name = Some(text),
                    1 => imported_name = Some(text),
                    _ => {}
                }
            }

            if let Some(imported_name) = imported_name {
                let span = utils::node_to_span(mat.captures[1].node);
                let symbol = utils::create_symbol_node(
                    format!(
                        "from {} import {}",
                        module_name.unwrap_or_default(),
                        imported_name
                    ),
                    SymbolKind::Module,
                    Language::Python,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract variable assignments (constants)
    fn extract_assignments(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&self.assignment_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut value = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => value = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                // Only treat uppercase names as constants
                if name.chars().all(|c| c.is_uppercase() || c == '_') {
                    let span = utils::node_to_span(mat.captures[0].node);
                    let visibility = utils::parse_visibility(&name, Language::Python);
                    let mut symbol = utils::create_symbol_node(
                        name,
                        SymbolKind::Const,
                        Language::Python,
                        span,
                        file_path,
                    )
                    .with_visibility(visibility);

                    if let Some(value) = value {
                        symbol = symbol.with_signature(value);
                    }

                    symbols.push(symbol);
                }
            }
        }

        Ok(())
    }
}

impl LanguageParser for PythonParser {
    fn language(&self) -> Language {
        Language::Python
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        self.parse_file(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["py"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("Test temp directory creation should succeed")
    }

    #[test]
    fn test_python_parser_creation() {
        let parser = PythonParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = PythonParser::new().expect("PythonParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.py");

        std::fs::write(&test_file, "def test_function():\n    return 42")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::Python);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_async_function() {
        let mut parser = PythonParser::new().expect("PythonParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.py");

        std::fs::write(&test_file, "async def async_function():\n    return 42")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "async_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::Python);
        assert!(symbol.is_async);
    }

    #[test]
    fn test_parse_class() {
        let mut parser = PythonParser::new().expect("PythonParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.py");

        std::fs::write(
            &test_file,
            "class TestClass:\n    def __init__(self):\n        pass",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(!result.symbols.is_empty());

        let class_symbol = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Struct)
            .expect("Should find class symbol");
        assert_eq!(class_symbol.name, "TestClass");
        assert_eq!(class_symbol.language, Language::Python);
    }

    #[test]
    fn test_parse_import() {
        let mut parser = PythonParser::new().expect("PythonParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.py");

        std::fs::write(&test_file, "import os\nfrom sys import path")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2);

        let import_symbols: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(import_symbols.len(), 2);
    }

    #[test]
    fn test_parse_private_function() {
        let mut parser = PythonParser::new().expect("PythonParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.py");

        std::fs::write(&test_file, "def _private_function():\n    pass")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "_private_function");
        assert_eq!(symbol.visibility, Visibility::Private);
    }
}
