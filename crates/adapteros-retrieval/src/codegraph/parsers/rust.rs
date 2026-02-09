//! Rust language parser
//!
//! Provides tree-sitter based parsing for Rust source code,
//! including symbol extraction and deterministic hashing.

use crate::parsers::{utils, LanguageParser};
use crate::types::{Language, ParseResult, SymbolKind, SymbolNode, Visibility};
use adapteros_core::{AosError, Result};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Parser, Query, QueryCursor};

fn rust_language() -> TSLanguage {
    tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)
}

/// Rust-specific parser implementation
pub struct RustParser {
    /// Tree-sitter parser
    parser: Parser,
    /// Rust language
    #[allow(dead_code)]
    rust_lang: TSLanguage,
    /// Query for function definitions
    function_query: Query,
    /// Query for struct definitions
    struct_query: Query,
    /// Query for trait definitions
    trait_query: Query,
    /// Query for impl blocks
    impl_query: Query,
    /// Query for enum definitions
    enum_query: Query,
    /// Query for const definitions
    const_query: Query,
    /// Query for static definitions
    static_query: Query,
    /// Query for type alias definitions
    type_query: Query,
    /// Query for macro definitions
    macro_query: Query,
    /// Query for module declarations
    module_query: Query,
}

impl RustParser {
    /// Create a new Rust parser
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let rust_lang = rust_language();

        parser
            .set_language(&rust_lang)
            .map_err(|e| AosError::Parse(format!("Failed to set Rust language: {}", e)))?;

        // Define queries for different symbol types
        let function_query = Query::new(
            &rust_lang,
            r#"
            (function_item
                (visibility_modifier)? @visibility
                name: (identifier) @name
                parameters: (parameters) @params
                return_type: (_type)? @return_type
            ) @function
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create function query: {}", e)))?;

        let struct_query = Query::new(
            &rust_lang,
            r#"
            (struct_item
                name: (type_identifier) @name
            ) @struct
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create struct query: {}", e)))?;

        let trait_query = Query::new(
            &rust_lang,
            r#"
            (trait_item
                (visibility_modifier)? @visibility
                name: (type_identifier) @name
            ) @trait
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create trait query: {}", e)))?;

        let impl_query = Query::new(
            &rust_lang,
            r#"
            (impl_item
                trait: (type_identifier)? @trait_name
                type: (type_identifier) @type_name
            ) @impl
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create impl query: {}", e)))?;

        let enum_query = Query::new(
            &rust_lang,
            r#"
            (enum_item
                (visibility_modifier)? @visibility
                name: (type_identifier) @name
            ) @enum
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create enum query: {}", e)))?;

        let const_query = Query::new(
            &rust_lang,
            r#"
            (const_item
                (visibility_modifier)? @visibility
                name: (identifier) @name
                type: (_type)? @type
            ) @const
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create const query: {}", e)))?;

        let static_query = Query::new(
            &rust_lang,
            r#"
            (static_item
                (visibility_modifier)? @visibility
                name: (identifier) @name
                type: (_type)? @type
            ) @static
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create static query: {}", e)))?;

        let type_query = Query::new(
            &rust_lang,
            r#"
            (type_item
                (visibility_modifier)? @visibility
                name: (type_identifier) @name
                type: (_type)? @type
            ) @type_alias
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create type query: {}", e)))?;

        let macro_query = Query::new(
            &rust_lang,
            r#"
            (macro_definition
                name: (identifier) @name
            ) @macro
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create macro query: {}", e)))?;

        let module_query = Query::new(
            &rust_lang,
            r#"
            (mod_item
                (visibility_modifier)? @visibility
                name: (identifier) @name
            ) @module
            "#,
        )
        .map_err(|e| AosError::Parse(format!("Failed to create module query: {}", e)))?;

        Ok(Self {
            parser,
            rust_lang,
            function_query,
            struct_query,
            trait_query,
            impl_query,
            enum_query,
            const_query,
            static_query,
            type_query,
            macro_query,
            module_query,
        })
    }

    /// Parse a single Rust source file
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
        self.extract_structs(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_traits(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_impls(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_enums(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_consts(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_statics(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_types(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_macros(&tree, &source_code, file_path, &mut symbols)?;
        self.extract_modules(&tree, &source_code, file_path, &mut symbols)?;

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
            let mut visibility = Visibility::Private;
            let mut params = None;
            let mut _return_type = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
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
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Function,
                    Language::Rust,
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

    /// Extract struct definitions
    fn extract_structs(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.struct_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut struct_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    1 => struct_node = Some(capture.node),
                    _ => {}
                }
            }

            if let Some(name) = name {
                if let Some(struct_node) = struct_node {
                    if let Some(vis_node) =
                        utils::find_child_of_type(struct_node, "visibility_modifier")
                    {
                        let vis_text = utils::extract_text(vis_node, source);
                        visibility = utils::parse_visibility(&vis_text, Language::Rust);
                    }
                }
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Struct,
                    Language::Rust,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

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
        let mut matches = cursor.matches(&self.trait_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Trait,
                    Language::Rust,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

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
        let mut matches = cursor.matches(&self.impl_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut trait_name = None;
            let mut type_name = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => trait_name = Some(text),
                    1 => type_name = Some(text),
                    _ => {}
                }
            }

            if let Some(type_name) = type_name {
                let span = utils::node_to_span(mat.captures[0].node);
                let name = if let Some(trait_name) = trait_name {
                    format!("impl {} for {}", trait_name, type_name)
                } else {
                    format!("impl {}", type_name)
                };

                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Impl,
                    Language::Rust,
                    span,
                    file_path,
                );

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract enum definitions
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
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Enum,
                    Language::Rust,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract const definitions
    fn extract_consts(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.const_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;
            let mut type_annotation = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    2 => type_annotation = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Const,
                    Language::Rust,
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

    /// Extract static definitions
    fn extract_statics(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.static_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;
            let mut type_annotation = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    2 => type_annotation = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Static,
                    Language::Rust,
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

    /// Extract type alias definitions
    fn extract_types(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.type_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;
            let mut type_annotation = None;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    2 => type_annotation = Some(text),
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let mut symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Type,
                    Language::Rust,
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

    /// Extract macro definitions
    fn extract_macros(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.macro_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Macro,
                    Language::Rust,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                symbols.push(symbol);
            }
        }

        Ok(())
    }

    /// Extract module declarations
    fn extract_modules(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
        symbols: &mut Vec<SymbolNode>,
    ) -> Result<()> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.module_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            let mut name = None;
            let mut name_node = None;
            let mut visibility = Visibility::Private;

            for capture in mat.captures {
                let text = utils::extract_text(capture.node, source);
                match capture.index {
                    0 => visibility = utils::parse_visibility(&text, Language::Rust),
                    1 => {
                        name_node = Some(capture.node);
                        name = Some(text);
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
                let span = utils::node_to_span(name_node.unwrap_or_else(|| mat.captures[0].node));
                let symbol = utils::create_symbol_node(
                    name,
                    SymbolKind::Module,
                    Language::Rust,
                    span,
                    file_path,
                )
                .with_visibility(visibility);

                symbols.push(symbol);
            }
        }

        Ok(())
    }
}

impl LanguageParser for RustParser {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn parse_file(&mut self, path: &Path) -> Result<ParseResult> {
        self.parse_file(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["rs"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        adapteros_core::tempdir_in_var("aos-test-")
            .expect("Test temp directory creation should succeed")
    }

    #[test]
    fn test_rust_parser_creation() {
        let parser = RustParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let mut parser = RustParser::new().expect("RustParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        std::fs::write(&test_file, "pub fn test_function() -> i32 { 42 }")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::Rust);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_struct() {
        let mut parser = RustParser::new().expect("RustParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        std::fs::write(&test_file, "struct TestStruct { field: i32 }")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestStruct");
        assert_eq!(symbol.kind, SymbolKind::Struct);
        assert_eq!(symbol.language, Language::Rust);
        assert_eq!(symbol.visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_trait() {
        let mut parser = RustParser::new().expect("RustParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        std::fs::write(&test_file, "pub trait TestTrait { fn method(); }")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestTrait");
        assert_eq!(symbol.kind, SymbolKind::Trait);
        assert_eq!(symbol.language, Language::Rust);
        assert_eq!(symbol.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_enum() {
        let mut parser = RustParser::new().expect("RustParser creation should succeed");
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        std::fs::write(&test_file, "enum TestEnum { Variant1, Variant2 }")
            .expect("Writing test file should succeed");

        let result = parser
            .parse_file(&test_file)
            .expect("Parsing test file should succeed");
        assert_eq!(result.symbols.len(), 1);

        let symbol = &result.symbols[0];
        assert_eq!(symbol.name, "TestEnum");
        assert_eq!(symbol.kind, SymbolKind::Enum);
        assert_eq!(symbol.language, Language::Rust);
        assert_eq!(symbol.visibility, Visibility::Private);
    }
}
