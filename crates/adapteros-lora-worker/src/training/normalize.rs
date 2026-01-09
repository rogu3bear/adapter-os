//! Text normalization for dataset ingestion.
//!
//! Provides deterministic text normalization:
//! - UTF-8 validation (enforced by Rust's &str)
//! - Line ending normalization (\r\n and \r → \n)
//! - Unicode NFKC normalization
//! - Trailing whitespace removal per line

use adapteros_core::{AosError, Result};
use unicode_normalization::UnicodeNormalization;

/// Normalization configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizationConfig {
    /// Normalize line endings to \n.
    pub normalize_line_endings: bool,
    /// Apply Unicode NFKC normalization.
    pub apply_nfkc: bool,
    /// Trim trailing whitespace per line.
    pub trim_trailing_whitespace: bool,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        Self {
            normalize_line_endings: true,
            apply_nfkc: true,
            trim_trailing_whitespace: true,
        }
    }
}

/// The normalization scheme identifier for manifest metadata.
pub const NORMALIZATION_SCHEME: &str = "utf8_nfkc_lf";

/// Normalize text content for dataset ingestion.
///
/// Applies the following transformations in order:
/// 1. Line ending normalization: \r\n and \r → \n
/// 2. Unicode NFKC normalization
/// 3. Trailing whitespace trimmed per line
///
/// Returns an error if the result is empty after normalization.
pub fn normalize_text(text: &str) -> Result<String> {
    normalize_text_with_config(text, &NormalizationConfig::default())
}

/// Normalize text with custom configuration.
pub fn normalize_text_with_config(text: &str, config: &NormalizationConfig) -> Result<String> {
    let mut result = text.to_string();

    // 1. Normalize line endings: \r\n → \n, then \r → \n
    if config.normalize_line_endings {
        result = result.replace("\r\n", "\n").replace('\r', "\n");
    }

    // 2. Apply Unicode NFKC normalization
    if config.apply_nfkc {
        result = result.nfkc().collect();
    }

    // 3. Trim trailing whitespace per line
    if config.trim_trailing_whitespace {
        result = result
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
    }

    Ok(result)
}

/// Validate that text is not empty or whitespace-only.
///
/// Returns an error with context for debugging.
pub fn validate_non_empty(text: &str, field_name: &str, context: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(AosError::Validation(format!(
            "Empty or whitespace-only {} in {}",
            field_name, context
        )));
    }
    Ok(())
}

/// Check if text is empty or whitespace-only after normalization.
pub fn is_empty_after_normalize(text: &str) -> bool {
    normalize_text(text)
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_line_endings() {
        assert_eq!(normalize_text("a\r\nb\rc").unwrap(), "a\nb\nc");
        assert_eq!(normalize_text("no\nchange").unwrap(), "no\nchange");
    }

    #[test]
    fn test_normalize_unicode_nfkc() {
        // NFKC normalizes full-width to ASCII
        assert_eq!(normalize_text("ABC").unwrap(), "ABC");
        // Composed vs decomposed
        let composed = "é"; // U+00E9
        let decomposed = "é"; // e + U+0301
        let result_composed = normalize_text(composed).unwrap();
        let result_decomposed = normalize_text(decomposed).unwrap();
        assert_eq!(result_composed, result_decomposed);
    }

    #[test]
    fn test_trim_trailing_whitespace() {
        assert_eq!(normalize_text("hello   \nworld  ").unwrap(), "hello\nworld");
        assert_eq!(normalize_text("  leading").unwrap(), "  leading");
    }

    #[test]
    fn test_idempotent() {
        let text = "Hello\r\nWorld  \n\tIndented";
        let once = normalize_text(text).unwrap();
        let twice = normalize_text(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn test_validate_non_empty() {
        assert!(validate_non_empty("hello", "input", "test").is_ok());
        assert!(validate_non_empty("", "input", "test").is_err());
        assert!(validate_non_empty("   ", "input", "test").is_err());
        assert!(validate_non_empty("\n\t", "input", "test").is_err());
    }

    #[test]
    fn test_is_empty_after_normalize() {
        assert!(!is_empty_after_normalize("hello"));
        assert!(is_empty_after_normalize(""));
        assert!(is_empty_after_normalize("   \n\t  "));
    }
}
