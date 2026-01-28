//! Guard test to prevent hardcoded `PathBuf::from("var/...")`  patterns.
//!
//! All var/ paths must use `rebase_var_path()` or `resolve_var_dir().join()`
//! to correctly honor AOS_VAR_DIR when set. This test scans source files
//! and fails if it finds hardcoded `PathBuf::from("var/` patterns outside
//! of test code and allowed exceptions.

use std::fs;
use std::path::{Path, PathBuf};

/// Patterns that are allowed to contain `PathBuf::from("var/` for backwards
/// compatibility or documentation purposes.
const ALLOWED_PATTERNS: &[&str] = &[
    // This guard test file itself
    "var_rebase_guard.rs",
    // Test files are allowed to use hardcoded paths
    "#[test]",
    "#[cfg(test)]",
    "mod tests",
    // Documentation examples
    "//!",
    "///",
    // Explicitly allowed patterns (e.g., default fallbacks that get rebased)
    "unwrap_or_else(|_| PathBuf::from(\"var",
    // The path_utils module that implements rebasing
    "path_utils.rs",
    // Defaults module that defines constants
    "defaults.rs",
];

/// Files/directories to skip entirely
const SKIP_PATHS: &[&str] = &[
    "target",
    "var_rebase_guard.rs",
    "var_path_canonical_guard.rs",
];

#[test]
fn no_hardcoded_var_pathbuf_in_sources() {
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
            "\n\n\
            Found hardcoded `PathBuf::from(\"var/...)` patterns that ignore AOS_VAR_DIR:\n\n{}\n\n\
            Fix: Use `adapteros_core::rebase_var_path(\"var/...\")` or \n\
                 `adapteros_core::resolve_var_dir().join(\"subdir\")` instead.\n\n\
            This ensures paths are correctly rebased when AOS_VAR_DIR is set.\n\
            See docs/VAR_STRUCTURE.md for details.\n",
            violations
                .iter()
                .map(|(path, line_num, line)| format!(
                    "  {}:{}: {}",
                    path.strip_prefix(&repo_root).unwrap_or(path).display(),
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

        // Skip specified paths
        if SKIP_PATHS.iter().any(|skip| {
            path.file_name()
                .map(|n| n.to_string_lossy().contains(skip))
                .unwrap_or(false)
        }) {
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

    // Check if entire file is a test module
    let is_test_file = path
        .file_name()
        .map(|n| n.to_string_lossy().ends_with("_test.rs"))
        .unwrap_or(false)
        || path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n == "tests")
            .unwrap_or(false);

    if is_test_file {
        return;
    }

    // Track if we're inside a test module
    let mut in_test_block = false;
    let mut brace_depth = 0;

    for (line_num, line) in contents.lines().enumerate() {
        // Track test module/function boundaries
        if line.contains("#[test]") || line.contains("#[cfg(test)]") || line.contains("mod tests") {
            in_test_block = true;
        }

        // Simple brace tracking for test blocks
        if in_test_block {
            brace_depth += line.matches('{').count();
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());
            if brace_depth == 0 && line.contains('}') {
                in_test_block = false;
            }
        }

        // Skip if in test block
        if in_test_block {
            continue;
        }

        // Look for the problematic pattern: PathBuf::from("var/
        if !line.contains("PathBuf::from(\"var/") {
            continue;
        }

        // Skip if line matches any allowed pattern
        if ALLOWED_PATTERNS.iter().any(|pat| line.contains(pat)) {
            continue;
        }

        // Skip if this file is in an allowed path
        if SKIP_PATHS.iter().any(|skip| {
            path.to_string_lossy().contains(skip)
        }) {
            continue;
        }

        violations.push((path.to_path_buf(), line_num + 1, line.to_string()));
    }
}
