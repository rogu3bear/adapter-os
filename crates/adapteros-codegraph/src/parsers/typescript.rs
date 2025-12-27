//! TypeScript language parser
//!
//! Provides tree-sitter based parsing for TypeScript source code,
//! including symbol extraction and deterministic hashing.

use crate::parsers::{utils, LanguageParser};
use crate::types::{Language, ParseResult, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Parser, Query, QueryCursor};

/// TypeScript-specific parser implementation
pub struct TypeScriptParser {
    /// Tree-sitter parser
    parser: Parser,
    /// TypeScript language
    #[allow(dead_code)]
    typescript_lang: TSLanguage,
    /// Query for function declarations
    function_query: Query,
    /// Query for arrow functions
    arrow_function_query: Query,
    /// Query for class declarations
    class_query: Query,
    /// Query for interface declarations
    interface_query: Query,
    /// Query for type alias declarations
    type_alias_query: Query,
    /// Query for enum declarations
    enum_query: Query,
    /// Query for import statements
    import_query: Query,
    /// Query for export statements
    export_query: Query,
    /// Query for variable declarations
    variable_query: Query,
    /// Query for method definitions (functions within classes)
    method_query: Query,
}

impl TypeScriptParser {
    /// Create a new TypeScript parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let typescript_lang =
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);

        parser
            .set_language(&typescript_lang)
            .map_err(|e| AosError::Parse(format!("Failed to set TypeScript language: {}", e)))?;

        // Define queries for different symbol types
        let function_query = Query::new(
            &typescript_lang,
            r#"
            (function_declaration
                name: (identifier) @name
                parameters: (formal_parameters) @params
                return_type: (type_annotation)? @return_type
            ) @function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create function query: {}", e)))?;

        let arrow_function_query = Query::new(
            &typescript_lang,
            r#"
            (arrow_function
                parameters: (formal_parameters) @params
                return_type: (type_annotation)? @return_type
            ) @arrow_function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create arrow function query: {}", e)))?;

        let class_query = Query::new(
            &typescript_lang,
            r#"
            (class_declaration
                name: (type_identifier) @name
                extends: (extends_clause)? @extends
                implements: (implements_clause)? @implements
            ) @class
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create class query: {}", e)))?;

        let interface_query = Query::new(
            &typescript_lang,
            r#"
            (interface_declaration
                name: (type_identifier) @name
                extends: (extends_clause)? @extends
            ) @interface
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create interface query: {}", e)))?;

        let type_alias_query = Query::new(
            &typescript_lang,
            r#"
            (type_alias_declaration
                name: (type_identifier) @name
                type_parameters: (type_parameters)? @type_params
                type: (type_annotation) @type
            ) @type_alias
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create type alias query: {}", e)))?;

        let enum_query = Query::new(
            &typescript_lang,
            r#"
            (enum_declaration
                name: (identifier) @name
            ) @enum
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create enum query: {}", e)))?;

        let import_query = Query::new(
            &typescript_lang,
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
            &typescript_lang,
            r#"
            (export_statement
                (export_clause)? @export_clause
            ) @export
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create export query: {}", e)))?;

        let variable_query = Query::new(
            &typescript_lang,
            r#"
            (variable_declaration
                (variable_declarator
                    name: (identifier) @name
                    type: (type_annotation)? @type
                    value: (expression)? @value
                ) @declarator
            ) @variable
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create variable query: {}", e)))?;

        let method_query = Query::new(
            &typescript_lang,
            r#"
            (class_declaration
                name: (type_identifier) @class_name
                body: (class_body
                    (method_definition
                        name: (property_identifier) @method_name
                        parameters: (formal_parameters) @params
                        return_type: (type_annotation)? @return_type
                    ) @method
                )
            ) @class_with_methods
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create method query: {}", e)))?;

        Ok(Self {
            parser,
            typescript_lang,
            function_query,
            arrow_function_query,
            class_query,
            interface_query,
            type_alias_query,
            enum_query,
            import_query,
            export_query,
            variable_query,
            method_query,
        })
    }

    /// Parse a single TypeScript source file
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
        self.extract_classes(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_interfaces(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_type_aliases(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_enums(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_methods(&tree, &source_code, file_path, &mut symbols)?;
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
        let mut matches = cursor.matches(&self.function_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut params = None;
            let mut _return_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => params = Some(text),
                    2 => _return_type = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::TypeScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Function,
                    Language::TypeScript,
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
        let mut matches = cursor.matches(
            &self.arrow_function_query,
            tree.root_node(),
            source.as_bytes(),
        );

        while let Some(mat) = matches.next() {
            let mut params = None;
            let mut _return_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => params = Some(text),
                    1 => _return_type = Some(text),
                    _ => {}
                }
            }

            // Generate a name for arrow functions based on their position
            let span = utils::node_to_span(mat.captures[0].node);
            let name = format!("arrow_function_{}_{}", span.start_line, span.start_column);

            let mut symbol = utils::create_symbol_node(
                name,
                SymbolKind::Function,
                Language::TypeScript,
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

    /// Extract class declarations
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
            let mut extends = None;
            let mut implements = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => extends = Some(text),
                    2 => implements = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::TypeScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Struct, // Map TypeScript classes to Struct kind
                    Language::TypeScript,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                // Add inheritance information to signature
                let mut signature_parts = Vec::new();
                if let Some(extends) = extends {
                    signature_parts.push(format!("extends {}", extends));
                }
                if let Some(implements) = implements {
                    signature_parts.push(format!("implements {}", implements));
                }
                if !signature_parts.is_empty() {
                    symbol = symbol.with_signature(signature_parts.join(", "));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract interface declarations
    fn extract_interfaces(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&self.interface_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut extends = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => extends = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::TypeScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Trait, // Map TypeScript interfaces to Trait kind
                    Language::TypeScript,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                if let Some(extends) = extends {
                    symbol = symbol.with_signature(format!("extends {}", extends));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract type alias declarations
    fn extract_type_aliases(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&self.type_alias_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut type_params = None;
            let mut type_annotation = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => name = Some(text),
                    1 => type_params = Some(text),
                    2 => type_annotation = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::TypeScript);
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Type,
                    Language::TypeScript,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                // Combine type parameters and type annotation
                let mut signature_parts = Vec::new();
                if let Some(type_params) = type_params {
                    signature_parts.push(type_params);
                }
                if let Some(type_annotation) = type_annotation {
                    signature_parts.push(type_annotation);
                }
                if !signature_parts.is_empty() {
                    symbol = symbol.with_signature(signature_parts.join(" = "));
                }

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract enum declarations
    fn extract_enums(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.enum_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                if capture.index == 0 {
                    name = Some(text)
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(mat.captures[0].node);
                let visibility = utils::parse_visibility(&name, Language::TypeScript);
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Enum,
                    Language::TypeScript,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

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
            let mut _return_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => class_name = Some(text),
                    1 => method_name = Some(text),
                    2 => params = Some(text),
                    3 => _return_type = Some(text),
                    _ => {}
                }
            }

            if let (Some(class_name), Some(method_name)) = (class_name, method_name) {
                let span = utils::node_to_span(mat.captures[1].node); // Use method node span
                let visibility = utils::parse_visibility(&method_name, Language::TypeScript);
                let mut symbol = utils::create_symbol_node(
                    format!("{}.{}", class_name, method_name),
                    SymbolKind::Method,
                    Language::TypeScript,
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
        let mut matches = cursor.matches(&self.import_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
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
                    Language::TypeScript,
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
        let mut matches = cursor.matches(&self.export_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
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
                    Language::TypeScript,
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
        let mut matches = cursor.matches(&self.variable_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
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
                // Only treat uppercase names as constants
                if name.chars().all(|c| c.is_uppercase() || c == '_') {
                    let span = utils::node_to_span(mat.captures[0].node);
                    let visibility = utils::parse_visibility(&name, Language::TypeScript);
                    let mut symbol = utils::create_symbol_node(
                        name,
                        SymbolKind::Const,
                        Language::TypeScript,
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
}

impl LanguageParser for TypeScriptParser {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        self.parse_file(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("Test temp directory creation should succeed")
    }

    #[test]
    fn test_typescript_parser_creation() {
        let parser = TypeScriptParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

        std::fs::write(
            &test_file,
            "function testFunction(): number {\n    return 42;\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "testFunction");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::TypeScript);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_class() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

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
        assert_eq!(class_symbol.language, Language::TypeScript);
    }

    #[test]
    fn test_parse_interface() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

        std::fs::write(
            &test_file,
            "interface TestInterface {\n    prop: string;\n}",
        )
        .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestInterface");
        assert_eq!(symbol.kind, SymbolKind::Trait);
        assert_eq!(symbol.language, Language::TypeScript);
    }

    #[test]
    fn test_parse_type_alias() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

        std::fs::write(&test_file, "type TestType = string | number;")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestType");
        assert_eq!(symbol.kind, SymbolKind::Type);
        assert_eq!(symbol.language, Language::TypeScript);
    }

    #[test]
    fn test_parse_import() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

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
    fn test_parse_enum() {
        let mut parser = TypeScriptParser::new().expect("TypeScriptParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.ts");

        std::fs::write(&test_file, "enum TestEnum {\n    VALUE1,\n    VALUE2\n}")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestEnum");
        assert_eq!(symbol.kind, SymbolKind::Enum);
        assert_eq!(symbol.language, Language::TypeScript);
    }
}
