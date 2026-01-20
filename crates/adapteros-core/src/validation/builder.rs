//! Validator builder with fluent API
//!
//! Provides a composable way to build validators from multiple rules.
//!
//! # Example
//!
//! ```rust
//! use adapteros_core::validation::{ValidatorBuilder, Validator};
//!
//! let validator = ValidatorBuilder::new("adapter_id")
//!     .not_empty()
//!     .with_chars("-_")  // Alphanumeric plus hyphens and underscores
//!     .length(1, 64)
//!     .starts_with_alphanumeric()
//!     .ends_with_alphanumeric()
//!     .no_consecutive(&["--", "__"])
//!     .not_reserved_prefixes(&["system-", "internal-"])
//!     .build();
//!
//! assert!(validator.validate("my-adapter").is_ok());
//! assert!(validator.validate("").is_err());
//! ```

use super::error::ValidationError;
use super::rules::{
    AllowedChars, EndsWithAlphanumeric, MaxLength, MinLength, NoConsecutive, NotBlank, NotEmpty,
    NotReserved, Pattern, StartsWithAlphanumeric, ValidationRule,
};
use std::sync::Arc;

/// Builder for creating validators with a fluent API.
///
/// # Design
///
/// The builder accumulates rules that are checked in order. The first
/// failing rule produces the error. Rules are stored as trait objects
/// for flexibility.
///
/// # Thread Safety
///
/// Both `ValidatorBuilder` and `Validator` are `Send + Sync`, allowing
/// them to be shared across threads safely.
#[derive(Default)]
pub struct ValidatorBuilder {
    /// Name of the field being validated (used in error messages)
    field_name: String,
    /// Accumulated validation rules
    rules: Vec<Box<dyn ValidationRule>>,
}

impl ValidatorBuilder {
    /// Create a new validator builder.
    ///
    /// # Arguments
    ///
    /// * `field_name` - Name of the field being validated, used in error messages.
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
            rules: Vec::new(),
        }
    }

    /// Require the input to be non-empty.
    pub fn not_empty(mut self) -> Self {
        self.rules.push(Box::new(NotEmpty));
        self
    }

    /// Require the input to be non-blank (not empty or whitespace-only).
    pub fn not_blank(mut self) -> Self {
        self.rules.push(Box::new(NotBlank));
        self
    }

    /// Require only alphanumeric ASCII characters.
    ///
    /// Note: This is strict alphanumeric only. Use `with_chars()` to allow
    /// additional characters like hyphens or underscores.
    pub fn alphanumeric(mut self) -> Self {
        self.rules.push(Box::new(super::rules::Alphanumeric));
        self
    }

    /// Allow specific additional characters beyond alphanumeric.
    ///
    /// # Arguments
    ///
    /// * `allowed` - String of additional allowed characters (e.g., "-_").
    ///
    /// # Example
    ///
    /// ```rust
    /// use adapteros_core::validation::ValidatorBuilder;
    ///
    /// let validator = ValidatorBuilder::new("id")
    ///     .with_chars("-_./")  // Allow hyphens, underscores, dots, slashes
    ///     .build();
    /// ```
    pub fn with_chars(mut self, allowed: &str) -> Self {
        self.rules.push(Box::new(AllowedChars::new(allowed)));
        self
    }

    /// Allow only specific characters (no alphanumeric by default).
    ///
    /// Use this when you want very precise control over allowed characters.
    pub fn only_chars(mut self, allowed: &str) -> Self {
        self.rules.push(Box::new(AllowedChars::only(allowed)));
        self
    }

    /// Set minimum length constraint.
    pub fn min_length(mut self, min: usize) -> Self {
        self.rules.push(Box::new(MinLength(min)));
        self
    }

    /// Set maximum length constraint.
    pub fn max_length(mut self, max: usize) -> Self {
        self.rules.push(Box::new(MaxLength(max)));
        self
    }

    /// Set both minimum and maximum length constraints.
    ///
    /// This is a convenience method combining `min_length` and `max_length`.
    pub fn length(self, min: usize, max: usize) -> Self {
        self.min_length(min).max_length(max)
    }

    /// Require the input to start with an alphanumeric character.
    pub fn starts_with_alphanumeric(mut self) -> Self {
        self.rules.push(Box::new(StartsWithAlphanumeric));
        self
    }

    /// Require the input to end with an alphanumeric character.
    pub fn ends_with_alphanumeric(mut self) -> Self {
        self.rules.push(Box::new(EndsWithAlphanumeric));
        self
    }

    /// Forbid consecutive occurrences of specified patterns.
    ///
    /// # Arguments
    ///
    /// * `patterns` - Patterns to forbid (e.g., `&["--", "__"]`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use adapteros_core::validation::ValidatorBuilder;
    ///
    /// let validator = ValidatorBuilder::new("slug")
    ///     .no_consecutive(&["--", "__", "-_", "_-"])
    ///     .build();
    /// ```
    pub fn no_consecutive(mut self, patterns: &[&str]) -> Self {
        self.rules.push(Box::new(NoConsecutive::new(patterns)));
        self
    }

    /// Forbid consecutive hyphens and underscores (common case).
    pub fn no_consecutive_separators(self) -> Self {
        self.no_consecutive(&["--", "__", "-_", "_-"])
    }

    /// Forbid specific reserved words.
    ///
    /// Comparison is case-insensitive by default.
    pub fn not_reserved_words(mut self, words: &[&str]) -> Self {
        self.rules.push(Box::new(NotReserved::words(words)));
        self
    }

    /// Forbid specific reserved prefixes.
    ///
    /// Comparison is case-insensitive by default.
    pub fn not_reserved_prefixes(mut self, prefixes: &[&str]) -> Self {
        self.rules.push(Box::new(NotReserved::prefixes(prefixes)));
        self
    }

    /// Forbid both reserved words and prefixes.
    pub fn not_reserved(mut self, words: &[&str], prefixes: &[&str]) -> Self {
        self.rules.push(Box::new(NotReserved::new(words, prefixes)));
        self
    }

    /// Require the input to match a hexadecimal pattern.
    pub fn hexadecimal(mut self) -> Self {
        self.rules.push(Box::new(Pattern::hex()));
        self
    }

    /// Require the input to match a lowercase slug pattern.
    ///
    /// Allows: lowercase letters, numbers, underscores.
    pub fn lowercase_slug(mut self) -> Self {
        self.rules.push(Box::new(Pattern::lowercase_slug()));
        self
    }

    /// Add a custom validation rule using a function.
    ///
    /// # Arguments
    ///
    /// * `description` - Human-readable description for error messages.
    /// * `validator` - Function that returns `true` if valid, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use adapteros_core::validation::ValidatorBuilder;
    ///
    /// let validator = ValidatorBuilder::new("version")
    ///     .custom_fn("semantic version", |s| {
    ///         s.split('.').count() == 3
    ///     })
    ///     .build();
    /// ```
    pub fn custom_fn(mut self, description: &str, validator: fn(&str) -> bool) -> Self {
        self.rules
            .push(Box::new(Pattern::new(description, validator)));
        self
    }

    /// Add a custom validation rule with full error control.
    ///
    /// This allows you to provide custom error messages and codes.
    pub fn custom<F>(mut self, description: &str, validator: F) -> Self
    where
        F: Fn(&str) -> Result<(), ValidationError> + Send + Sync + 'static,
    {
        self.rules
            .push(Box::new(super::rules::Custom::new(description, validator)));
        self
    }

    /// Add an existing rule to the builder.
    pub fn rule<R: ValidationRule + 'static>(mut self, rule: R) -> Self {
        self.rules.push(Box::new(rule));
        self
    }

    /// Build the validator.
    ///
    /// The returned `Validator` is thread-safe and can be shared.
    pub fn build(self) -> Validator {
        Validator {
            field_name: self.field_name,
            rules: self.rules.into_iter().map(Arc::from).collect(),
        }
    }
}

impl std::fmt::Debug for ValidatorBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatorBuilder")
            .field("field_name", &self.field_name)
            .field("rules_count", &self.rules.len())
            .finish()
    }
}

/// A composed validator built from multiple rules.
///
/// This is the final product of `ValidatorBuilder`. It is `Clone`, `Send`,
/// and `Sync`, making it suitable for use in concurrent contexts.
#[derive(Clone)]
pub struct Validator {
    /// Name of the field being validated
    field_name: String,
    /// Composed validation rules
    rules: Vec<Arc<dyn ValidationRule>>,
}

impl Validator {
    /// Validate the input string.
    ///
    /// Returns `Ok(())` if all rules pass, or the first `ValidationError`.
    pub fn validate(&self, input: &str) -> Result<(), ValidationError> {
        for rule in &self.rules {
            if let Err(mut err) = rule.validate(input) {
                // Replace generic field name with actual field name
                err.field = self.field_name.clone();
                return Err(err);
            }
        }
        Ok(())
    }

    /// Validate and collect all errors (not just the first).
    ///
    /// This is useful for form validation where you want to show all errors at once.
    pub fn validate_all(&self, input: &str) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for rule in &self.rules {
            if let Err(mut err) = rule.validate(input) {
                err.field = self.field_name.clone();
                errors.push(err);
            }
        }
        errors
    }

    /// Check if the input is valid without returning error details.
    pub fn is_valid(&self, input: &str) -> bool {
        self.validate(input).is_ok()
    }

    /// Get the field name this validator is for.
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Get the number of rules in this validator.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl std::fmt::Debug for Validator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Validator")
            .field("field_name", &self.field_name)
            .field("rules_count", &self.rules.len())
            .finish()
    }
}

// Implement Default for convenience
impl Default for Validator {
    fn default() -> Self {
        Self {
            field_name: "value".to_string(),
            rules: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let validator = ValidatorBuilder::new("test")
            .not_empty()
            .max_length(10)
            .build();

        assert!(validator.validate("hello").is_ok());
        assert!(validator.validate("").is_err());
        assert!(validator.validate("12345678901").is_err());
    }

    #[test]
    fn test_builder_adapter_id_like() {
        let validator = ValidatorBuilder::new("adapter_id")
            .not_empty()
            .with_chars("-_")
            .length(1, 64)
            .starts_with_alphanumeric()
            .ends_with_alphanumeric()
            .no_consecutive_separators()
            .not_reserved_prefixes(&["system-", "internal-"])
            .build();

        assert!(validator.validate("my-adapter").is_ok());
        assert!(validator.validate("adapter_123").is_ok());
        assert!(validator.validate("a").is_ok());
        assert!(validator.validate("").is_err());
        assert!(validator.validate("-start").is_err());
        assert!(validator.validate("end-").is_err());
        assert!(validator.validate("double--hyphen").is_err());
        assert!(validator.validate("system-adapter").is_err());
    }

    #[test]
    fn test_validator_field_name_in_error() {
        let validator = ValidatorBuilder::new("my_field").not_empty().build();

        let err = validator.validate("").unwrap_err();
        assert_eq!(err.field, "my_field");
    }

    #[test]
    fn test_validate_all() {
        let validator = ValidatorBuilder::new("test")
            .min_length(5)
            .max_length(3)
            .build();

        // Input "abcd" fails both rules:
        // - min_length(5): 4 < 5 (fail)
        // - max_length(3): 4 > 3 (fail)
        let errors = validator.validate_all("abcd");
        assert_eq!(errors.len(), 2);

        // Input "ab" only fails min_length (2 < 5), passes max_length (2 <= 3)
        let errors = validator.validate_all("ab");
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_is_valid() {
        let validator = ValidatorBuilder::new("test").not_empty().build();

        assert!(validator.is_valid("hello"));
        assert!(!validator.is_valid(""));
    }

    #[test]
    fn test_custom_fn() {
        let validator = ValidatorBuilder::new("version")
            .custom_fn("must have 3 parts", |s| s.split('.').count() == 3)
            .build();

        assert!(validator.validate("1.2.3").is_ok());
        assert!(validator.validate("1.2").is_err());
    }

    #[test]
    fn test_validator_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Validator>();
        assert_send_sync::<ValidatorBuilder>();
    }

    #[test]
    fn test_validator_clone() {
        let validator = ValidatorBuilder::new("test")
            .not_empty()
            .max_length(10)
            .build();

        let cloned = validator.clone();
        assert_eq!(validator.field_name(), cloned.field_name());
        assert_eq!(validator.rule_count(), cloned.rule_count());
    }
}
