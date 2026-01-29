//! Guard test to prevent non-canonical `./var` paths from entering the codebase.
//!
//! The canonical form is `var/` (not `./var/`). This test scans source files
//! and fails if it finds `./var` patterns outside of allowed exceptions.

use std::fs;
use std::path::{Path, PathBuf};

/// Allowed patterns that may contain `./var` (backwards-compat checks, documentation).
const ALLOWED_PATTERNS: &[&str] = &[
    r#"|| trimmed == "./var""#, // backwards-compat validation
    r#"trimmed != "./var""#,    // backwards-compat validation
    r#"(NOT "./var")"#,         // documentation
    r#"(NOT `./var/`)"#,        // documentation
    r#"(not "./var")"#,         // documentation
    r#"\"./var/\""#,            // quoted in docs explaining what NOT to do
    r#"NOT "./var/")"#,         // documentation note format
];

#[test]
fn no_dotslash_var_in_sources() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();

    let crates_dir = repo_root.join("crates");

    let mut violations = Vec::new();
    scan_dir(&crates_dir, &mut violations);

    if !violations.is_empty() {
        panic!(
            "\n\nFound non-canonical './var' paths (use 'var/' instead):\n{}\n\n\
             See docs/VAR_STRUCTURE.md for the canonical path format.\n",
            violations
                .iter()
                .map(|(path, line_num, line)| format!(
                    "  {}:{}: {}",
                    path.display(),
                    line_num,
                    line.trim()
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn scan_dir(dir: &Path, violations: &mut Vec<(PathBuf, usize, String)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip target directories
        if path.ends_with("target") {
            continue;
        }

        if path.is_dir() {
            scan_dir(&path, violations);
            continue;
        }

        // Only check Rust source files
        if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
            check_file(&path, violations);
        }
    }
}

fn check_file(path: &Path, violations: &mut Vec<(PathBuf, usize, String)>) {
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for (line_num, line) in contents.lines().enumerate() {
        if !line.contains("./var") {
            continue;
        }

        // Skip if line matches an allowed pattern
        if ALLOWED_PATTERNS.iter().any(|pat| line.contains(pat)) {
            continue;
        }

        // Skip this test file itself
        if path.ends_with("var_path_canonical_guard.rs") {
            continue;
        }

        violations.push((path.to_path_buf(), line_num + 1, line.to_string()));
    }
}
