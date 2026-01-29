//! Debug Bypass Flag Utilities (SEC-BYPASS-001)
//!
//! This module provides secure handling of development bypass flags.
//! All bypass flags (AOS_SKIP_*, AOS_DEBUG_*, AOS_DEV_*) are strictly
//! limited to debug builds and are ignored in release builds for security.
//!
//! # Security Invariant
//!
//! Bypass flags MUST NEVER affect behavior in release builds. The functions
//! in this module guarantee this through compile-time `#[cfg(debug_assertions)]`
//! guards that make bypass checks always return `false` in release builds.
//!
//! # Usage
//!
//! ```rust
//! use adapteros_core::debug_bypass::is_bypass_enabled;
//!
//! // This will only return true in debug builds when the env var is set
//! if is_bypass_enabled("AOS_SKIP_SOME_CHECK") {
//!     // Skip the check (debug only)
//! } else {
//!     // Perform the check (always in release)
//! }
//! ```
//!
//! # Bypass Flag Categories
//!
//! | Prefix | Purpose | Example |
//! |--------|---------|---------|
//! | `AOS_SKIP_*` | Skip verification/validation | `AOS_SKIP_MIGRATION_SIGNATURES` |
//! | `AOS_DEBUG_*` | Enable debug logging/features | `AOS_DEBUG_DETERMINISM` |
//! | `AOS_DEV_*` | Development-only features | `AOS_DEV_NO_AUTH` |

/// Check if a bypass flag is enabled.
///
/// This function is cfg-gated to ensure bypass flags are NEVER honored
/// in release builds. In release mode, this always returns `false` and
/// logs a warning if the env var is set.
///
/// # Arguments
///
/// * `flag_name` - The name of the environment variable (e.g., "AOS_SKIP_MIGRATION_SIGNATURES")
///
/// # Returns
///
/// * `true` if running in debug mode AND the env var is set to a truthy value
/// * `false` in all other cases (including release builds)
///
/// # Security
///
/// This function is the ONLY safe way to check bypass flags. Direct `std::env::var`
/// checks bypass the security gate and can lead to vulnerabilities in release builds.
pub fn is_bypass_enabled(flag_name: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var(flag_name)
            .map(|v| is_truthy(&v))
            .unwrap_or(false)
    }

    #[cfg(not(debug_assertions))]
    {
        // In release builds, log a warning if the flag is set (helps detect misconfigurations)
        // but NEVER honor it.
        if std::env::var(flag_name).is_ok() {
            // Use eprintln since tracing may not be initialized
            eprintln!(
                "SECURITY WARNING: {} is set but IGNORED in release builds",
                flag_name
            );
        }
        false
    }
}

/// Check if a bypass flag is enabled, with logging.
///
/// Like `is_bypass_enabled`, but logs when the bypass is activated.
/// Use this for bypass flags that have security implications.
///
/// # Arguments
///
/// * `flag_name` - The name of the environment variable
/// * `context` - A description of what's being bypassed (for logging)
///
/// # Returns
///
/// * `true` if running in debug mode AND the env var is set to a truthy value
/// * `false` in all other cases
pub fn is_bypass_enabled_with_log(flag_name: &str, context: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        let enabled = std::env::var(flag_name)
            .map(|v| is_truthy(&v))
            .unwrap_or(false);
        if enabled {
            tracing::warn!(
                flag = flag_name,
                context = context,
                build = "debug",
                "DEBUG-ONLY bypass enabled; {} will be skipped",
                context
            );
        }
        enabled
    }

    #[cfg(not(debug_assertions))]
    {
        if std::env::var(flag_name).is_ok() {
            tracing::warn!(
                flag = flag_name,
                context = context,
                build = "release",
                "{} is set but IGNORED in release builds for security",
                flag_name
            );
        }
        false
    }
}

/// Check if any of multiple bypass flags are enabled.
///
/// Use this when multiple flags can trigger the same bypass (e.g., for
/// backwards compatibility with renamed flags).
///
/// # Arguments
///
/// * `flag_names` - A slice of environment variable names to check
///
/// # Returns
///
/// * `Some(flag_name)` if running in debug mode AND any flag is set
/// * `None` in all other cases
pub fn any_bypass_enabled<'a>(flag_names: &[&'a str]) -> Option<&'a str> {
    #[cfg(debug_assertions)]
    {
        flag_names
            .iter()
            .copied()
            .find(|name| std::env::var(name).map(|v| is_truthy(&v)).unwrap_or(false))
    }

    #[cfg(not(debug_assertions))]
    {
        // Log warnings for any flags that are set but ignored
        for name in flag_names {
            if std::env::var(name).is_ok() {
                tracing::warn!(
                    flag = *name,
                    build = "release",
                    "{} is set but IGNORED in release builds for security",
                    name
                );
            }
        }
        None
    }
}

/// Check if a value is considered "truthy".
///
/// Returns true for: "1", "true", "yes", "on" (case-insensitive)
fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Returns whether the current build is a debug build.
///
/// This is a compile-time constant that can be used for conditional logic.
#[inline]
pub const fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

/// Returns whether the current build is a release build.
///
/// This is a compile-time constant that can be used for conditional logic.
#[inline]
pub const fn is_release_build() -> bool {
    !cfg!(debug_assertions)
}

// =============================================================================
// Known Bypass Flags (Documentation)
// =============================================================================
//
// This section documents all known bypass flags in the codebase.
// When adding a new bypass flag, document it here.
//
// ## Migration/Signature Bypasses
// - AOS_SKIP_MIGRATION_SIGNATURES: Skip database migration signature verification
// - AOS_SKIP_KERNEL_SIGNATURE_VERIFY: Skip Metal kernel signature verification
// - AOS_DEBUG_SKIP_KERNEL_SIG: Alias for AOS_SKIP_KERNEL_SIGNATURE_VERIFY
// - AOS_DEV_SKIP_METALLIB_CHECK: Skip metallib hash verification
// - AOS_SKIP_MODEL_HASH_VERIFY: Skip model integrity hash verification
// - AOS_DEV_SIGNATURE_BYPASS: Skip bundle signature verification
//
// ## Security Bypasses
// - AOS_DEV_NO_AUTH: Skip authentication (use dev admin claims)
// - AOS_DEV_DISABLE_TENANT_CHECK: Skip tenant isolation checks
// - AOS_DEV_RBAC_BYPASS: Skip RBAC permission checks
// - AOS_SKIP_BUNDLE_SIGNATURE: Skip adapter bundle signature verification
// - AOS_SKIP_MANIFEST_HASH: Skip manifest hash verification
//
// ## PF/Network Bypasses
// - AOS_SKIP_PF_CHECK: Skip PF deny preflight check
// - AOS_SKIP_SYMLINK_CHECK: Skip symlink security validation
//
// ## Debug/Trace Flags
// - AOS_DEBUG_DETERMINISM: Enable determinism debug logging
// - AOS_DEBUG_ENABLED: Enable general debug features
// - AOS_DEBUG_PROFILING: Enable profiling
// - AOS_DEBUG_TRACE_REQUESTS: Enable request tracing
//
// ## Other
// - AOS_SKIP_DOTENV: Skip loading .env file

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_truthy() {
        assert!(is_truthy("1"));
        assert!(is_truthy("true"));
        assert!(is_truthy("TRUE"));
        assert!(is_truthy("True"));
        assert!(is_truthy("yes"));
        assert!(is_truthy("YES"));
        assert!(is_truthy("on"));
        assert!(is_truthy("ON"));

        assert!(!is_truthy("0"));
        assert!(!is_truthy("false"));
        assert!(!is_truthy("no"));
        assert!(!is_truthy("off"));
        assert!(!is_truthy(""));
        assert!(!is_truthy("maybe"));
    }

    #[test]
    fn test_is_debug_build() {
        // This test verifies the build type detection
        #[cfg(debug_assertions)]
        {
            assert!(is_debug_build());
            assert!(!is_release_build());
        }

        #[cfg(not(debug_assertions))]
        {
            assert!(!is_debug_build());
            assert!(is_release_build());
        }
    }

    #[test]
    fn test_bypass_flag_behavior_in_current_build() {
        // Set a test flag
        std::env::set_var("AOS_TEST_BYPASS_FLAG", "1");

        #[cfg(debug_assertions)]
        {
            // In debug builds, the flag should be honored
            assert!(is_bypass_enabled("AOS_TEST_BYPASS_FLAG"));
        }

        #[cfg(not(debug_assertions))]
        {
            // In release builds, the flag should be ignored
            assert!(!is_bypass_enabled("AOS_TEST_BYPASS_FLAG"));
        }

        std::env::remove_var("AOS_TEST_BYPASS_FLAG");
    }

    #[test]
    fn test_bypass_returns_false_when_not_set() {
        // Ensure the flag is not set
        std::env::remove_var("AOS_NONEXISTENT_FLAG");

        // Should always return false when not set
        assert!(!is_bypass_enabled("AOS_NONEXISTENT_FLAG"));
    }

    #[test]
    fn test_any_bypass_enabled() {
        std::env::remove_var("AOS_FLAG_A");
        std::env::remove_var("AOS_FLAG_B");

        // None set - should return None
        assert!(any_bypass_enabled(&["AOS_FLAG_A", "AOS_FLAG_B"]).is_none());

        // Set one flag
        std::env::set_var("AOS_FLAG_B", "1");

        #[cfg(debug_assertions)]
        {
            assert_eq!(
                any_bypass_enabled(&["AOS_FLAG_A", "AOS_FLAG_B"]),
                Some("AOS_FLAG_B")
            );
        }

        #[cfg(not(debug_assertions))]
        {
            // In release, should still return None
            assert!(any_bypass_enabled(&["AOS_FLAG_A", "AOS_FLAG_B"]).is_none());
        }

        std::env::remove_var("AOS_FLAG_B");
    }

    /// This test documents and verifies the security invariant:
    /// bypass flags MUST be ignored in release builds.
    #[test]
    fn security_invariant_bypass_flags_ignored_in_release() {
        // This test runs in both debug and release modes
        // and verifies the expected behavior for each.

        let test_flag = "AOS_TEST_SECURITY_INVARIANT";
        std::env::set_var(test_flag, "1");

        let bypass_active = is_bypass_enabled(test_flag);

        #[cfg(debug_assertions)]
        {
            // Debug: bypass is active
            assert!(
                bypass_active,
                "In debug builds, bypass flags should be honored"
            );
        }

        #[cfg(not(debug_assertions))]
        {
            // Release: bypass is NEVER active
            assert!(
                !bypass_active,
                "SECURITY VIOLATION: bypass flag was honored in release build!"
            );
        }

        std::env::remove_var(test_flag);
    }
}
