//! Directory level analysis utilities.
//!
//! The directory analyser inspects a subtree and extracts lightweight
//! statistics that are later used by the LoRA worker and router to build
//! specialized adapters.  The focus is on deterministic summarisation of
//! the symbols and coding patterns found inside a directory.

use adapteros_core::{AosError, B3Hash, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Type of symbol encountered inside a directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DirectorySymbolKind {
    Function,
    Class,
    Module,
    Variable,
}

/// Symbol extracted from the directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectorySymbol {
    pub name: String,
    pub kind: DirectorySymbolKind,
    pub file: PathBuf,
    pub language: String,
}

/// Deterministic fingerprint and metadata for a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryAnalysis {
    pub path: PathBuf,
    pub symbols: Vec<DirectorySymbol>,
    pub language_stats: BTreeMap<String, usize>,
    pub pattern_counts: BTreeMap<String, usize>,
    pub architectural_styles: BTreeSet<String>,
    pub fingerprint: B3Hash,
    pub total_files: usize,
    pub total_lines: usize,
}

/// Analyse a directory relative to the repository root.
pub fn analyze_directory(root: &Path, relative: &Path) -> Result<DirectoryAnalysis> {
    let target = root.join(relative);
    if !target.exists() {
        return Err(AosError::Io(format!(
            "directory '{}' does not exist",
            relative.display()
        )));
    }

    let mut symbols = Vec::new();
    let mut language_stats: BTreeMap<String, usize> = BTreeMap::new();
    let mut pattern_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut architectural_styles: BTreeSet<String> = BTreeSet::new();
    let mut hasher = blake3::Hasher::new();
    let mut total_files = 0usize;
    let mut total_lines = 0usize;

    for entry in WalkDir::new(&target).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        total_files += 1;

        let rel_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let language = language_from_extension(&ext);
        *language_stats.entry(language.to_string()).or_default() += 1;

        total_lines += contents.lines().count();

        hasher.update(rel_path.to_string_lossy().as_bytes());
        hasher.update(blake3::hash(contents.as_bytes()).as_bytes());

        extract_symbols(&contents, &rel_path, &language, &mut symbols);
        track_patterns(
            &contents,
            &rel_path,
            &mut pattern_counts,
            &mut architectural_styles,
        );
    }

    symbols.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));

    let fingerprint = B3Hash::from_bytes(hasher.finalize().into());

    Ok(DirectoryAnalysis {
        path: relative.to_path_buf(),
        symbols,
        language_stats,
        pattern_counts,
        architectural_styles,
        fingerprint,
        total_files,
        total_lines,
    })
}

fn language_from_extension(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "rb" => "ruby",
        "php" => "php",
        "java" => "java",
        "kt" => "kotlin",
        "cs" => "csharp",
        _ => "unknown",
    }
}

fn extract_symbols(
    contents: &str,
    file: &Path,
    language: &str,
    symbols: &mut Vec<DirectorySymbol>,
) {
    let patterns = match language {
        "rust" => vec![
            (
                Regex::new(r"fn\s+([a-zA-Z0-9_]+)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"struct\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
            (
                Regex::new(r"enum\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
        ],
        "python" => vec![
            (
                Regex::new(r"def\s+([a-zA-Z0-9_]+)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"class\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
        ],
        "javascript" | "typescript" => vec![
            (
                Regex::new(r"function\s+([a-zA-Z0-9_]+)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"class\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
            (
                Regex::new(r"const\s+([a-zA-Z0-9_]+)").unwrap(),
                DirectorySymbolKind::Variable,
            ),
        ],
        "go" => vec![
            (
                Regex::new(r"func\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"type\s+([A-Z][a-zA-Z0-9_]*)\s+struct").unwrap(),
                DirectorySymbolKind::Class,
            ),
        ],
        "ruby" => vec![
            (
                Regex::new(r"def\s+([a-zA-Z0-9_!?]+)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"class\s+([A-Z][a-zA-Z0-9_:]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
            (
                Regex::new(r"module\s+([A-Z][a-zA-Z0-9_:]*)").unwrap(),
                DirectorySymbolKind::Module,
            ),
        ],
        "php" => vec![
            (
                Regex::new(r"function\s+([a-zA-Z0-9_]+)").unwrap(),
                DirectorySymbolKind::Function,
            ),
            (
                Regex::new(r"class\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
        ],
        "java" | "kotlin" | "csharp" => vec![
            (
                Regex::new(r"class\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
            (
                Regex::new(r"interface\s+([A-Z][a-zA-Z0-9_]*)").unwrap(),
                DirectorySymbolKind::Class,
            ),
            (
                Regex::new(
                    r"(public|private|protected)?\s*(static\s+)?(fun|void)\s+([a-zA-Z0-9_]+)\s*\(",
                )
                .unwrap(),
                DirectorySymbolKind::Function,
            ),
        ],
        _ => vec![(
            Regex::new(r"fn\s+([a-zA-Z0-9_]+)").unwrap(),
            DirectorySymbolKind::Function,
        )],
    };

    for (regex, kind) in patterns {
        for caps in regex.captures_iter(contents) {
            if let Some(name) = caps.get(1) {
                symbols.push(DirectorySymbol {
                    name: name.as_str().to_string(),
                    kind: kind.clone(),
                    file: file.to_path_buf(),
                    language: language.to_string(),
                });
            }
        }
    }
}

fn track_patterns(
    contents: &str,
    file: &Path,
    pattern_counts: &mut BTreeMap<String, usize>,
    architectural_styles: &mut BTreeSet<String>,
) {
    const KEYWORDS: &[(&str, &str)] = &[
        ("async", "async_usage"),
        ("await ", "async_usage"),
        ("test_", "tests"),
        ("describe(", "tests"),
        ("#[test]", "tests"),
        ("http", "http"),
        ("router", "routing"),
        ("controller", "mvc"),
        ("service", "service_layer"),
    ];

    for (needle, key) in KEYWORDS {
        if contents.contains(needle) {
            *pattern_counts.entry((*key).to_string()).or_default() += 1;
        }
    }

    let lower_path = file.to_string_lossy().to_lowercase();
    if lower_path.contains("/controllers") {
        architectural_styles.insert("mvc-controller".into());
    }
    if lower_path.contains("/views") {
        architectural_styles.insert("mvc-views".into());
    }
    if lower_path.contains("/routers") || lower_path.contains("/routes") {
        architectural_styles.insert("routing".into());
    }
    if lower_path.contains("/migrations") {
        architectural_styles.insert("migrations".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn analyzes_rust_directory() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("lib.rs"),
            r#"
                pub mod api;
                pub fn handler() {}
                pub struct Router;
            "#,
        )
        .unwrap();

        let analysis = analyze_directory(root, Path::new("src")).unwrap();
        assert_eq!(analysis.total_files, 1);
        assert!(analysis
            .symbols
            .iter()
            .any(|s| s.name == "handler" && s.kind == DirectorySymbolKind::Function));
        assert!(analysis.fingerprint.to_hex().len() > 0);
    }

    #[test]
    fn captures_python_patterns() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let app = root.join("app");
        fs::create_dir_all(app.join("controllers")).unwrap();
        fs::write(
            app.join("controllers").join("users.py"),
            r#"
                import fastapi

                class UsersController:
                    async def list_users(self):
                        return []
            "#,
        )
        .unwrap();

        let analysis = analyze_directory(root, Path::new("app")).unwrap();
        assert!(analysis.pattern_counts.get("async_usage").is_some());
        assert!(analysis.architectural_styles.contains("mvc-controller"));
    }
}
