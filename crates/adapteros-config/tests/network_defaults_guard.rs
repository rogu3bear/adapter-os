//! Guard test to prevent hardcoded network values from entering the codebase.
//!
//! Network defaults should be centralized in `adapteros_core::defaults`:
//! - `DEFAULT_SERVER_PORT` (8080)
//! - `DEFAULT_UI_PORT` (3200)
//! - `DEFAULT_SERVER_HOST` ("127.0.0.1")
//! - `DEFAULT_SERVER_URL` -> "http://127.0.0.1:8080"
//! - `DEFAULT_API_URL` -> "http://127.0.0.1:8080/api"
//!
//! This test scans source files and fails if it finds hardcoded network values
//! outside of allowed exceptions (tests, documentation, the defaults module itself).

use std::fs;
use std::path::{Path, PathBuf};

/// Patterns that indicate hardcoded network values.
const HARDCODED_PATTERNS: &[&str] = &[
    r#""localhost:8080""#,
    r#""localhost:3200""#,
    r#""localhost:9011""#,
    r#""127.0.0.1:8080""#,
    r#""127.0.0.1:3200""#,
    r#""127.0.0.1:9011""#,
    r#""http://localhost:8080""#,
    r#""http://127.0.0.1:8080""#,
    r#""http://localhost:8080/api""#,
    r#""http://127.0.0.1:8080/api""#,
    r#""http://localhost:3200""#,
    r#""http://127.0.0.1:3200""#,
];

/// Files that are allowed to contain hardcoded values (the source of truth).
const ALLOWED_FILES: &[&str] = &[
    "defaults.rs",                   // The canonical source of defaults (adapteros-core and adapteros-api-types)
    "network_defaults_guard.rs",     // This test file
];

/// Patterns within lines that indicate allowed usage (backwards-compat, docs, tests).
const ALLOWED_LINE_PATTERNS: &[&str] = &[
    // Clap default_value attributes (Rust requires string literals in attributes)
    // These are acceptable because clap doesn't support cross-crate const expressions
    "default_value =",
    "default_value=",
    // Documentation and help text (visible to users)
    "after_help",
    "/// ",
    "//! ",
    "// Example",
    "// Run with",
    "Examples:",
    "println!",
    "eprintln!",
    "output.kv(",  // CLI output formatting
    // Test attributes (tests are allowed to use literals for clarity)
    "#[test]",
    "#[ignore",
    "assert",
    // Validation code that checks URL format
    "validate_url",
    "validate_value",
    // Error message templates (user-facing)
    "curl ",
    "\\x1b[",  // ANSI escape sequences in error messages
    // OpenAPI/codegen specification (static JSON)
    r#""url":"#,
];

/// Directories to skip entirely (test directories, target, etc.).
const SKIP_DIRS: &[&str] = &["target", "tests", "benches", "examples"];

#[test]
fn no_hardcoded_network_values_in_sources() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();

    let crates_dir = repo_root.join("crates");

    let mut violations = Vec::new();
    scan_dir(&crates_dir, &mut violations);

    // Also scan tools and xtask
    for extra_dir in ["tools", "xtask"] {
        let dir = repo_root.join(extra_dir);
        if dir.exists() {
            scan_dir(&dir, &mut violations);
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\n\
            Found hardcoded network values (use adapteros_core::defaults instead):\n\n\
            {}\n\n\
            Fix: Import and use these from adapteros_core::defaults:\n\
            - DEFAULT_SERVER_PORT (8080)\n\
            - DEFAULT_UI_PORT (3200)\n\
            - DEFAULT_SERVER_HOST (\"127.0.0.1\")\n\
            - default_server_url() -> \"http://127.0.0.1:8080\"\n\
            - default_api_url() -> \"http://127.0.0.1:8080/api\"\n\
            - default_ui_url() -> \"http://127.0.0.1:3200\"\n",
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

        // Skip excluded directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if SKIP_DIRS.contains(&name) {
                continue;
            }
        }

        if path.is_dir() {
            scan_dir(&path, violations);
            continue;
        }

        // Only check Rust source files in src/ directories
        let is_rs = path.extension().map(|ext| ext == "rs").unwrap_or(false);
        let in_src = path
            .components()
            .any(|c| c.as_os_str() == "src");

        if is_rs && in_src {
            check_file(&path, violations);
        }
    }
}

fn check_file(path: &Path, violations: &mut Vec<(PathBuf, usize, String)>) {
    // Skip allowed files
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if ALLOWED_FILES.contains(&name) {
            return;
        }
    }

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut in_test_module = false;
    let mut in_test_function = false;
    let mut brace_depth: usize = 0;
    let mut test_function_brace_depth: usize = 0;

    for (line_num, line) in contents.lines().enumerate() {
        let trimmed = line.trim_start();

        // Track #[cfg(test)] modules
        if trimmed.starts_with("#[cfg(test)]") {
            in_test_module = true;
        }

        // Track #[test] functions
        if trimmed.starts_with("#[test]") || trimmed.starts_with("#[ignore") {
            in_test_function = true;
            test_function_brace_depth = brace_depth;
        }

        // Track brace depth for test function scope
        let open_braces = line.matches('{').count();
        let close_braces = line.matches('}').count();
        brace_depth = brace_depth.saturating_add(open_braces).saturating_sub(close_braces);

        // Exit test function when we close its scope
        if in_test_function && brace_depth <= test_function_brace_depth && close_braces > 0 {
            in_test_function = false;
        }

        // Skip test code entirely
        if in_test_module || in_test_function {
            continue;
        }

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        // Skip lines with allowed patterns
        if ALLOWED_LINE_PATTERNS.iter().any(|pat| line.contains(pat)) {
            continue;
        }

        // Check for hardcoded patterns
        for pattern in HARDCODED_PATTERNS {
            if line.contains(pattern) {
                violations.push((path.to_path_buf(), line_num + 1, line.to_string()));
                break; // Only report each line once
            }
        }
    }
}
