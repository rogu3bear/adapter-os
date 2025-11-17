//! JavaScript language parser
//!
//! Provides tree-sitter based parsing for JavaScript source code,
//! including symbol extraction and deterministic hashing.

use crate::parsers::{utils, LanguageParser};
use crate::types::{Language, ParseResult, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use std::path::Path;
use tree_sitter::{Language as TSLanguage, Parser, Query, QueryCursor};

/// JavaScript-specific parser implementation
pub struct JavaScriptParser {
    /// Tree-sitter parser
    parser: Parser,
    /// JavaScript language
    #[allow(dead_code)]
    javascript_lang: TSLanguage,
    /// Query for function declarations
    function_query: Query,
    /// Query for arrow functions
    arrow_function_query: Query,
    /// Query for function expressions
    function_expression_query: Query,
    /// Query for class declarations
    class_query: Query,
    /// Query for method definitions (functions within classes)
    method_query: Query,
    /// Query for import statements
    import_query: Query,
    /// Query for export statements
    export_query: Query,
    /// Query for variable declarations
    variable_query: Query,
    /// Query for object method definitions
    object_method_query: Query,
}

impl JavaScriptParser {
    /// Create a new JavaScript parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let javascript_lang = tree_sitter_javascript::language();

        parser
            .set_language(javascript_lang)
            .map_err(|e| AosError::Parse(format!("Failed to set JavaScript language: {}", e)))?;

        // Define queries for different symbol types
        let function_query = Query::new(
            javascript_lang,
            r#"
            (function_declaration
                name: (identifier) @name
                parameters: (formal_parameters) @params
            ) @function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create function query: {}", e)))?;

        let arrow_function_query = Query::new(
            javascript_lang,
            r#"
            (arrow_function
                parameters: (formal_parameters) @params
            ) @arrow_function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create arrow function query: {}", e)))?;

        let function_expression_query = Query::new(
            javascript_lang,
            r#"
            (function_expression
                name: (identifier)? @name
                parameters: (formal_parameters) @params
            ) @function_expression
            "#,
        )
        .map_err(|e| {
            AosError::Parse(format!("Failed to create function expression query: {}", e))
        })?;

        let class_query = Query::new(
            javascript_lang,
            r#"
            (class_declaration
                name: (identifier) @name
                superclass: (identifier)? @superclass
            ) @class
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create class query: {}", e)))?;

        let method_query = Query::new(
            javascript_lang,
            r#"
            (class_declaration
                name: (identifier) @class_name
                body: (class_body
                    (method_definition
                        name: (property_identifier) @method_name
                        parameters: (formal_parameters) @params
                    ) @method
                )
            ) @class_with_methods
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create method query: {}", e)))?;

        let import_query = Query::new(
            javascript_lang,
            r#"
            (import_statement
                source: (string) @source
                (import_clause
                    (namespace_import)? @namespace
                    (named_imports)? @named
                )? @import_clause
            ) @import
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create import query: {}", e)))?;

        let export_query = Query::new(
            javascript_lang,
            r#"
            (export_statement
                (export_clause)? @export_clause
            ) @export
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create export query: {}", e)))?;

        let variable_query = Query::new(
            javascript_lang,
            r#"
            (variable_declaration
                (variable_declarator
                    name: (identifier) @name
                    value: (expression)? @value
                ) @declarator
            ) @variable
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create variable query: {}", e)))?;

        let object_method_query = Query::new(
            javascript_lang,
            r#"
            (object_expression
                (pair
                    key: (property_identifier) @method_name
                    value: (function_expression
                        parameters: (formal_parameters) @params
                    ) @method
                ) @pair
            ) @object_with_methods
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create object method query: {}", e)))?;

        Ok(Self {
            parser,
            javascript_lang,
            function_query,
            arrow_function_query,
            function_expression_query,
            class_query,
            method_query,
            import_query,
            export_query,
            variable_query,
            object_method_query,
        })
    }

    /// Parse a single JavaScript source file
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
        self.extract_arrow_functions(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_function_expressions(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_classes(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_methods(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_object_methods(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_imports(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_exports(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_variables(&tree, &source_code, file_path, &mut symbols)?;

        // Compute content hash for determinism
        let _content_hash = adapteros_core::B3Hash::hash(source_code.as_bytes());

        Ok(ParseResult {
            file_path: file_path.to_path_buf(),
            symbols,
        })
    }

    /// Extract function declarations
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
            let mut params = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => params = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::JavaScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Function,
                    Language::JavaScript,
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

    /// Extract arrow functions
    fn extract_arrow_functions(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(
            &self.arrow_function_query,
            tree.root_node(),
            source.as_bytes(),
        );

        for mat in matches {
            let mut params = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                if capture.index == 0 {
                    params = Some(text)
                }
            }

            // Generate a name for arrow functions based on their position
            let span = utils::node_to_span(mat.captures[0].node);
            let name = format!("arrow_function_{}_{}", span.start_line, span.start_column);

            let mut symbol = utils::create_symbol_node(
                name,
                SymbolKind::Function,
                Language::JavaScript,
                span,
                file_path,
            )
            .with_visibility(Visibility::Private);

            if let Some(params) = params {
                symbol = symbol.with_signature(params);
            }

            symbols.push(symbol);
        }

        Ok(())
    }

    /// Extract function expressions
    fn extract_function_expressions(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(
            &self.function_expression_query,
            tree.root_node(),
            source.as_bytes(),
        );

        for mat in matches {
            let mut name = None;
            let mut params = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => params = Some(text),
                    _ => {}
                }
            }

            // Use the name if available, otherwise generate one
            let name = name.unwrap_or_else(|| {
                let span = utils::node_to_span(mat.captures[0].node);
                format!(
                    "function_expression_{}_{}",
                    span.start_line, span.start_column
                )
            });

            let span = utils::node_to_span(mat.captures[0].node);
            let visibility = utils::parse_visibility(&name, Language::JavaScript);
            let mut symbol = utils::create_symbol_node(
                name,
                SymbolKind::Function,
                Language::JavaScript,
                span,
                file_path,
            )
            .with_visibility(visibility);

            if let Some(params) = params {
                symbol = symbol.with_signature(params);
            }

            symbols.push(symbol);
        }

        Ok(())
    }

    /// Extract class declarations
    fn extract_classes(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.class_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut superclass = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => superclass = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::JavaScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Struct, // Map JavaScript classes to Struct kind
                    Language::JavaScript,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(superclass) = superclass {
                    symbol = symbol.with_signature(format!("extends {}", superclass));
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
        let matches = cursor.matches(&self.method_query, tree.root_node(), source.as_bytes());

        for mat in matches {
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
                let visibility = utils::parse_visibility(&method_name, Language::JavaScript);
                let mut symbol = utils::create_symbol_node(
                    format!("{}.{}", class_name, method_name),
                    SymbolKind::Method,
                    Language::JavaScript,
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

    /// Extract object method definitions
    fn extract_object_methods(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(
            &self.object_method_query,
            tree.root_node(),
            source.as_bytes(),
        );

        for mat in matches {
            let mut method_name = None;
            let mut params = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => method_name = Some(text),
                    1 => params = Some(text),
                    _ => {}
                }
            }

            if let Some(method_name) = method_name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&method_name, Language::JavaScript);
                let mut symbol = utils::create_symbol_node(
                    format!("object.{}", method_name),
                    SymbolKind::Method,
                    Language::JavaScript,
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
        let matches = cursor.matches(&self.import_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut source_module = None;
            let mut namespace = None;
            let mut named_imports = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => source_module = Some(text),
                    1 => namespace = Some(text),
                    2 => named_imports = Some(text),
                    _ => {}
                }
            }

            if let Some(source_module) = source_module {
                let span = utils::node_to_span(mat.captures[0].node);
                let import_name = if let Some(namespace) = namespace {
                    format!("import * as {} from {}", namespace, source_module)
                } else if let Some(named_imports) = named_imports {
                    format!("import {} from {}", named_imports, source_module)
                } else {
                    format!("import {}", source_module)
                };

                let symbol = utils::create_symbol_node(
                    import_name,
                    SymbolKind::Module,
                    Language::JavaScript,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract export statements
    fn extract_exports(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.export_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut export_clause = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                if capture.index == 0 {
                    export_clause = Some(text)
                }
            }

            if let Some(export_clause) = export_clause {
                let span = utils::node_to_span(mat.captures[0].node);
                let symbol = utils::create_symbol_node(
                    format!("export {}", export_clause),
                    SymbolKind::Module,
                    Language::JavaScript,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract variable declarations
    fn extract_variables(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.variable_query, tree.root_node(), source.as_bytes());

        for mat in matches {
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
                    let visibility = utils::parse_visibility(&name, Language::JavaScript);
                    let mut symbol = utils::create_symbol_node(
                        name,
                        SymbolKind::Const,
                        Language::JavaScript,
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

impl LanguageParser for JavaScriptParser {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        self.parse_file(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["js", "jsx"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_javascript_parser_creation() {
        let parser = JavaScriptParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(&test_file, "function testFunction() {\n    return 42;\n}")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "testFunction");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::JavaScript);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_arrow_function() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(
            &test_file,
            "const arrowFunction = () => {\n    return 42;\n};",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 1);

        let arrow_function = result
            .symbols
            .iter()
            .find(|s| s.name.starts_with("arrow_function_"))
            .expect("Should find arrow function");
        assert_eq!(arrow_function.kind, SymbolKind::Function);
        assert_eq!(arrow_function.language, Language::JavaScript);
    }

    #[test]
    fn test_parse_class() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(&test_file, "class TestClass {\n    constructor() {}\n}")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 1);

        let class_symbol = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Struct)
            .expect("Should find class symbol");
        assert_eq!(class_symbol.name, "TestClass");
        assert_eq!(class_symbol.language, Language::JavaScript);
    }

    #[test]
    fn test_parse_import() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(&test_file, "import { Component } from 'react';")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 1);

        let import_symbols: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert!(!import_symbols.is_empty());
    }

    #[test]
    fn test_parse_function_expression() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(
            &test_file,
            "const myFunction = function namedFunction() {};",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 1);

        let function_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "namedFunction")
            .expect("Should find named function expression");
        assert_eq!(function_symbol.kind, SymbolKind::Function);
        assert_eq!(function_symbol.language, Language::JavaScript);
    }

    #[test]
    fn test_parse_object_method() {
        let mut parser = JavaScriptParser::new().expect("JavaScriptParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.js");

        std::fs::write(&test_file, "const obj = {\n    method() {}\n};")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 1);

        let method_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "object.method")
            .expect("Should find object method");
        assert_eq!(method_symbol.kind, SymbolKind::Method);
        assert_eq!(method_symbol.language, Language::JavaScript);
    }
}
