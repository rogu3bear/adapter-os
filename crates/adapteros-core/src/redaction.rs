//! Structured redaction utilities for sensitive data.
//!
//! This module provides tools for redacting sensitive information from error messages,
//! logs, and API responses before they leave the system boundary.
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_core::redaction::{redact_sensitive, SecretString};
//!
//! // String-based redaction (regex patterns)
//! let error_msg = "Failed to connect to postgres://user:pass@localhost:5432/db";
//! let safe_msg = redact_sensitive(error_msg);
//! assert!(safe_msg.contains("postgres://[REDACTED]"));
//!
//! // Type-safe redaction (auto-redacts on Display)
//! let api_key = SecretString::new("sk-12345abcdef");
//! assert_eq!(format!("{}", api_key), "[REDACTED]");
//! ```
//!
//! # Environment Variable
//!
//! Set `ADAPTEROS_DISABLE_ERROR_REDACTION=1` to disable redaction for debugging.

use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::fmt;

// ============================================================================
// Environment Configuration
// ============================================================================

/// Cached env var check (read once at startup)
static REDACTION_DISABLED: Lazy<bool> = Lazy::new(|| {
    std::env::var("ADAPTEROS_DISABLE_ERROR_REDACTION")
        .map(|v| v == "1")
        .unwrap_or(false)
});

/// Check if redaction is disabled via environment variable
pub fn is_redaction_disabled() -> bool {
    *REDACTION_DISABLED
}

// ============================================================================
// Regex-based Redaction
// ============================================================================

/// Pre-compiled regex patterns for sensitive data redaction.
///
/// Pattern order matters: more specific patterns should come before general ones.
/// For example, /tmp and /run patterns must come before the general path pattern.
static REDACTION_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        // Bearer tokens
        (
            Regex::new(r"Bearer\s+[A-Za-z0-9\-_\.]+").unwrap(),
            "Bearer [REDACTED]",
        ),
        // JWT tokens (three base64 segments)
        (
            Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap(),
            "[JWT]",
        ),
        // API keys (common formats)
        (
            Regex::new(r"(?i)(api[_-]?key|apikey)[=:\s]+[A-Za-z0-9\-_]{16,}").unwrap(),
            "$1=[REDACTED]",
        ),
        // Secrets/tokens/passwords (base64-like values)
        (
            Regex::new(r"(?i)(secret|password)[=:\s]+[A-Za-z0-9+/]{16,}=*").unwrap(),
            "$1=[REDACTED]",
        ),
        // Social Security Numbers (XXX-XX-XXXX format)
        (Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(), "[SSN]"),
        // Credit card numbers (16 digits with optional spaces/dashes)
        (
            Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap(),
            "[CREDIT_CARD]",
        ),
        // PostgreSQL connection strings
        (
            Regex::new(r"postgres://[^@\s]+@[^\s]+").unwrap(),
            "postgres://[REDACTED]",
        ),
        // SQLite paths
        (
            Regex::new(r"sqlite://[^\s]+\.db").unwrap(),
            "sqlite://[REDACTED]",
        ),
        // UDS socket paths (before general paths)
        (Regex::new(r"/run/[^\s]+\.sock").unwrap(), "[SOCKET]"),
        // Temp file paths (before general paths)
        (Regex::new(r"/tmp/[^\s]+").unwrap(), "[TEMP]"),
        // Stack trace locations (file.rs:123:45)
        (Regex::new(r"\b[a-z_]+\.rs:\d+:\d+\b").unwrap(), "[SOURCE]"),
        // Windows file paths (C:\Users\... or \\server\share)
        (
            Regex::new(r"(?i)([a-z]:\\[^\s]+|\\\\[^\s]+)").unwrap(),
            "[PATH]",
        ),
        // Unix file paths with extension (must have file extension to avoid matching API routes)
        (
            Regex::new(r"(/[a-zA-Z0-9_\-\.]+){2,}\.[a-zA-Z0-9]+").unwrap(),
            "[PATH]",
        ),
        // Home directory paths
        (Regex::new(r"~(/[a-zA-Z0-9_\-\.]+)+").unwrap(), "[PATH]"),
    ]
});

/// Redact sensitive information from a string.
///
/// Applies regex-based redaction patterns to mask file paths, tokens,
/// connection strings, and other potentially sensitive data.
///
/// Set `ADAPTEROS_DISABLE_ERROR_REDACTION=1` to bypass redaction for debugging.
///
/// # Example
///
/// ```ignore
/// let msg = "Failed to open /Users/admin/secrets/config.json";
/// let safe = redact_sensitive(msg);
/// assert!(!safe.contains("/Users/"));
/// assert!(safe.contains("[PATH]"));
/// ```
pub fn redact_sensitive(input: &str) -> Cow<'_, str> {
    if *REDACTION_DISABLED {
        return Cow::Borrowed(input);
    }

    let mut result = Cow::Borrowed(input);
    for (pattern, replacement) in REDACTION_PATTERNS.iter() {
        if pattern.is_match(&result) {
            result = Cow::Owned(pattern.replace_all(&result, *replacement).into_owned());
        }
    }

    result
}

// ============================================================================
// Type-safe Redaction
// ============================================================================

/// A string type that automatically redacts its contents when displayed.
///
/// Use this for fields that should never appear in logs or error messages,
/// such as API keys, passwords, or other credentials.
///
/// # Example
///
/// ```ignore
/// use adapteros_core::redaction::SecretString;
///
/// #[derive(Debug)]
/// struct DbConfig {
///     host: String,
///     password: SecretString,
/// }
///
/// let config = DbConfig {
///     host: "localhost".into(),
///     password: SecretString::new("super_secret"),
/// };
///
/// // Debug output shows [REDACTED] for password
/// println!("{:?}", config);
/// ```
#[derive(Clone, Default)]
pub struct SecretString(String);

impl SecretString {
    /// Create a new SecretString
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Get the inner value (use with caution!)
    ///
    /// This exposes the raw secret value. Only use when you actually
    /// need the secret (e.g., to authenticate with a service).
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Check if the secret is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SecretString").field(&"[REDACTED]").finish()
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

// ============================================================================
// Copyright Excerpt Enforcement
// ============================================================================

/// Maximum allowed length for quoted excerpts to respect copyright (in words).
/// Excerpts longer than this should be summarized rather than reproduced.
pub const MAX_EXCERPT_WORDS: usize = 25;

/// Maximum allowed length for a single quoted excerpt (in characters).
/// This provides a secondary check to ensure short word counts with very long words
/// don't bypass the protection.
pub const MAX_EXCERPT_CHARS: usize = 200;

/// Result of checking content against copyright excerpt limits.
#[derive(Debug, Clone)]
pub struct ExcerptCheckResult {
    /// Whether the content exceeds excerpt limits
    pub exceeds_limit: bool,
    /// Word count of the content
    pub word_count: usize,
    /// Character count of the content
    pub char_count: usize,
    /// Suggested truncation point (if exceeds limit)
    pub suggested_truncation: Option<usize>,
    /// Whether content appears to be quoted text
    pub is_quoted: bool,
}

/// Check if content exceeds copyright excerpt limits.
///
/// This function analyzes text content to determine if it's being reproduced
/// beyond fair use limits. Content that appears to be quoted (starts with quotes,
/// uses quotation marks, etc.) is analyzed more strictly.
///
/// # Arguments
/// * `content` - The text content to check
///
/// # Returns
/// An `ExcerptCheckResult` indicating whether the content exceeds limits
///
/// # Example
///
/// ```ignore
/// use adapteros_core::redaction::check_excerpt_limits;
///
/// let short_quote = "\"This is a short quote from a book.\"";
/// let result = check_excerpt_limits(short_quote);
/// assert!(!result.exceeds_limit);
///
/// let long_quote = "\"This is a very long quote that goes on and on..."; // 50+ words
/// let result = check_excerpt_limits(long_quote);
/// assert!(result.exceeds_limit);
/// ```
pub fn check_excerpt_limits(content: &str) -> ExcerptCheckResult {
    let trimmed = content.trim();

    // Check if content appears to be quoted text
    let is_quoted = is_quoted_content(trimmed);

    // Count words and characters
    let word_count = trimmed.split_whitespace().count();
    let char_count = trimmed.chars().count();

    // Determine if limits are exceeded
    let exceeds_word_limit = word_count > MAX_EXCERPT_WORDS;
    let exceeds_char_limit = char_count > MAX_EXCERPT_CHARS;
    let exceeds_limit = exceeds_word_limit || exceeds_char_limit;

    // Calculate suggested truncation point (word boundary near limit)
    let suggested_truncation = if exceeds_limit {
        Some(find_truncation_point(trimmed, MAX_EXCERPT_WORDS))
    } else {
        None
    };

    ExcerptCheckResult {
        exceeds_limit,
        word_count,
        char_count,
        suggested_truncation,
        is_quoted,
    }
}

/// Check if content appears to be quoted text
fn is_quoted_content(content: &str) -> bool {
    let trimmed = content.trim();

    // Direct quote indicators
    let starts_with_quote = trimmed.starts_with('"')
        || trimmed.starts_with('"')
        || trimmed.starts_with('\u{2018}')
        || trimmed.starts_with('\'');

    let ends_with_quote = trimmed.ends_with('"')
        || trimmed.ends_with('"')
        || trimmed.ends_with('\u{2019}')
        || trimmed.ends_with('\'');

    // Block quote indicators
    let has_block_quote = trimmed.starts_with("> ") || trimmed.starts_with(">");

    // Citation indicators
    let has_citation = trimmed.contains(" - ") && trimmed.len() > 50;

    starts_with_quote || ends_with_quote || has_block_quote || has_citation
}

/// Find a good truncation point near the word limit
fn find_truncation_point(content: &str, max_words: usize) -> usize {
    let mut word_count = 0;
    let mut last_boundary = 0;

    for (idx, c) in content.char_indices() {
        if c.is_whitespace() {
            word_count += 1;
            if word_count >= max_words {
                return idx;
            }
            last_boundary = idx;
        }
    }

    // If we didn't hit the limit, return the last word boundary
    last_boundary
}

/// Truncate content to respect copyright excerpt limits.
///
/// If the content exceeds limits, it will be truncated at a word boundary
/// and appended with an indicator.
///
/// # Arguments
/// * `content` - The content to potentially truncate
/// * `add_indicator` - Whether to append "[...]" to indicate truncation
///
/// # Returns
/// The (possibly truncated) content
pub fn enforce_excerpt_limit(content: &str, add_indicator: bool) -> String {
    let result = check_excerpt_limits(content);

    if !result.exceeds_limit {
        return content.to_string();
    }

    if let Some(truncation_point) = result.suggested_truncation {
        let truncated = &content[..truncation_point];
        if add_indicator {
            format!("{}[...]", truncated.trim())
        } else {
            truncated.trim().to_string()
        }
    } else {
        content.to_string()
    }
}

/// Check if content requires source attribution.
///
/// Content that appears to be a direct quote or substantial excerpt
/// should include source attribution.
///
/// # Arguments
/// * `content` - The content to check
/// * `has_attribution` - Whether the content already has source attribution
///
/// # Returns
/// `true` if attribution is required but missing
pub fn requires_attribution(content: &str, has_attribution: bool) -> bool {
    if has_attribution {
        return false;
    }

    let result = check_excerpt_limits(content);

    // Quoted content always needs attribution
    if result.is_quoted {
        return true;
    }

    // Longer content (even if under limit) may need attribution
    result.word_count > 15
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacts_file_paths() {
        let input = "Failed to open /Users/admin/secrets/config.json";
        let result = redact_sensitive(input);
        assert!(!result.contains("/Users/"), "Path should be redacted");
        assert!(
            result.contains("[PATH]"),
            "Should contain [PATH] placeholder"
        );
    }

    #[test]
    fn test_redacts_jwt_tokens() {
        let input = "Invalid token: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abcdefghijk";
        let result = redact_sensitive(input);
        assert!(!result.contains("eyJ"), "JWT should be redacted");
        assert!(result.contains("[JWT]"), "Should contain [JWT] placeholder");
    }

    #[test]
    fn test_redacts_bearer_tokens() {
        let input = "Authorization: Bearer sk-12345abcdefghijklmnop";
        let result = redact_sensitive(input);
        assert!(
            !result.contains("sk-12345"),
            "Bearer token should be redacted"
        );
        assert!(
            result.contains("Bearer [REDACTED]"),
            "Should contain Bearer [REDACTED]"
        );
    }

    #[test]
    fn test_redacts_postgres_connection() {
        let input = "Connection failed: postgres://user:password@localhost:5432/db";
        let result = redact_sensitive(input);
        assert!(!result.contains("password"), "Password should be redacted");
        assert!(
            result.contains("postgres://[REDACTED]"),
            "Should contain postgres://[REDACTED]"
        );
    }

    #[test]
    fn test_preserves_api_routes() {
        // API routes should NOT be redacted (no file extension)
        let input = "Not found: /api/v1/users";
        let result = redact_sensitive(input);
        assert!(
            result.contains("/api/v1/users"),
            "API route should be preserved"
        );
    }

    #[test]
    fn test_secret_string_display() {
        let secret = SecretString::new("super_secret_password");
        assert_eq!(format!("{}", secret), "[REDACTED]");
    }

    #[test]
    fn test_secret_string_debug() {
        let secret = SecretString::new("super_secret_password");
        let debug = format!("{:?}", secret);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super_secret"));
    }

    #[test]
    fn test_secret_string_expose() {
        let secret = SecretString::new("my_password");
        assert_eq!(secret.expose(), "my_password");
    }

    #[test]
    fn test_redaction_returns_borrowed_when_no_match() {
        // When nothing matches, should return Cow::Borrowed (no allocation)
        let input = "Simple error message";
        let result = redact_sensitive(input);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "Should return borrowed when no redaction needed"
        );
    }

    #[test]
    fn test_redacts_ssn() {
        let input = "User SSN: 123-45-6789 is on file";
        let result = redact_sensitive(input);
        assert!(!result.contains("123-45-6789"), "SSN should be redacted");
        assert!(result.contains("[SSN]"), "Should contain [SSN] placeholder");
    }

    #[test]
    fn test_redacts_credit_card() {
        let input = "Card number: 4111-1111-1111-1111 on file";
        let result = redact_sensitive(input);
        assert!(
            !result.contains("4111-1111-1111-1111"),
            "Credit card should be redacted"
        );
        assert!(
            result.contains("[CREDIT_CARD]"),
            "Should contain [CREDIT_CARD] placeholder"
        );
    }

    #[test]
    fn test_redacts_credit_card_no_dashes() {
        let input = "Card: 4111111111111111";
        let result = redact_sensitive(input);
        assert!(
            !result.contains("4111111111111111"),
            "Credit card without dashes should be redacted"
        );
    }

    // ========================================================================
    // Copyright Excerpt Tests
    // ========================================================================

    #[test]
    fn test_check_excerpt_limits_short_content() {
        let short = "This is a short excerpt.";
        let result = check_excerpt_limits(short);
        assert!(!result.exceeds_limit);
        assert_eq!(result.word_count, 5);
    }

    #[test]
    fn test_check_excerpt_limits_long_content() {
        // Create content with more than MAX_EXCERPT_WORDS
        let long = "word ".repeat(30);
        let result = check_excerpt_limits(&long);
        assert!(result.exceeds_limit);
        assert!(result.word_count > MAX_EXCERPT_WORDS);
        assert!(result.suggested_truncation.is_some());
    }

    #[test]
    fn test_check_excerpt_limits_detects_quotes() {
        let quoted = "\"This is a quoted text from a book.\"";
        let result = check_excerpt_limits(quoted);
        assert!(result.is_quoted);

        let not_quoted = "This is just regular text without quotes.";
        let result = check_excerpt_limits(not_quoted);
        assert!(!result.is_quoted);
    }

    #[test]
    fn test_check_excerpt_limits_block_quote() {
        let block_quote = "> This is a block quote from an article.";
        let result = check_excerpt_limits(block_quote);
        assert!(result.is_quoted);
    }

    #[test]
    fn test_enforce_excerpt_limit_truncates() {
        let long = "word ".repeat(30);
        let truncated = enforce_excerpt_limit(&long, true);
        assert!(truncated.len() < long.len());
        assert!(truncated.ends_with("[...]"));
    }

    #[test]
    fn test_enforce_excerpt_limit_preserves_short() {
        let short = "This is short.";
        let result = enforce_excerpt_limit(short, true);
        assert_eq!(result, short);
        assert!(!result.contains("[...]"));
    }

    #[test]
    fn test_requires_attribution_quoted() {
        let quoted = "\"This is a quote from someone.\"";
        assert!(requires_attribution(quoted, false));
        assert!(!requires_attribution(quoted, true)); // Already has attribution
    }

    #[test]
    fn test_requires_attribution_long() {
        let long_text = "This is a longer piece of text that spans multiple words and should probably have some source attribution even though it is not in quotes.";
        assert!(requires_attribution(long_text, false));
    }

    #[test]
    fn test_requires_attribution_short() {
        let short = "Just a few words.";
        assert!(!requires_attribution(short, false));
    }
}
