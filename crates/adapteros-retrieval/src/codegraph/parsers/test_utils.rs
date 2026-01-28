//! Test utilities for multi-language parsing

use crate::types::{Language, ParseResult, Span, SymbolKind, SymbolNode};
use std::path::Path;

/// Helper function to create test symbols
pub fn create_test_symbol(
    name: &str,
    kind: SymbolKind,
    language: Language,
    file_path: &Path,
) -> SymbolNode {
    let span = Span::new(1, 1, 1, 20, 0, 20);
    let id = crate::SymbolId::new(&file_path.to_string_lossy(), &span.to_string(), name);

    SymbolNode::new(
        id,
        name.to_string(),
        kind,
        language,
        span,
        file_path.to_string_lossy().to_string(),
    )
}

/// Helper function to create test parse results
pub fn create_test_parse_result(file_path: &Path, symbols: Vec<SymbolNode>) -> ParseResult {
    ParseResult {
        file_path: file_path.to_path_buf(),
        symbols,
    }
}

/// Helper function to create multi-language test files
pub fn create_multi_language_test_files(temp_dir: &std::path::Path) -> Result<(), std::io::Error> {
    // Rust file
    std::fs::write(
        temp_dir.join("test.rs"),
        "pub fn rust_function() -> i32 { 42 }",
    )?;

    // Python file
    std::fs::write(
        temp_dir.join("test.py"),
        "def python_function():\n    return 42",
    )?;

    // TypeScript file
    std::fs::write(
        temp_dir.join("test.ts"),
        "function typescriptFunction(): number {\n    return 42;\n}",
    )?;

    // JavaScript file
    std::fs::write(
        temp_dir.join("test.js"),
        "function javascriptFunction() {\n    return 42;\n}",
    )?;

    // Go file
    std::fs::write(
        temp_dir.join("test.go"),
        "package main\n\nfunc goFunction() int {\n    return 42\n}",
    )?;

    Ok(())
}

/// Helper function to verify language-specific parsing
pub fn verify_language_parsing(
    result: &ParseResult,
    expected_language: Language,
    expected_symbol_count: usize,
) -> bool {
    if result.symbols.len() != expected_symbol_count {
        return false;
    }

    for symbol in &result.symbols {
        if symbol.language != expected_language {
            return false;
        }
    }

    true
}

/// Helper function to verify cross-language import detection
pub fn verify_cross_language_imports(result: &ParseResult, expected_import_count: usize) -> bool {
    let import_count = result
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .count();

    import_count == expected_import_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("create temp dir")
    }

    #[test]
    fn test_create_test_symbol() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        let symbol = create_test_symbol(
            "test_function",
            SymbolKind::Function,
            Language::Rust,
            &test_file,
        );

        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, Language::Rust);
    }

    #[test]
    fn test_create_test_parse_result() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, Language::Rust, &test_file),
            create_test_symbol("func2", SymbolKind::Function, Language::Rust, &test_file),
        ];

        let result = create_test_parse_result(&test_file, symbols);

        assert_eq!(result.symbols.len(), 2);
        assert_eq!(result.file_path, test_file);
    }

    #[test]
    fn test_create_multi_language_test_files() {
        let temp_dir = new_test_tempdir();

        create_multi_language_test_files(temp_dir.path()).unwrap();

        // Verify all files were created
        assert!(temp_dir.path().join("test.rs").exists());
        assert!(temp_dir.path().join("test.py").exists());
        assert!(temp_dir.path().join("test.ts").exists());
        assert!(temp_dir.path().join("test.js").exists());
        assert!(temp_dir.path().join("test.go").exists());
    }

    #[test]
    fn test_verify_language_parsing() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, Language::Rust, &test_file),
            create_test_symbol("func2", SymbolKind::Function, Language::Rust, &test_file),
        ];

        let result = create_test_parse_result(&test_file, symbols);

        assert!(verify_language_parsing(&result, Language::Rust, 2));
        assert!(!verify_language_parsing(&result, Language::Python, 2));
    }

    #[test]
    fn test_verify_cross_language_imports() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.rs");

        let symbols = vec![
            create_test_symbol("func1", SymbolKind::Function, Language::Rust, &test_file),
            create_test_symbol(
                "import_module",
                SymbolKind::Module,
                Language::Rust,
                &test_file,
            ),
            create_test_symbol(
                "import_python",
                SymbolKind::Module,
                Language::Rust,
                &test_file,
            ),
        ];

        let result = create_test_parse_result(&test_file, symbols);

        assert!(verify_cross_language_imports(&result, 2));
        assert!(!verify_cross_language_imports(&result, 1));
    }
}
