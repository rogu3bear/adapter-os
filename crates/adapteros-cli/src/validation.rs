//! CLI input validation utilities
//!
//! Provides validation helpers that use the structured error types
//! from adapteros_core for consistent error handling.

use adapteros_core::{AosError, Result};
use std::path::Path;

/// Deprecated CLI flags and their replacements
const DEPRECATED_FLAGS: &[(&str, &str, &str)] = &[
    ("verbose", "--log-level debug", "2.0.0"),
    ("quiet", "--log-level error", "2.0.0"),
];

/// Validate CLI flags for deprecated options
///
/// Returns an error if a deprecated flag is used and should be rejected.
/// In warning mode (default), logs a deprecation warning but allows continuation.
pub fn validate_deprecated_flags(args: &[String], strict: bool) -> Result<()> {
    for arg in args {
        let flag_name = arg.strip_prefix("--").unwrap_or(arg);

        for (deprecated, replacement, removal_version) in DEPRECATED_FLAGS {
            if flag_name == *deprecated {
                if strict {
                    return Err(AosError::DeprecatedFlag {
                        flag: deprecated.to_string(),
                        replacement: replacement.to_string(),
                        removal_version: removal_version.to_string(),
                    });
                } else {
                    tracing::warn!(
                        flag = %deprecated,
                        replacement = %replacement,
                        removal_version = %removal_version,
                        "Deprecated flag used - please migrate before removal"
                    );
                }
            }
        }
    }
    Ok(())
}

/// Validate that input is valid UTF-8
///
/// Returns an error with the byte offset of the first invalid character.
pub fn validate_utf8_input<'a>(input: &'a [u8], context: &str) -> Result<&'a str> {
    match std::str::from_utf8(input) {
        Ok(s) => Ok(s),
        Err(e) => Err(AosError::InvalidInputEncoding {
            offset: e.valid_up_to(),
            context: context.to_string(),
            suggested_flag: Some("--binary".to_string()),
        }),
    }
}

/// Validate that a path is writable
///
/// Checks if the parent directory exists and is writable.
pub fn validate_writable_path(path: &Path, operation: &str) -> Result<()> {
    // Check if parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            return Err(AosError::CliWritePermissionDenied {
                path: path.display().to_string(),
                reason: format!("Parent directory '{}' does not exist", parent.display()),
                operation: operation.to_string(),
            });
        }

        // Check if parent is writable (best effort check)
        if parent
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
        {
            return Err(AosError::CliWritePermissionDenied {
                path: path.display().to_string(),
                reason: format!("Directory '{}' is not writable", parent.display()),
                operation: operation.to_string(),
            });
        }
    }

    // If file already exists, check if it's writable
    if path.exists()
        && path
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    {
        return Err(AosError::CliWritePermissionDenied {
            path: path.display().to_string(),
            reason: "File exists and is read-only".to_string(),
            operation: operation.to_string(),
        });
    }

    Ok(())
}

/// Check if an error is retriable
///
/// Returns an error if the original error is not retriable and a retry was attempted.
pub fn validate_retry_attempt(error: &AosError) -> Result<()> {
    // Check if the error is a non-retriable type
    let is_retriable = match error {
        // Network errors may be retriable depending on the specific type
        AosError::Network(_) => true,
        AosError::Timeout { .. } => true,
        AosError::CircuitBreakerHalfOpen { .. } => true,

        // Auth errors are never retriable (credentials won't change)
        AosError::Auth(_) | AosError::Authz(_) => false,

        // Validation errors are never retriable (input won't change)
        AosError::Validation(_) | AosError::InvalidManifest(_) | AosError::Parse(_) => false,

        // Policy violations require human intervention
        AosError::PolicyViolation(_) | AosError::DeterminismViolation(_) => false,

        // Most other errors can be retried
        _ => true,
    };

    if !is_retriable {
        return Err(AosError::InvalidRetryAttempt {
            error_type: format!("{:?}", std::mem::discriminant(error)),
            reason: "This error type requires human intervention to resolve".to_string(),
            original_error: error.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    // =========================================================================
    // Deprecated Flag Detection Tests
    // =========================================================================

    #[test]
    fn test_deprecated_flags_warning_mode() {
        // In non-strict mode, deprecated flags should not error
        let args = vec!["--verbose".to_string()];
        assert!(validate_deprecated_flags(&args, false).is_ok());
    }

    #[test]
    fn test_deprecated_flags_strict_mode() {
        // In strict mode, deprecated flags should error
        let args = vec!["--verbose".to_string()];
        let result = validate_deprecated_flags(&args, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_deprecated_flags_strict_mode_error_details() {
        let args = vec!["--verbose".to_string()];
        let result = validate_deprecated_flags(&args, true);
        let err = result.unwrap_err();
        match err {
            AosError::DeprecatedFlag {
                flag,
                replacement,
                removal_version,
            } => {
                assert_eq!(flag, "verbose");
                assert_eq!(replacement, "--log-level debug");
                assert_eq!(removal_version, "2.0.0");
            }
            _ => panic!("Expected DeprecatedFlag error, got {:?}", err),
        }
    }

    #[test]
    fn test_deprecated_flags_quiet_flag() {
        let args = vec!["--quiet".to_string()];
        let result = validate_deprecated_flags(&args, true);
        let err = result.unwrap_err();
        match err {
            AosError::DeprecatedFlag {
                flag,
                replacement,
                removal_version,
            } => {
                assert_eq!(flag, "quiet");
                assert_eq!(replacement, "--log-level error");
                assert_eq!(removal_version, "2.0.0");
            }
            _ => panic!("Expected DeprecatedFlag error, got {:?}", err),
        }
    }

    #[test]
    fn test_deprecated_flags_without_dashes() {
        // Flags passed without -- prefix should still be detected
        let args = vec!["verbose".to_string()];
        let result = validate_deprecated_flags(&args, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_deprecated_flags_multiple_args() {
        // Should detect deprecated flag among multiple valid args
        let args = vec![
            "--output".to_string(),
            "file.txt".to_string(),
            "--verbose".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        let result = validate_deprecated_flags(&args, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_deprecated_flags_no_deprecated() {
        // Should pass with no deprecated flags
        let args = vec![
            "--output".to_string(),
            "--format".to_string(),
            "--log-level".to_string(),
            "debug".to_string(),
        ];
        assert!(validate_deprecated_flags(&args, true).is_ok());
    }

    #[test]
    fn test_deprecated_flags_empty_args() {
        let args: Vec<String> = vec![];
        assert!(validate_deprecated_flags(&args, true).is_ok());
    }

    #[test]
    fn test_deprecated_flags_first_match_returned() {
        // When multiple deprecated flags are present, the first one triggers error
        let args = vec!["--verbose".to_string(), "--quiet".to_string()];
        let result = validate_deprecated_flags(&args, true);
        let err = result.unwrap_err();
        match err {
            AosError::DeprecatedFlag { flag, .. } => {
                assert_eq!(flag, "verbose");
            }
            _ => panic!("Expected DeprecatedFlag error"),
        }
    }

    // =========================================================================
    // Input Encoding Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_utf8_valid() {
        let input = b"Hello, world!";
        assert!(validate_utf8_input(input, "test").is_ok());
    }

    #[test]
    fn test_validate_utf8_invalid() {
        let input = b"Hello, \xff\xfe world!";
        let result = validate_utf8_input(input, "test input");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            AosError::InvalidInputEncoding { offset, .. } => {
                assert_eq!(offset, 7); // Invalid byte at position 7
            }
            _ => panic!("Expected InvalidInputEncoding error"),
        }
    }

    #[test]
    fn test_validate_utf8_empty_input() {
        let input = b"";
        let result = validate_utf8_input(input, "empty");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_validate_utf8_multibyte_characters() {
        // Valid UTF-8 with multibyte characters
        let input = "Hello, \u{4e16}\u{754c}!".as_bytes(); // "Hello, 世界!"
        let result = validate_utf8_input(input, "unicode test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, 世界!");
    }

    #[test]
    fn test_validate_utf8_invalid_at_start() {
        let input = b"\xff\xfeHello";
        let result = validate_utf8_input(input, "start invalid");
        let err = result.unwrap_err();
        match err {
            AosError::InvalidInputEncoding { offset, .. } => {
                assert_eq!(offset, 0);
            }
            _ => panic!("Expected InvalidInputEncoding error"),
        }
    }

    #[test]
    fn test_validate_utf8_invalid_at_end() {
        let input = b"Hello\xff";
        let result = validate_utf8_input(input, "end invalid");
        let err = result.unwrap_err();
        match err {
            AosError::InvalidInputEncoding { offset, .. } => {
                assert_eq!(offset, 5);
            }
            _ => panic!("Expected InvalidInputEncoding error"),
        }
    }

    #[test]
    fn test_validate_utf8_error_includes_context() {
        let input = b"\xff";
        let result = validate_utf8_input(input, "stdin from user");
        let err = result.unwrap_err();
        match err {
            AosError::InvalidInputEncoding { context, .. } => {
                assert_eq!(context, "stdin from user");
            }
            _ => panic!("Expected InvalidInputEncoding error"),
        }
    }

    #[test]
    fn test_validate_utf8_error_suggests_binary_flag() {
        let input = b"\xff";
        let result = validate_utf8_input(input, "test");
        let err = result.unwrap_err();
        match err {
            AosError::InvalidInputEncoding { suggested_flag, .. } => {
                assert_eq!(suggested_flag, Some("--binary".to_string()));
            }
            _ => panic!("Expected InvalidInputEncoding error"),
        }
    }

    #[test]
    fn test_validate_utf8_incomplete_multibyte() {
        // Incomplete UTF-8 sequence (first byte of 3-byte sequence)
        let input = b"Hello \xe4";
        let result = validate_utf8_input(input, "incomplete");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_utf8_overlong_encoding() {
        // Overlong encoding of '/' (should be 0x2F, but encoded as 2 bytes)
        let input = b"\xc0\xaf";
        let result = validate_utf8_input(input, "overlong");
        assert!(result.is_err());
    }

    // =========================================================================
    // Permission Checking Tests
    // =========================================================================

    #[test]
    fn test_validate_writable_path_nonexistent_parent() {
        let path = Path::new("/nonexistent/directory/file.txt");
        let result = validate_writable_path(path, "write output");
        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::CliWritePermissionDenied { reason, .. } => {
                assert!(reason.contains("does not exist"));
            }
            _ => panic!("Expected CliWritePermissionDenied error"),
        }
    }

    #[test]
    fn test_validate_writable_path_existing_writable_dir() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");
        let result = validate_writable_path(&file_path, "create file");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_writable_path_existing_readonly_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("readonly_file.txt");

        // Create and make readonly
        File::create(&file_path).unwrap();
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(&file_path, perms).unwrap();

        let result = validate_writable_path(&file_path, "update file");
        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::CliWritePermissionDenied { reason, .. } => {
                assert!(reason.contains("read-only"));
            }
            _ => panic!("Expected CliWritePermissionDenied error"),
        }

        // Cleanup: restore write permission so temp dir can be deleted
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&file_path, perms).unwrap();
    }

    #[test]
    fn test_validate_writable_path_error_includes_operation() {
        let path = Path::new("/nonexistent/file.txt");
        let result = validate_writable_path(path, "export adapter");
        let err = result.unwrap_err();
        match err {
            AosError::CliWritePermissionDenied { operation, .. } => {
                assert_eq!(operation, "export adapter");
            }
            _ => panic!("Expected CliWritePermissionDenied error"),
        }
    }

    #[test]
    fn test_validate_writable_path_error_includes_path() {
        let path = Path::new("/nonexistent/test.txt");
        let result = validate_writable_path(path, "write");
        let err = result.unwrap_err();
        match err {
            AosError::CliWritePermissionDenied { path: err_path, .. } => {
                assert!(err_path.contains("nonexistent"));
            }
            _ => panic!("Expected CliWritePermissionDenied error"),
        }
    }

    #[test]
    fn test_validate_writable_path_root_path() {
        // Path at root level should work if parent exists (root always exists)
        let path = Path::new("/tmp/test_file.txt");
        let result = validate_writable_path(path, "write");
        // /tmp should be writable on most systems
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_writable_path_relative_path() {
        // Relative path - parent is "" which returns Some("") but doesn't "exist"
        // in the filesystem sense. The function uses the current directory.
        // For a path like "simple_file.txt", parent() returns Some("")
        let path = Path::new("simple_file.txt");
        let result = validate_writable_path(path, "write");
        // The parent "" doesn't exist as a path, so this should fail
        // unless we're in a writable current directory
        // This test verifies the function handles relative paths
        assert!(result.is_err());
        match result.unwrap_err() {
            AosError::CliWritePermissionDenied { reason, .. } => {
                // Empty parent string doesn't exist
                assert!(reason.contains("does not exist"));
            }
            _ => panic!("Expected CliWritePermissionDenied error"),
        }
    }

    #[test]
    fn test_validate_writable_path_current_dir_explicit() {
        // Using "./" prefix makes parent "." which exists
        let path = Path::new("./simple_file.txt");
        let result = validate_writable_path(path, "write");
        // "." should exist and be writable in most test environments
        assert!(result.is_ok());
    }

    // =========================================================================
    // Retry Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_retry_attempt_auth_error() {
        let auth_error = AosError::Auth("invalid token".to_string());
        let result = validate_retry_attempt(&auth_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_network_error() {
        let network_error = AosError::Network("connection reset".to_string());
        let result = validate_retry_attempt(&network_error);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_retry_attempt_authz_error() {
        let authz_error = AosError::Authz("insufficient permissions".to_string());
        let result = validate_retry_attempt(&authz_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_validation_error() {
        let validation_error = AosError::Validation("invalid input".to_string());
        let result = validate_retry_attempt(&validation_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_policy_violation() {
        let policy_error = AosError::PolicyViolation("blocked by policy".to_string());
        let result = validate_retry_attempt(&policy_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_determinism_violation() {
        let det_error = AosError::DeterminismViolation("hash mismatch".to_string());
        let result = validate_retry_attempt(&det_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_timeout_error() {
        let timeout_error = AosError::Timeout {
            duration: std::time::Duration::from_secs(30),
        };
        let result = validate_retry_attempt(&timeout_error);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_retry_attempt_circuit_breaker_half_open() {
        let cb_error = AosError::CircuitBreakerHalfOpen {
            service: "worker".to_string(),
        };
        let result = validate_retry_attempt(&cb_error);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_retry_attempt_invalid_manifest() {
        let manifest_error = AosError::InvalidManifest("missing field".to_string());
        let result = validate_retry_attempt(&manifest_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_parse_error() {
        let parse_error = AosError::Parse("invalid JSON".to_string());
        let result = validate_retry_attempt(&parse_error);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_retry_attempt_io_error_retriable() {
        // I/O errors should be retriable (might be transient)
        let io_error = AosError::Io("connection interrupted".to_string());
        let result = validate_retry_attempt(&io_error);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_retry_attempt_error_details() {
        let auth_error = AosError::Auth("token expired".to_string());
        let result = validate_retry_attempt(&auth_error);
        let err = result.unwrap_err();
        match err {
            AosError::InvalidRetryAttempt {
                reason,
                original_error,
                ..
            } => {
                assert!(reason.contains("human intervention"));
                assert!(original_error.contains("token expired"));
            }
            _ => panic!("Expected InvalidRetryAttempt error"),
        }
    }

    // =========================================================================
    // Error Code Generation Tests
    // =========================================================================

    /// Test that CLI-specific errors map to correct exit codes
    ///
    /// Note: For error code (ECode) mapping, use the compile-time checked
    /// `AosError::ecode()` method from the unified error registry.
    #[test]
    fn test_cli_error_exit_code_mapping() {
        use crate::error_codes::ExitCode;

        // DeprecatedFlag should map to DeprecatedFlag exit code
        let deprecated_err = AosError::DeprecatedFlag {
            flag: "verbose".to_string(),
            replacement: "--log-level debug".to_string(),
            removal_version: "2.0.0".to_string(),
        };
        assert_eq!(ExitCode::from(&deprecated_err), ExitCode::DeprecatedFlag);

        // InvalidInputEncoding should map to InputEncoding exit code
        let encoding_err = AosError::InvalidInputEncoding {
            offset: 10,
            context: "stdin".to_string(),
            suggested_flag: Some("--binary".to_string()),
        };
        assert_eq!(ExitCode::from(&encoding_err), ExitCode::InputEncoding);

        // CliWritePermissionDenied should map to Io exit code
        let perm_err = AosError::CliWritePermissionDenied {
            path: "/tmp/file.txt".to_string(),
            reason: "read-only".to_string(),
            operation: "write".to_string(),
        };
        assert_eq!(ExitCode::from(&perm_err), ExitCode::Io);

        // InvalidRetryAttempt should map to InvalidRetry exit code
        let retry_err = AosError::InvalidRetryAttempt {
            error_type: "Auth".to_string(),
            reason: "not retriable".to_string(),
            original_error: "auth failed".to_string(),
        };
        assert_eq!(ExitCode::from(&retry_err), ExitCode::InvalidRetry);
    }

    #[test]
    fn test_exit_code_category_for_cli_errors() {
        use crate::error_codes::ExitCode;

        // All CLI-related exit codes should be in Configuration category (10-19)
        assert_eq!(ExitCode::DeprecatedFlag.category(), "Configuration");
        assert_eq!(ExitCode::OutputFormat.category(), "Configuration");
        assert_eq!(ExitCode::InputEncoding.category(), "Configuration");
        assert_eq!(ExitCode::InvalidRetry.category(), "Configuration");
    }

    #[test]
    fn test_error_display_format() {
        // Verify error messages are formatted correctly
        let deprecated_err = AosError::DeprecatedFlag {
            flag: "verbose".to_string(),
            replacement: "--log-level debug".to_string(),
            removal_version: "2.0.0".to_string(),
        };
        let msg = format!("{}", deprecated_err);
        assert!(msg.contains("verbose"));
        assert!(msg.contains("--log-level debug"));
        assert!(msg.contains("2.0.0"));

        let encoding_err = AosError::InvalidInputEncoding {
            offset: 42,
            context: "stdin".to_string(),
            suggested_flag: Some("--binary".to_string()),
        };
        let msg = format!("{}", encoding_err);
        assert!(msg.contains("42"));
        assert!(msg.contains("UTF-8"));
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[test]
    fn test_validation_pipeline_deprecated_then_encoding() {
        // Simulate a validation pipeline: first check deprecated flags, then encoding
        let args = vec!["--output".to_string(), "file.txt".to_string()];
        let input = b"valid utf8 content";

        // Step 1: Check deprecated flags
        assert!(validate_deprecated_flags(&args, true).is_ok());

        // Step 2: Validate input encoding
        assert!(validate_utf8_input(input, "user input").is_ok());
    }

    #[test]
    fn test_deprecated_flag_list_completeness() {
        // Verify all deprecated flags have proper replacements defined
        for (flag, replacement, version) in DEPRECATED_FLAGS {
            assert!(!flag.is_empty(), "Flag name should not be empty");
            assert!(
                !replacement.is_empty(),
                "Replacement for {} should not be empty",
                flag
            );
            assert!(
                version.contains('.'),
                "Version {} should be semver format",
                version
            );
        }
    }
}
