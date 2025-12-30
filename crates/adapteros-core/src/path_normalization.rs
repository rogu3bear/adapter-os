//! Cross-Platform Path Normalization for Determinism
//!
//! This module provides utilities for normalizing file paths to ensure
//! consistent ordering across different platforms (Windows, macOS, Linux).
//!
//! # Problem
//!
//! Different operating systems use different path separators:
//! - Windows: `C:\Users\foo\file.rs`
//! - Unix: `/home/foo/file.rs`
//!
//! When sorting paths for deterministic hash computation, the byte values
//! of these separators (`\` = 0x5C vs `/` = 0x2F) produce different orderings.
//! This module normalizes all paths to use forward slashes for consistent
//! sorting across platforms.
//!
//! # Usage
//!
//! ```rust
//! use adapteros_core::path_normalization::{normalize_path_for_sorting, compare_paths_deterministic};
//! use std::path::Path;
//!
//! // Normalize a path for deterministic sorting
//! let normalized = normalize_path_for_sorting(Path::new("src\\main.rs"));
//! assert_eq!(normalized, "src/main.rs");
//!
//! // Compare two paths deterministically
//! let cmp = compare_paths_deterministic(
//!     Path::new("src\\lib.rs"),
//!     Path::new("src/main.rs")
//! );
//! assert!(cmp == std::cmp::Ordering::Less);
//! ```
//!
//! # Normalization Rules
//!
//! 1. **Backslashes → Forward slashes**: `\` becomes `/`
//! 2. **Collapse multiple slashes**: `//` becomes `/`
//! 3. **Remove trailing slash**: `foo/` becomes `foo`
//! 4. **Unicode NFC normalization**: Ensures consistent Unicode representation
//!
//! # Version History
//!
//! This module's behavior is tracked by [`PATH_NORMALIZATION_VERSION`](crate::version::PATH_NORMALIZATION_VERSION).
//! - v1: Platform-native separators (non-deterministic)
//! - v2: Normalized forward slashes everywhere (this implementation)

use std::cmp::Ordering;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// Normalize a path to a canonical form for deterministic sorting.
///
/// This function transforms a path into a normalized string that:
/// 1. Uses forward slashes (`/`) as separators on all platforms
/// 2. Collapses multiple consecutive slashes into one
/// 3. Removes trailing slashes (except for root `/`)
/// 4. Applies Unicode NFC normalization for consistent representation
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// A normalized string representation suitable for deterministic comparison.
///
/// # Examples
///
/// ```rust
/// use adapteros_core::path_normalization::normalize_path_for_sorting;
/// use std::path::Path;
///
/// assert_eq!(normalize_path_for_sorting(Path::new("foo\\bar")), "foo/bar");
/// assert_eq!(normalize_path_for_sorting(Path::new("foo//bar")), "foo/bar");
/// assert_eq!(normalize_path_for_sorting(Path::new("foo/bar/")), "foo/bar");
/// ```
pub fn normalize_path_for_sorting(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");

    // Collapse multiple consecutive slashes into single slash
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }

    // Remove trailing slash (but preserve root "/")
    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    // Apply Unicode NFC normalization for consistent representation
    // This ensures that equivalent Unicode sequences compare as equal
    normalized.nfc().collect()
}

/// Compare two paths deterministically for sorting.
///
/// This function normalizes both paths and compares them lexicographically.
/// The comparison is consistent across all platforms.
///
/// # Arguments
///
/// * `a` - First path to compare
/// * `b` - Second path to compare
///
/// # Returns
///
/// `Ordering::Less`, `Ordering::Equal`, or `Ordering::Greater`
///
/// # Examples
///
/// ```rust
/// use adapteros_core::path_normalization::compare_paths_deterministic;
/// use std::path::Path;
/// use std::cmp::Ordering;
///
/// let result = compare_paths_deterministic(
///     Path::new("src\\lib.rs"),
///     Path::new("src/main.rs")
/// );
/// assert_eq!(result, Ordering::Less); // "lib" < "main"
/// ```
pub fn compare_paths_deterministic(a: &Path, b: &Path) -> Ordering {
    normalize_path_for_sorting(a).cmp(&normalize_path_for_sorting(b))
}

/// Normalize a path string (already converted to string).
///
/// This is a convenience function for cases where the path is already
/// a string representation.
///
/// # Arguments
///
/// * `path_str` - The path string to normalize
///
/// # Returns
///
/// A normalized string representation.
pub fn normalize_path_str(path_str: &str) -> String {
    let mut normalized = path_str.replace('\\', "/");

    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }

    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized.nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_backslashes() {
        assert_eq!(
            normalize_path_for_sorting(Path::new("foo\\bar\\baz")),
            "foo/bar/baz"
        );
        assert_eq!(
            normalize_path_for_sorting(Path::new("C:\\Users\\test\\file.rs")),
            "C:/Users/test/file.rs"
        );
    }

    #[test]
    fn test_normalize_double_slashes() {
        assert_eq!(normalize_path_for_sorting(Path::new("foo//bar")), "foo/bar");
        assert_eq!(
            normalize_path_for_sorting(Path::new("foo///bar")),
            "foo/bar"
        );
        assert_eq!(
            normalize_path_for_sorting(Path::new("//foo//bar//")),
            "/foo/bar"
        );
    }

    #[test]
    fn test_normalize_trailing_slash() {
        assert_eq!(normalize_path_for_sorting(Path::new("foo/bar/")), "foo/bar");
        assert_eq!(
            normalize_path_for_sorting(Path::new("foo/bar///")),
            "foo/bar"
        );
        // Root should be preserved
        assert_eq!(normalize_path_for_sorting(Path::new("/")), "/");
    }

    #[test]
    fn test_normalize_preserves_relative_paths() {
        assert_eq!(
            normalize_path_for_sorting(Path::new("./foo/bar")),
            "./foo/bar"
        );
        assert_eq!(
            normalize_path_for_sorting(Path::new("../foo/bar")),
            "../foo/bar"
        );
    }

    #[test]
    fn test_compare_paths_deterministic() {
        // Same content, different separators should be equal
        assert_eq!(
            compare_paths_deterministic(Path::new("foo/bar"), Path::new("foo\\bar")),
            Ordering::Equal
        );

        // Lexicographic ordering should work
        assert_eq!(
            compare_paths_deterministic(Path::new("a/file.rs"), Path::new("b/file.rs")),
            Ordering::Less
        );
        assert_eq!(
            compare_paths_deterministic(Path::new("z/file.rs"), Path::new("a/file.rs")),
            Ordering::Greater
        );
    }

    #[test]
    fn test_cross_platform_consistency() {
        // These paths should produce identical normalized forms
        let windows_paths = ["src\\main.rs", "src\\lib.rs", "tests\\test.rs"];
        let unix_paths = ["src/main.rs", "src/lib.rs", "tests/test.rs"];

        let mut windows_normalized: Vec<_> = windows_paths
            .iter()
            .map(|p| normalize_path_for_sorting(Path::new(p)))
            .collect();
        windows_normalized.sort();

        let mut unix_normalized: Vec<_> = unix_paths
            .iter()
            .map(|p| normalize_path_for_sorting(Path::new(p)))
            .collect();
        unix_normalized.sort();

        assert_eq!(
            windows_normalized, unix_normalized,
            "Normalized paths must sort identically regardless of separator style"
        );
    }

    #[test]
    fn test_edge_cases() {
        // Empty path
        assert_eq!(normalize_path_for_sorting(Path::new("")), "");

        // Just a filename
        assert_eq!(normalize_path_for_sorting(Path::new("file.rs")), "file.rs");

        // Path with spaces
        assert_eq!(
            normalize_path_for_sorting(Path::new("path with spaces/file.rs")),
            "path with spaces/file.rs"
        );

        // Mixed separators
        assert_eq!(
            normalize_path_for_sorting(Path::new("foo/bar\\baz/qux")),
            "foo/bar/baz/qux"
        );
    }

    #[test]
    fn test_unicode_normalization() {
        // NFD vs NFC: "é" can be represented as single codepoint or combining chars
        // Both should normalize to the same form
        let nfc_path = "café/file.rs"; // Single codepoint é
        let nfd_path = "cafe\u{0301}/file.rs"; // e + combining acute accent

        let nfc_normalized = normalize_path_str(nfc_path);
        let nfd_normalized = normalize_path_str(nfd_path);

        assert_eq!(
            nfc_normalized, nfd_normalized,
            "Unicode NFC normalization should make equivalent paths equal"
        );
    }
}
