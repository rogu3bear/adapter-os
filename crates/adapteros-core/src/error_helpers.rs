//! Error helper extension traits for ergonomic error handling
//!
//! This module provides extension traits that simplify the common pattern of
//! converting errors to `AosError` with contextual information.
//!
//! ## Usage
//!
//! Instead of verbose `.map_err()` calls:
//!
//! ```rust,ignore
//! sqlx::query("SELECT * FROM adapters WHERE id = ?")
//!     .bind(id)
//!     .fetch_one(&pool)
//!     .await
//!     .map_err(|e| AosError::Database(format!("Failed to fetch adapter: {}", e)))?;
//! ```
//!
//! Use concise helper methods:
//!
//! ```rust,ignore
//! use adapteros_core::error_helpers::DbErrorExt;
//!
//! sqlx::query("SELECT * FROM adapters WHERE id = ?")
//!     .bind(id)
//!     .fetch_one(&pool)
//!     .await
//!     .db_err("fetch adapter")?;
//! ```
//!
//! ## Available Traits
//!
//! - [`DbErrorExt`] - Database operation errors
//! - [`IoErrorExt`] - I/O operation errors with optional path context
//! - [`CryptoErrorExt`] - Cryptographic operation errors
//! - [`ValidationErrorExt`] - Validation errors with field context
//! - [`ConfigErrorExt`] - Configuration errors with setting context
//!
//! ## Examples
//!
//! ```rust
//! use adapteros_core::{Result, AosError};
//! use adapteros_core::error_helpers::{DbErrorExt, IoErrorExt, ValidationErrorExt};
//! use std::fs;
//! use std::path::Path;
//!
//! // Database errors
//! fn fetch_adapter(id: &str) -> Result<String> {
//!     // Simulating a database error
//!     let result: std::result::Result<String, String> = Err("connection timeout".to_string());
//!     result.db_err("fetch adapter")
//! }
//!
//! // I/O errors with path
//! fn read_config(path: &Path) -> Result<String> {
//!     fs::read_to_string(path)
//!         .io_err_path("read config file", path)
//! }
//!
//! // Validation errors
//! fn validate_port(port: u16) -> Result<()> {
//!     if port == 0 {
//!         return Err("port must be non-zero").validation_err("server_port");
//!     }
//!     Ok(())
//! }
//! ```

use crate::{AosError, Result};
use std::path::Path;

/// Extension trait for database operation errors
///
/// Converts any error that implements `Display` into `AosError::Database`
/// with contextual operation information.
pub trait DbErrorExt<T> {
    /// Convert error to `AosError::Database` with operation context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::DbErrorExt};
    ///
    /// fn create_adapter() -> Result<()> {
    ///     let result: std::result::Result<(), String> = Err("unique constraint violation".to_string());
    ///     result.db_err("create adapter")?;
    ///     Ok(())
    /// }
    /// ```
    fn db_err(self, operation: &str) -> Result<T>;

    /// Convert error to `AosError::Database` with dynamic context
    ///
    /// Use this when the context message needs to be computed lazily.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::DbErrorExt};
    ///
    /// fn update_adapter(id: &str) -> Result<()> {
    ///     let result: std::result::Result<(), String> = Err("row not found".to_string());
    ///     result.db_context(|| format!("update adapter {}", id))?;
    ///     Ok(())
    /// }
    /// ```
    fn db_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

/// Extension trait for I/O operation errors
///
/// Converts `std::io::Error` into `AosError::Io` with contextual information.
pub trait IoErrorExt<T> {
    /// Convert error to `AosError::Io` with operation context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::IoErrorExt};
    /// use std::fs;
    ///
    /// fn create_directory() -> Result<()> {
    ///     fs::create_dir("/tmp/test")
    ///         .io_err("create directory")?;
    ///     Ok(())
    /// }
    /// ```
    fn io_err(self, operation: &str) -> Result<T>;

    /// Convert error to `AosError::Io` with operation and path context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::IoErrorExt};
    /// use std::fs;
    /// use std::path::Path;
    ///
    /// fn read_file(path: &Path) -> Result<String> {
    ///     fs::read_to_string(path)
    ///         .io_err_path("read file", path)
    /// }
    /// ```
    fn io_err_path<P: AsRef<Path>>(self, operation: &str, path: P) -> Result<T>;
}

/// Extension trait for cryptographic operation errors
///
/// Converts any error that implements `Display` into `AosError::Crypto`
/// with contextual operation information.
pub trait CryptoErrorExt<T> {
    /// Convert error to `AosError::Crypto` with operation context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::CryptoErrorExt};
    ///
    /// fn sign_data() -> Result<Vec<u8>> {
    ///     let result: std::result::Result<Vec<u8>, String> = Err("invalid key".to_string());
    ///     result.crypto_err("sign data")
    /// }
    /// ```
    fn crypto_err(self, operation: &str) -> Result<T>;
}

/// Extension trait for validation errors
///
/// Converts any error that implements `Display` into `AosError::Validation`
/// with field context.
pub trait ValidationErrorExt<T> {
    /// Convert error to `AosError::Validation` with field context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::ValidationErrorExt};
    ///
    /// fn validate_config(port: u16) -> Result<()> {
    ///     if port == 0 {
    ///         return Err("must be non-zero").validation_err("port");
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn validation_err(self, field: &str) -> Result<T>;
}

/// Extension trait for configuration errors
///
/// Converts any error that implements `Display` into `AosError::Config`
/// with setting context.
pub trait ConfigErrorExt<T> {
    /// Convert error to `AosError::Config` with setting context
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::{Result, error_helpers::ConfigErrorExt};
    ///
    /// fn parse_port(s: &str) -> Result<u16> {
    ///     s.parse::<u16>()
    ///         .config_err("server_port")
    /// }
    /// ```
    fn config_err(self, setting: &str) -> Result<T>;
}

// ============================================================================
// Implementations
// ============================================================================

// DbErrorExt for generic Result types
impl<T, E> DbErrorExt<T> for std::result::Result<T, E>
where
    E: std::fmt::Display,
{
    fn db_err(self, operation: &str) -> Result<T> {
        self.map_err(|e| AosError::Database(format!("Failed to {}: {}", operation, e)))
    }

    fn db_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| AosError::Database(format!("{}: {}", f(), e)))
    }
}

// IoErrorExt for std::io::Result
impl<T> IoErrorExt<T> for std::io::Result<T> {
    fn io_err(self, operation: &str) -> Result<T> {
        self.map_err(|e| AosError::Io(format!("Failed to {}: {}", operation, e)))
    }

    fn io_err_path<P: AsRef<Path>>(self, operation: &str, path: P) -> Result<T> {
        self.map_err(|e| {
            AosError::Io(format!(
                "Failed to {} at '{}': {}",
                operation,
                path.as_ref().display(),
                e
            ))
        })
    }
}

// CryptoErrorExt for generic Result types
impl<T, E> CryptoErrorExt<T> for std::result::Result<T, E>
where
    E: std::fmt::Display,
{
    fn crypto_err(self, operation: &str) -> Result<T> {
        self.map_err(|e| AosError::Crypto(format!("Failed to {}: {}", operation, e)))
    }
}

// ValidationErrorExt for generic Result types
impl<T, E> ValidationErrorExt<T> for std::result::Result<T, E>
where
    E: std::fmt::Display,
{
    fn validation_err(self, field: &str) -> Result<T> {
        self.map_err(|e| AosError::Validation(format!("Invalid {}: {}", field, e)))
    }
}

// ConfigErrorExt for generic Result types
impl<T, E> ConfigErrorExt<T> for std::result::Result<T, E>
where
    E: std::fmt::Display,
{
    fn config_err(self, setting: &str) -> Result<T> {
        self.map_err(|e| AosError::Config(format!("Invalid configuration for {}: {}", setting, e)))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;

    #[test]
    fn test_db_err() {
        let result: std::result::Result<(), String> = Err("connection timeout".to_string());
        let err = result.db_err("fetch adapter").unwrap_err();

        match err {
            AosError::Database(msg) => {
                assert_eq!(msg, "Failed to fetch adapter: connection timeout");
            }
            _ => panic!("Expected Database error"),
        }
    }

    #[test]
    fn test_db_context() {
        let adapter_id = "code-review-v1";
        let result: std::result::Result<(), String> = Err("unique constraint".to_string());
        let err = result
            .db_context(|| format!("create adapter {}", adapter_id))
            .unwrap_err();

        match err {
            AosError::Database(msg) => {
                assert_eq!(msg, "create adapter code-review-v1: unique constraint");
            }
            _ => panic!("Expected Database error"),
        }
    }

    #[test]
    fn test_io_err() {
        let result: io::Result<()> = Err(io::Error::new(io::ErrorKind::NotFound, "file missing"));
        let err = result.io_err("read config file").unwrap_err();

        match err {
            AosError::Io(msg) => {
                assert!(msg.starts_with("Failed to read config file:"));
                assert!(msg.contains("file missing"));
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_io_err_path() {
        let path = Path::new("/tmp/nonexistent.toml");
        let result: io::Result<()> = Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
        let err = result.io_err_path("read config", path).unwrap_err();

        match err {
            AosError::Io(msg) => {
                assert!(msg.contains("Failed to read config"));
                assert!(msg.contains("/tmp/nonexistent.toml"));
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_crypto_err() {
        let result: std::result::Result<(), String> = Err("invalid signature".to_string());
        let err = result.crypto_err("verify signature").unwrap_err();

        match err {
            AosError::Crypto(msg) => {
                assert_eq!(msg, "Failed to verify signature: invalid signature");
            }
            _ => panic!("Expected Crypto error"),
        }
    }

    #[test]
    fn test_validation_err() {
        let result: std::result::Result<(), &str> = Err("must be non-zero");
        let err = result.validation_err("port").unwrap_err();

        match err {
            AosError::Validation(msg) => {
                assert_eq!(msg, "Invalid port: must be non-zero");
            }
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_config_err() {
        let result: std::result::Result<u16, std::num::ParseIntError> = "invalid".parse();
        let err = result.config_err("server_port").unwrap_err();

        match err {
            AosError::Config(msg) => {
                assert!(msg.starts_with("Invalid configuration for server_port:"));
            }
            _ => panic!("Expected Config error"),
        }
    }

    #[test]
    fn test_real_world_io_example() {
        // Try to read from a nonexistent path
        let path = Path::new("/tmp/adapteros_test_nonexistent_12345.txt");
        let result = fs::read_to_string(path).io_err_path("read adapter manifest", path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            AosError::Io(msg) => {
                assert!(msg.contains("Failed to read adapter manifest"));
                assert!(msg.contains(path.to_str().unwrap()));
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_chaining_with_context() {
        let result: std::result::Result<(), String> = Err("connection refused".to_string());
        let err = result
            .db_err("fetch training job")
            .map_err(|e| AosError::WithContext {
                context: "processing training request".to_string(),
                source: Box::new(e),
            })
            .unwrap_err();

        match err {
            AosError::WithContext { context, source } => {
                assert_eq!(context, "processing training request");
                match source.as_ref() {
                    AosError::Database(msg) => {
                        assert_eq!(msg, "Failed to fetch training job: connection refused");
                    }
                    _ => panic!("Expected Database error in source"),
                }
            }
            _ => panic!("Expected WithContext error"),
        }
    }

    #[test]
    fn test_multiple_error_types_in_function() {
        fn complex_operation(should_fail_db: bool, should_fail_io: bool) -> Result<String> {
            if should_fail_db {
                let db_result: std::result::Result<(), String> = Err("db error".to_string());
                db_result.db_err("load adapter")?;
            }

            if should_fail_io {
                fs::read_to_string("/nonexistent").io_err("read weights")?;
            }

            Ok("success".to_string())
        }

        // Test DB error
        let db_err = complex_operation(true, false).unwrap_err();
        assert!(matches!(db_err, AosError::Database(_)));

        // Test IO error
        let io_err = complex_operation(false, true).unwrap_err();
        assert!(matches!(io_err, AosError::Io(_)));

        // Test success
        let success = complex_operation(false, false);
        assert!(success.is_ok());
    }

    #[test]
    fn test_lazy_context_evaluation() {
        // Ensure that the context closure is only called on error
        let mut call_count = 0;

        let success_result: std::result::Result<i32, String> = Ok(42);
        let _ = success_result.db_context(|| {
            call_count += 1;
            "should not be called".to_string()
        });

        // Context should not be evaluated for Ok case
        assert_eq!(call_count, 0);

        let error_result: std::result::Result<i32, String> = Err("failure".to_string());
        let _ = error_result.db_context(|| {
            call_count += 1;
            format!("expensive context computation {}", call_count)
        });

        // Context should be evaluated exactly once for Err case
        assert_eq!(call_count, 1);
    }
}
