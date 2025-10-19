//! Go language parser
//!
//! Provides tree-sitter based parsing for Go source code,
//! including symbol extraction and deterministic hashing.

use crate::parsers::{utils, LanguageParser};
use crate::types::{Language, ParseResult, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use std::path::Path;
use tree_sitter::{Language as TSLanguage, Parser, Query, QueryCursor};

/// Go-specific parser implementation
pub struct GoParser {
    /// Tree-sitter parser
    parser: Parser,
    /// Go language
    #[allow(dead_code)]
    go_lang: TSLanguage,
    /// Query for function declarations
    function_query: Query,
    /// Query for method declarations
    method_query: Query,
    /// Query for struct type declarations
    struct_query: Query,
    /// Query for interface type declarations
    interface_query: Query,
    /// Query for type declarations
    type_query: Query,
    /// Query for variable declarations
    variable_query: Query,
    /// Query for constant declarations
    const_query: Query,
    /// Query for package declarations
    package_query: Query,
    /// Query for import declarations
    import_query: Query,
}

impl GoParser {
    /// Create a new Go parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let go_lang = tree_sitter_go::language();

        parser
            .set_language(go_lang)
            .map_err(|e| AosError::Parse(format!("Failed to set Go language: {}", e)))?;

        // Define queries for different symbol types
        let function_query = Query::new(
            go_lang,
            r#"
            (function_declaration
                name: (identifier) @name
                parameters: (parameter_list)? @params
                result: (parameter_list)? @result
            ) @function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create function query: {}", e)))?;

        let method_query = Query::new(
            go_lang,
            r#"
            (method_declaration
                name: (field_identifier) @name
                parameters: (parameter_list)? @params
                result: (parameter_list)? @result
            ) @method
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create method query: {}", e)))?;

        let struct_query = Query::new(
            go_lang,
            r#"
            (type_declaration
                (type_spec
                    name: (type_identifier) @name
                    type: (struct_type) @struct_type
                ) @type_spec
            ) @type_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create struct query: {}", e)))?;

        let interface_query = Query::new(
            go_lang,
            r#"
            (type_declaration
                (type_spec
                    name: (type_identifier) @name
                    type: (interface_type) @interface_type
                ) @type_spec
            ) @type_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create interface query: {}", e)))?;

        let type_query = Query::new(
            go_lang,
            r#"
            (type_declaration
                (type_spec
                    name: (type_identifier) @name
                    type: (type_identifier) @type
                ) @type_spec
            ) @type_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create type query: {}", e)))?;

        let variable_query = Query::new(
            go_lang,
            r#"
            (var_declaration
                (var_spec
                    name: (identifier) @name
                    type: (type_identifier)? @type
                    value: (expression_list)? @value
                ) @var_spec
            ) @var_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create variable query: {}", e)))?;

        let const_query = Query::new(
            go_lang,
            r#"
            (const_declaration
                (const_spec
                    name: (identifier) @name
                    type: (type_identifier)? @type
                    value: (expression_list)? @value
                ) @const_spec
            ) @const_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create const query: {}", e)))?;

        let package_query = Query::new(
            go_lang,
            r#"
            (package_clause
                name: (package_identifier) @name
            ) @package
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create package query: {}", e)))?;

        let import_query = Query::new(
            go_lang,
            r#"
            (import_declaration
                (import_spec
                    name: (package_identifier)? @name
                    path: (interpreted_string_literal) @path
                ) @import_spec
            ) @import_declaration
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create import query: {}", e)))?;

        Ok(Self {
            parser,
            go_lang,
            function_query,
            method_query,
            struct_query,
            interface_query,
            type_query,
            variable_query,
            const_query,
            package_query,
            import_query,
        })
    }

    /// Parse a single Go source file
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
        self.extract_methods(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_structs(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_interfaces(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_types(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_variables(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_constants(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_packages(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_imports(&tree, &source_code, file_path, &mut symbols)?;

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
            let mut result = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => params = Some(text),
                    2 => result = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Go);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Function,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                // Combine parameters and result into signature
                let mut signature_parts = Vec::new();
                if let Some(params) = params {
                    signature_parts.push(params);
                }
                if let Some(result) = result {
                    signature_parts.push(result);
                }
                if !signature_parts.is_empty() {
                    symbol = symbol.with_signature(signature_parts.join(" "));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract method declarations
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
            let mut name = None;
            let mut params = None;
            let mut result = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => params = Some(text),
                    2 => result = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Go);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Method,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                // Combine parameters and result into signature
                let mut signature_parts = Vec::new();
                if let Some(params) = params {
                    signature_parts.push(params);
                }
                if let Some(result) = result {
                    signature_parts.push(result);
                }
                if !signature_parts.is_empty() {
                    symbol = symbol.with_signature(signature_parts.join(" "));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract struct type declarations
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
            let mut struct_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => struct_type = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Go);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Struct,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(struct_type) = struct_type {
                    symbol = symbol.with_signature(struct_type);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract interface type declarations
    fn extract_interfaces(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.interface_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut interface_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => interface_type = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Go);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Trait, // Map Go interfaces to Trait kind
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(interface_type) = interface_type {
                    symbol = symbol.with_signature(interface_type);
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract type declarations
    fn extract_types(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.type_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut type_annotation = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => type_annotation = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::Go);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Type,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(type_annotation) = type_annotation {
                    symbol = symbol.with_signature(type_annotation);
                }

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
            let mut type_annotation = None;
            let mut _value = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => type_annotation = Some(text),
                    2 => _value = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                // Only treat uppercase names as exported variables
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    let span = utils::node_to_span(mat.captures[0].node);
                    let visibility = utils::parse_visibility(&name, Language::Go);
                    let mut symbol = utils::create_symbol_node(
                        name,
                        SymbolKind::Static,
                        Language::Go,
                        span,
                        file_path,
                    )
                    .with_visibility(visibility);

                    if let Some(type_annotation) = type_annotation {
                        symbol = symbol.with_signature(type_annotation);
                    }

                    symbols.push(symbol);
                }
            }
        }

        Ok(())
    }

    /// Extract constant declarations
    fn extract_constants(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.const_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;
            let mut type_annotation = None;
            let mut _value = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => type_annotation = Some(text),
                    2 => _value = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                // Only treat uppercase names as exported constants
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    let span = utils::node_to_span(mat.captures[0].node);
                    let visibility = utils::parse_visibility(&name, Language::Go);
                    let mut symbol = utils::create_symbol_node(
                        name,
                        SymbolKind::Const,
                        Language::Go,
                        span,
                        file_path,
                    )
                    .with_visibility(visibility);

                    if let Some(type_annotation) = type_annotation {
                        symbol = symbol.with_signature(type_annotation);
                    }

                    symbols.push(symbol);
                }
            }
        }

        Ok(())
    }

    /// Extract package declarations
    fn extract_packages(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.package_query, tree.root_node(), source.as_bytes());

        for mat in matches {
            let mut name = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                if capture.index == 0 {
                    name = Some(text)
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let symbol = utils::create_symbol_node(
                    format!("package {}", name),
                    SymbolKind::Module,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract import declarations
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
            let mut name = None;
            let mut path = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => path = Some(text),
                    _ => {}
                }
            }

            if let Some(path) = path {
                let span = utils::node_to_span(mat.captures[1].node);
                let import_name = if let Some(name) = name {
                    format!("import {} {}", name, path)
                } else {
                    format!("import {}", path)
                };

                let symbol = utils::create_symbol_node(
                    import_name,
                    SymbolKind::Module,
                    Language::Go,
                    span,
                    file_path,
                )
                .with_visibility(Visibility::Public);

                symbols.push(symbol);
            }
        }

        Ok(())
    }
}

impl LanguageParser for GoParser {
    fn language(&self) -> Language {
        Language::Go
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        self.parse_file(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["go"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_go_parser_creation() {
        let parser = GoParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(
            &test_file,
            "package main\n\nfunc testFunction() int {\n    return 42\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2); // package + function

        let function_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "testFunction")
            .expect("Should find function symbol");
        assert_eq!(function_symbol.kind, SymbolKind::Function);
        assert_eq!(function_symbol.language, Language::Go);
        assert_eq!(function_symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_struct() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(
            &test_file,
            "package main\n\ntype TestStruct struct {\n    Field int\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2); // package + struct

        let struct_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "TestStruct")
            .expect("Should find struct symbol");
        assert_eq!(struct_symbol.kind, SymbolKind::Struct);
        assert_eq!(struct_symbol.language, Language::Go);
    }

    #[test]
    fn test_parse_interface() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(
            &test_file,
            "package main\n\ntype TestInterface interface {\n    Method() int\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2); // package + interface

        let interface_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "TestInterface")
            .expect("Should find interface symbol");
        assert_eq!(interface_symbol.kind, SymbolKind::Trait);
        assert_eq!(interface_symbol.language, Language::Go);
    }

    #[test]
    fn test_parse_method() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(&test_file, "package main\n\ntype TestStruct struct {}\n\nfunc (t TestStruct) Method() int {\n    return 42\n}")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 3); // package + struct + method

        let method_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "Method")
            .expect("Should find method symbol");
        assert_eq!(method_symbol.kind, SymbolKind::Method);
        assert_eq!(method_symbol.language, Language::Go);
    }

    #[test]
    fn test_parse_import() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(&test_file, "package main\n\nimport \"fmt\"")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2); // package + import

        let import_symbols: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module && s.name.contains("fmt"))
            .collect();
        assert!(!import_symbols.is_empty());
    }

    #[test]
    fn test_parse_private_function() {
        let mut parser = GoParser::new().expect("GoParser creation should succeed");
        let temp_dir = TempDir::new().expect("Test temp directory creation should succeed");
        let test_file = temp_dir.path().join("test.go");

        std::fs::write(
            &test_file,
            "package main\n\nfunc privateFunction() int {\n    return 42\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert!(result.symbols.len() >= 2); // package + function

        let function_symbol = result
            .symbols
            .iter()
            .find(|s| s.name == "privateFunction")
            .expect("Should find function symbol");
        assert_eq!(function_symbol.visibility, Visibility::Private);
    }
}
