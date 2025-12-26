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
}
