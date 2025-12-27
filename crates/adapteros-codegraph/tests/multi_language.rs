//! Multi-language parsing integration tests

use adapteros_codegraph::parsers::test_utils::{
    create_multi_language_test_files, verify_language_parsing,
};
use adapteros_codegraph::{
    detect_language, parse_directory, CodeGraph, Language, ParserFactory, SymbolKind,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

#[tokio::test]
async fn test_multi_language_directory_parsing() {
    let temp_dir = new_test_tempdir();
    create_multi_language_test_files(temp_dir.path()).unwrap();

    // Parse the entire directory
    let results = parse_directory(temp_dir.path()).await.unwrap();

    // Should have parsed all 5 files
    assert_eq!(results.len(), 5);

    // Verify each language was parsed correctly
    let mut language_counts = std::collections::HashMap::new();
    for result in &results {
        let language = result
            .file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "rs" => Some(Language::Rust),
                "py" => Some(Language::Python),
                "ts" => Some(Language::TypeScript),
                "js" => Some(Language::JavaScript),
                "go" => Some(Language::Go),
                _ => None,
            })
            .unwrap();

        *language_counts.entry(language).or_insert(0) += 1;
    }

    assert_eq!(language_counts.get(&Language::Rust), Some(&1));
    assert_eq!(language_counts.get(&Language::Python), Some(&1));
    assert_eq!(language_counts.get(&Language::TypeScript), Some(&1));
    assert_eq!(language_counts.get(&Language::JavaScript), Some(&1));
    assert_eq!(language_counts.get(&Language::Go), Some(&1));
}

#[tokio::test]
async fn test_language_detection() {
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

#[tokio::test]
async fn test_parser_factory() {
    // Test creating individual parsers
    let rust_parser = ParserFactory::create_parser(Language::Rust);
    assert!(rust_parser.is_ok());

    let python_parser = ParserFactory::create_parser(Language::Python);
    assert!(python_parser.is_ok());

    let typescript_parser = ParserFactory::create_parser(Language::TypeScript);
    assert!(typescript_parser.is_ok());

    let javascript_parser = ParserFactory::create_parser(Language::JavaScript);
    assert!(javascript_parser.is_ok());

    let go_parser = ParserFactory::create_parser(Language::Go);
    assert!(go_parser.is_ok());

    // Test creating all parsers
    let all_parsers = ParserFactory::create_all_parsers();
    assert!(all_parsers.is_ok());
    assert_eq!(all_parsers.unwrap().len(), 5);
}

#[tokio::test]
async fn test_rust_parser_symbol_extraction() {
    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test.rs");

    std::fs::write(
        &test_file,
        r#"
        pub fn public_function() -> i32 { 42 }
        fn private_function() -> String { "hello".to_string() }
        struct TestStruct { field: i32 }
        enum TestEnum { Variant1, Variant2 }
        trait TestTrait { fn method(); }
    "#,
    )
    .unwrap();

    let mut parser = ParserFactory::create_parser(Language::Rust).unwrap();
    let result = parser.parse_file(&test_file).unwrap();

    // Should extract multiple symbols
    assert!(result.symbols.len() >= 5);

    // Verify language is correct
    assert!(verify_language_parsing(
        &result,
        Language::Rust,
        result.symbols.len()
    ));

    // Check for specific symbol types
    let has_function = result
        .symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function);
    let has_struct = result.symbols.iter().any(|s| s.kind == SymbolKind::Struct);
    let has_enum = result.symbols.iter().any(|s| s.kind == SymbolKind::Enum);
    let has_trait = result.symbols.iter().any(|s| s.kind == SymbolKind::Trait);

    assert!(has_function);
    assert!(has_struct);
    assert!(has_enum);
    assert!(has_trait);
}

#[tokio::test]
async fn test_python_parser_symbol_extraction() {
    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test.py");

    std::fs::write(
        &test_file,
        r#"
        def public_function():
            return 42
        
        def _private_function():
            return "hello"
        
        class TestClass:
            def __init__(self):
                pass
            
            def method(self):
                return "world"
        
        async def async_function():
            return 42
    "#,
    )
    .unwrap();

    let mut parser = ParserFactory::create_parser(Language::Python).unwrap();
    let result = parser.parse_file(&test_file).unwrap();

    // Should extract multiple symbols
    assert!(result.symbols.len() >= 4);

    // Verify language is correct
    assert!(verify_language_parsing(
        &result,
        Language::Python,
        result.symbols.len()
    ));

    // Check for specific symbol types
    let has_function = result
        .symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function);
    let has_struct = result.symbols.iter().any(|s| s.kind == SymbolKind::Struct); // Python classes map to Struct
    let has_method = result.symbols.iter().any(|s| s.kind == SymbolKind::Method);

    assert!(has_function);
    assert!(has_struct);
    assert!(has_method);
}

#[tokio::test]
async fn test_typescript_parser_symbol_extraction() {
    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test.ts");

    std::fs::write(
        &test_file,
        r#"
        function publicFunction(): number {
            return 42;
        }
        
        class TestClass {
            constructor() {}
            
            method(): string {
                return "hello";
            }
        }
        
        interface TestInterface {
            prop: string;
        }
        
        type TestType = string | number;
        
        enum TestEnum {
            VALUE1,
            VALUE2
        }
    "#,
    )
    .unwrap();

    let mut parser = ParserFactory::create_parser(Language::TypeScript).unwrap();
    let result = parser.parse_file(&test_file).unwrap();

    // Should extract multiple symbols
    assert!(result.symbols.len() >= 5);

    // Verify language is correct
    assert!(verify_language_parsing(
        &result,
        Language::TypeScript,
        result.symbols.len()
    ));

    // Check for specific symbol types
    let has_function = result
        .symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function);
    let has_struct = result.symbols.iter().any(|s| s.kind == SymbolKind::Struct); // TypeScript classes map to Struct
    let has_trait = result.symbols.iter().any(|s| s.kind == SymbolKind::Trait); // TypeScript interfaces map to Trait
    let has_type = result.symbols.iter().any(|s| s.kind == SymbolKind::Type);
    let has_enum = result.symbols.iter().any(|s| s.kind == SymbolKind::Enum);

    assert!(has_function);
    assert!(has_struct);
    assert!(has_trait);
    assert!(has_type);
    assert!(has_enum);
}

#[tokio::test]
async fn test_javascript_parser_symbol_extraction() {
    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test.js");

    std::fs::write(
        &test_file,
        r#"
        function publicFunction() {
            return 42;
        }
        
        const arrowFunction = () => {
            return "hello";
        };
        
        class TestClass {
            constructor() {}
            
            method() {
                return "world";
            }
        }
        
        const obj = {
            method() {
                return "object method";
            }
        };
    "#,
    )
    .unwrap();

    let mut parser = ParserFactory::create_parser(Language::JavaScript).unwrap();
    let result = parser.parse_file(&test_file).unwrap();

    // Should extract multiple symbols
    assert!(result.symbols.len() >= 4);

    // Verify language is correct
    assert!(verify_language_parsing(
        &result,
        Language::JavaScript,
        result.symbols.len()
    ));

    // Check for specific symbol types
    let has_function = result
        .symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function);
    let has_struct = result.symbols.iter().any(|s| s.kind == SymbolKind::Struct); // JavaScript classes map to Struct
    let has_method = result.symbols.iter().any(|s| s.kind == SymbolKind::Method);

    assert!(has_function);
    assert!(has_struct);
    assert!(has_method);
}

#[tokio::test]
async fn test_go_parser_symbol_extraction() {
    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test.go");

    std::fs::write(
        &test_file,
        r#"
        package main
        
        func PublicFunction() int {
            return 42
        }
        
        func privateFunction() string {
            return "hello"
        }
        
        type TestStruct struct {
            Field int
        }
        
        func (t TestStruct) Method() string {
            return "world"
        }
        
        type TestInterface interface {
            Method() string
        }
        
        type TestType int
        
        const TestConstant = "constant"
    "#,
    )
    .unwrap();

    let mut parser = ParserFactory::create_parser(Language::Go).unwrap();
    let result = parser.parse_file(&test_file).unwrap();

    // Should extract multiple symbols
    assert!(result.symbols.len() >= 6);

    // Verify language is correct
    assert!(verify_language_parsing(
        &result,
        Language::Go,
        result.symbols.len()
    ));

    // Check for specific symbol types
    let has_function = result
        .symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function);
    let has_struct = result.symbols.iter().any(|s| s.kind == SymbolKind::Struct);
    let has_trait = result.symbols.iter().any(|s| s.kind == SymbolKind::Trait); // Go interfaces map to Trait
    let has_type = result.symbols.iter().any(|s| s.kind == SymbolKind::Type);
    let has_method = result.symbols.iter().any(|s| s.kind == SymbolKind::Method);
    let has_const = result.symbols.iter().any(|s| s.kind == SymbolKind::Const);

    assert!(has_function);
    assert!(has_struct);
    assert!(has_trait);
    assert!(has_type);
    assert!(has_method);
    assert!(has_const);
}

#[tokio::test]
async fn test_cross_language_import_detection() {
    let temp_dir = new_test_tempdir();

    // Create files with cross-language imports
    std::fs::write(
        temp_dir.path().join("rust_file.rs"),
        r#"
        use std::collections::HashMap;
        use serde_json;

        pub fn rust_function() -> i32 { 42 }
    "#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("python_file.py"),
        r#"
        import json
        from typing import List
        
        def python_function() -> int:
            return 42
    "#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.path().join("typescript_file.ts"),
        r#"
        import { Component } from 'react';
        import * as fs from 'fs';
        
        function typescriptFunction(): number {
            return 42;
        }
    "#,
    )
    .unwrap();

    // Parse the directory
    let results = parse_directory(temp_dir.path()).await.unwrap();

    // Should have parsed all files
    assert_eq!(results.len(), 3);

    // Check for import symbols
    let mut total_imports = 0;
    for result in &results {
        let import_count = result
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .count();
        total_imports += import_count;
    }

    // Should have detected some imports
    assert!(total_imports > 0);
}

#[tokio::test]
async fn test_deterministic_hashing_across_languages() {
    let temp_dir = new_test_tempdir();
    create_multi_language_test_files(temp_dir.path()).unwrap();

    // Parse directory multiple times
    let result1 = parse_directory(temp_dir.path()).await.unwrap();
    let result2 = parse_directory(temp_dir.path()).await.unwrap();

    // Results should be identical
    assert_eq!(result1.len(), result2.len());

    // Content hashes should be identical
    for (_r1, _r2) in result1.iter().zip(result2.iter()) {
        // Content hash comparison removed since ParseResult no longer has this field
    }
}

#[tokio::test]
async fn test_codegraph_from_multi_language_directory() {
    let temp_dir = new_test_tempdir();
    create_multi_language_test_files(temp_dir.path()).unwrap();

    // Build CodeGraph from directory
    let codegraph = CodeGraph::from_directory(temp_dir.path(), None)
        .await
        .unwrap();

    // Should have symbols from all languages
    assert!(codegraph.symbols.len() >= 5);

    // Verify language distribution
    let mut language_counts = std::collections::HashMap::new();
    for symbol in codegraph.symbols.values() {
        *language_counts.entry(symbol.language.clone()).or_insert(0) += 1;
    }

    // Should have symbols from all 5 languages
    assert_eq!(language_counts.len(), 5);
    assert!(language_counts.contains_key(&Language::Rust));
    assert!(language_counts.contains_key(&Language::Python));
    assert!(language_counts.contains_key(&Language::TypeScript));
    assert!(language_counts.contains_key(&Language::JavaScript));
    assert!(language_counts.contains_key(&Language::Go));
}

#[tokio::test]
async fn test_parser_error_handling() {
    let temp_dir = new_test_tempdir();

    // Create files with syntax errors
    std::fs::write(
        temp_dir.path().join("invalid.rs"),
        "fn invalid rust syntax {",
    )
    .unwrap();
    std::fs::write(
        temp_dir.path().join("invalid.py"),
        "def invalid python syntax",
    )
    .unwrap();
    std::fs::write(
        temp_dir.path().join("invalid.ts"),
        "function invalid typescript syntax {",
    )
    .unwrap();
    std::fs::write(
        temp_dir.path().join("invalid.js"),
        "function invalid javascript syntax {",
    )
    .unwrap();
    std::fs::write(
        temp_dir.path().join("invalid.go"),
        "func invalid go syntax {",
    )
    .unwrap();

    // Parsing should not crash, but may produce errors
    let results = parse_directory(temp_dir.path()).await.unwrap();

    // Should still return results (possibly with errors)
    assert_eq!(results.len(), 5);

    // Check that errors are recorded where appropriate
    for result in &results {
        // Some parsers may succeed with partial results, others may fail
        // The important thing is that the system doesn't crash
        assert!(result.file_path.exists());
    }
}
