//! Validation rules
//!
//! Defines the `ValidationRule` trait and common rule implementations.
//!
//! # Design
//!
//! Rules are composable and can be combined using the `ValidatorBuilder`.
//! Each rule is object-safe, allowing dynamic dispatch when needed.
//!
//! # Example
//!
//! ```rust
//! use adapteros_core::validation::{ValidationRule, NotEmpty, MaxLength};
//!
//! let not_empty = NotEmpty;
//! let max_len = MaxLength(64);
//!
//! assert!(not_empty.validate("hello").is_ok());
//! assert!(not_empty.validate("").is_err());
//! assert!(max_len.validate("short").is_ok());
//! ```

use super::error::{ValidationError, ValidationErrorCode};

/// A validation rule that can be applied to string input.
///
/// This trait is object-safe, allowing rules to be stored in collections
/// and combined dynamically.
pub trait ValidationRule: Send + Sync {
    /// Validate the input string.
    ///
    /// Returns `Ok(())` if validation passes, or a `ValidationError` on failure.
    fn validate(&self, input: &str) -> Result<(), ValidationError>;

    /// Get a description of this rule for error messages.
    fn description(&self) -> &str;
}

// =============================================================================
// Core Rules
// =============================================================================

/// Requires the input to be non-empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotEmpty;

impl ValidationRule for NotEmpty {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        if input.is_empty() {
            Err(ValidationError::with_code(
                "value",
                "Value cannot be empty",
                ValidationErrorCode::Empty,
            ))
        } else {
            Ok(())
        }
    }

    fn description(&self) -> &str {
        "must not be empty"
    }
}

/// Requires the input to be non-empty after trimming whitespace.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotBlank;

impl ValidationRule for NotBlank {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        if input.trim().is_empty() {
            Err(ValidationError::with_code(
                "value",
                "Value cannot be blank or whitespace-only",
                ValidationErrorCode::Empty,
            ))
        } else {
            Ok(())
        }
    }

    fn description(&self) -> &str {
        "must not be blank"
    }
}

/// Requires the input to have a minimum length.
#[derive(Debug, Clone, Copy)]
pub struct MinLength(pub usize);

impl ValidationRule for MinLength {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        if input.len() < self.0 {
            Err(ValidationError::too_short("value", self.0, input.len()))
        } else {
            Ok(())
        }
    }

    fn description(&self) -> &str {
        "must have minimum length"
    }
}

/// Requires the input to have a maximum length.
#[derive(Debug, Clone, Copy)]
pub struct MaxLength(pub usize);

impl ValidationRule for MaxLength {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        if input.len() > self.0 {
            Err(ValidationError::too_long("value", self.0, input.len()))
        } else {
            Ok(())
        }
    }

    fn description(&self) -> &str {
        "must have maximum length"
    }
}

/// Requires the input to only contain alphanumeric ASCII characters.
#[derive(Debug, Clone, Copy, Default)]
pub struct Alphanumeric;

impl ValidationRule for Alphanumeric {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        let invalid: Vec<char> = input
            .chars()
            .filter(|c| !c.is_ascii_alphanumeric())
            .collect();

        if invalid.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::invalid_chars("value", &invalid))
        }
    }

    fn description(&self) -> &str {
        "must contain only alphanumeric characters"
    }
}

/// Allows specific additional characters beyond alphanumeric.
#[derive(Debug, Clone)]
pub struct AllowedChars {
    /// Characters that are allowed in addition to alphanumeric
    pub allowed: Vec<char>,
    /// Whether to also allow alphanumeric by default
    pub include_alphanumeric: bool,
}

impl AllowedChars {
    /// Create a rule allowing specific chars plus alphanumeric.
    pub fn new(allowed: &str) -> Self {
        Self {
            allowed: allowed.chars().collect(),
            include_alphanumeric: true,
        }
    }

    /// Create a rule allowing only specific chars (no alphanumeric by default).
    pub fn only(allowed: &str) -> Self {
        Self {
            allowed: allowed.chars().collect(),
            include_alphanumeric: false,
        }
    }
}

impl ValidationRule for AllowedChars {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        let invalid: Vec<char> = input
            .chars()
            .filter(|c| {
                let is_allowed = self.allowed.contains(c);
                let is_alphanum = self.include_alphanumeric && c.is_ascii_alphanumeric();
                !is_allowed && !is_alphanum
            })
            .collect();

        if invalid.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::invalid_chars("value", &invalid))
        }
    }

    fn description(&self) -> &str {
        "must contain only allowed characters"
    }
}

/// Requires the input to start with an alphanumeric character.
#[derive(Debug, Clone, Copy, Default)]
pub struct StartsWithAlphanumeric;

impl ValidationRule for StartsWithAlphanumeric {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        match input.chars().next() {
            Some(c) if c.is_ascii_alphanumeric() => Ok(()),
            Some(c) => Err(ValidationError::with_code(
                "value",
                format!("Must start with alphanumeric character (got '{}')", c),
                ValidationErrorCode::InvalidStart,
            )),
            None => Ok(()), // Empty handled by NotEmpty rule
        }
    }

    fn description(&self) -> &str {
        "must start with alphanumeric character"
    }
}

/// Requires the input to end with an alphanumeric character.
#[derive(Debug, Clone, Copy, Default)]
pub struct EndsWithAlphanumeric;

impl ValidationRule for EndsWithAlphanumeric {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        match input.chars().last() {
            Some(c) if c.is_ascii_alphanumeric() => Ok(()),
            Some(c) => Err(ValidationError::with_code(
                "value",
                format!("Must end with alphanumeric character (got '{}')", c),
                ValidationErrorCode::InvalidEnd,
            )),
            None => Ok(()), // Empty handled by NotEmpty rule
        }
    }

    fn description(&self) -> &str {
        "must end with alphanumeric character"
    }
}

/// Forbids consecutive occurrences of specified characters.
#[derive(Debug, Clone)]
pub struct NoConsecutive {
    /// Patterns to forbid (e.g., "--", "__", "-_")
    pub patterns: Vec<String>,
}

impl NoConsecutive {
    /// Create a rule forbidding consecutive special characters.
    pub fn new(patterns: &[&str]) -> Self {
        Self {
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Forbid consecutive hyphens and underscores (common case).
    pub fn hyphens_and_underscores() -> Self {
        Self::new(&["--", "__", "-_", "_-"])
    }
}

impl ValidationRule for NoConsecutive {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        for pattern in &self.patterns {
            if input.contains(pattern) {
                return Err(ValidationError::with_code(
                    "value",
                    format!("Cannot contain consecutive '{}'", pattern),
                    ValidationErrorCode::ConsecutiveSpecialChars,
                ));
            }
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "must not contain consecutive special characters"
    }
}

/// Forbids specific reserved words or prefixes.
#[derive(Debug, Clone)]
pub struct NotReserved {
    /// Reserved words that are completely forbidden
    pub words: Vec<String>,
    /// Reserved prefixes that inputs cannot start with
    pub prefixes: Vec<String>,
    /// Whether to compare case-insensitively
    pub case_insensitive: bool,
}

impl NotReserved {
    /// Create a rule forbidding specific words.
    pub fn words(words: &[&str]) -> Self {
        Self {
            words: words.iter().map(|s| s.to_string()).collect(),
            prefixes: Vec::new(),
            case_insensitive: true,
        }
    }

    /// Create a rule forbidding specific prefixes.
    pub fn prefixes(prefixes: &[&str]) -> Self {
        Self {
            words: Vec::new(),
            prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
            case_insensitive: true,
        }
    }

    /// Create a rule forbidding both words and prefixes.
    pub fn new(words: &[&str], prefixes: &[&str]) -> Self {
        Self {
            words: words.iter().map(|s| s.to_string()).collect(),
            prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
            case_insensitive: true,
        }
    }

    /// Set case sensitivity.
    pub fn case_sensitive(mut self) -> Self {
        self.case_insensitive = false;
        self
    }
}

impl ValidationRule for NotReserved {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        let compare_input = if self.case_insensitive {
            input.to_lowercase()
        } else {
            input.to_string()
        };

        // Check exact matches
        for word in &self.words {
            let compare_word = if self.case_insensitive {
                word.to_lowercase()
            } else {
                word.clone()
            };

            if compare_input == compare_word {
                return Err(ValidationError::reserved_word("value", word));
            }
        }

        // Check prefixes
        for prefix in &self.prefixes {
            let compare_prefix = if self.case_insensitive {
                prefix.to_lowercase()
            } else {
                prefix.clone()
            };

            if compare_input.starts_with(&compare_prefix) {
                return Err(ValidationError::with_code(
                    "value",
                    format!("Cannot start with reserved prefix '{}'", prefix),
                    ValidationErrorCode::ReservedWord,
                ));
            }
        }

        Ok(())
    }

    fn description(&self) -> &str {
        "must not use reserved words"
    }
}

/// Requires the input to match a specific pattern (regex-like validation).
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Human-readable description of the expected pattern
    pub description: String,
    /// Validator function
    validator: fn(&str) -> bool,
}

impl Pattern {
    /// Create a pattern rule with a custom validator.
    pub fn new(description: &str, validator: fn(&str) -> bool) -> Self {
        Self {
            description: description.to_string(),
            validator,
        }
    }

    /// Hexadecimal pattern.
    pub fn hex() -> Self {
        Self::new("hexadecimal characters", |s| {
            s.chars().all(|c| c.is_ascii_hexdigit())
        })
    }

    /// Lowercase alphanumeric with underscores.
    pub fn lowercase_slug() -> Self {
        Self::new("lowercase letters, numbers, and underscores", |s| {
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    }
}

impl ValidationRule for Pattern {
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        if (self.validator)(input) {
            Ok(())
        } else {
            Err(ValidationError::pattern_mismatch("value", &self.description))
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// Custom validation rule using a closure.
pub struct Custom<F>
where
    F: Fn(&str) -> Result<(), ValidationError> + Send + Sync,
{
    validator: F,
    description: String,
}

impl<F> Custom<F>
where
    F: Fn(&str) -> Result<(), ValidationError> + Send + Sync,
{
    /// Create a custom validation rule.
    pub fn new(description: &str, validator: F) -> Self {
        Self {
            validator,
            description: description.to_string(),
        }
    }
}

impl<F> ValidationRule for Custom<F>
where
    F: Fn(&str) -> Result<(), ValidationError> + Send + Sync,
{
    fn validate(&self, input: &str) -> Result<(), ValidationError> {
        (self.validator)(input)
    }

    fn description(&self) -> &str {
        &self.description
    }
}

// Implement Debug manually for Custom
impl<F> std::fmt::Debug for Custom<F>
where
    F: Fn(&str) -> Result<(), ValidationError> + Send + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Custom")
            .field("description", &self.description)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_empty() {
        let rule = NotEmpty;
        assert!(rule.validate("hello").is_ok());
        assert!(rule.validate("").is_err());
    }

    #[test]
    fn test_not_blank() {
        let rule = NotBlank;
        assert!(rule.validate("hello").is_ok());
        assert!(rule.validate("").is_err());
        assert!(rule.validate("   ").is_err());
        assert!(rule.validate(" a ").is_ok());
    }

    #[test]
    fn test_min_length() {
        let rule = MinLength(3);
        assert!(rule.validate("abc").is_ok());
        assert!(rule.validate("abcd").is_ok());
        assert!(rule.validate("ab").is_err());
    }

    #[test]
    fn test_max_length() {
        let rule = MaxLength(5);
        assert!(rule.validate("abc").is_ok());
        assert!(rule.validate("abcde").is_ok());
        assert!(rule.validate("abcdef").is_err());
    }

    #[test]
    fn test_alphanumeric() {
        let rule = Alphanumeric;
        assert!(rule.validate("abc123").is_ok());
        assert!(rule.validate("ABC123").is_ok());
        assert!(rule.validate("abc-123").is_err());
        assert!(rule.validate("abc_123").is_err());
    }

    #[test]
    fn test_allowed_chars() {
        let rule = AllowedChars::new("-_");
        assert!(rule.validate("abc-123").is_ok());
        assert!(rule.validate("abc_123").is_ok());
        assert!(rule.validate("abc.123").is_err());
    }

    #[test]
    fn test_starts_with_alphanumeric() {
        let rule = StartsWithAlphanumeric;
        assert!(rule.validate("abc").is_ok());
        assert!(rule.validate("1abc").is_ok());
        assert!(rule.validate("-abc").is_err());
    }

    #[test]
    fn test_ends_with_alphanumeric() {
        let rule = EndsWithAlphanumeric;
        assert!(rule.validate("abc").is_ok());
        assert!(rule.validate("abc1").is_ok());
        assert!(rule.validate("abc-").is_err());
    }

    #[test]
    fn test_no_consecutive() {
        let rule = NoConsecutive::hyphens_and_underscores();
        assert!(rule.validate("a-b_c").is_ok());
        assert!(rule.validate("a--b").is_err());
        assert!(rule.validate("a__b").is_err());
        assert!(rule.validate("a-_b").is_err());
    }

    #[test]
    fn test_not_reserved_words() {
        let rule = NotReserved::words(&["admin", "root"]);
        assert!(rule.validate("user").is_ok());
        assert!(rule.validate("admin").is_err());
        assert!(rule.validate("ADMIN").is_err()); // case insensitive
        assert!(rule.validate("administrator").is_ok()); // not exact match
    }

    #[test]
    fn test_not_reserved_prefixes() {
        let rule = NotReserved::prefixes(&["system-", "internal-"]);
        assert!(rule.validate("my-adapter").is_ok());
        assert!(rule.validate("system-adapter").is_err());
        assert!(rule.validate("SYSTEM-adapter").is_err()); // case insensitive
    }

    #[test]
    fn test_pattern_hex() {
        let rule = Pattern::hex();
        assert!(rule.validate("abc123").is_ok());
        assert!(rule.validate("ABCDEF").is_ok());
        assert!(rule.validate("xyz").is_err());
    }

    #[test]
    fn test_pattern_lowercase_slug() {
        let rule = Pattern::lowercase_slug();
        assert!(rule.validate("my_slug_123").is_ok());
        assert!(rule.validate("MY_SLUG").is_err());
        assert!(rule.validate("my-slug").is_err());
    }
}
