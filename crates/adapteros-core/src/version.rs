//! Version Information Module
//!
//! This module provides a single source of truth for all adapterOS version information.
//! All version numbers should be read from this module to ensure consistency across
//! the codebase.
//!
//! # Version Strategy
//!
//! adapterOS uses Semantic Versioning (SemVer) with an optional phase suffix:
//! - Format: `MAJOR.MINOR.PATCH[-PHASE]`
//! - Example: `1.0.0-alpha`, `1.0.0-beta`, `1.0.0`
//!
//! Version increments:
//! - **MAJOR**: Breaking changes, incompatible API changes
//! - **MINOR**: New features, backward-compatible additions
//! - **PATCH**: Bug fixes, backward-compatible fixes
//! - **PHASE**: Development phase (alpha, beta, rc)
//!
//! # Version Components
//!
//! - **Product Version**: Overall adapterOS release version
//! - **API Schema Version**: REST API compatibility version
//! - **Database Schema Version**: Migration sequence number
//! - **Build Metadata**: Git commit, build timestamp, etc.
//!
//! # Algorithm Version Tracking
//!
//! For determinism and cross-version replay, we track algorithm versions:
//! - [`HKDF_ALGORITHM_VERSION`]: Seed derivation algorithm (from seed.rs)
//! - [`PARSER_ALGORITHM_VERSION`]: Directory parsing/sorting algorithm
//! - [`PATH_NORMALIZATION_VERSION`]: Cross-platform path normalization
//! - [`HASH_ALGORITHM_VERSION`]: Dataset content hashing algorithm
//!
//! See [`AlgorithmVersionBundle`] for bundled version tracking.

use serde::{Deserialize, Serialize};
use std::fmt;

// =============================================================================
// Compile-Time Parsing Utilities
// =============================================================================

/// Parse a string slice to u32 at compile time
///
/// Used by DATABASE_SCHEMA_VERSION to parse the build-time env var.
/// Returns None if the string is not a valid u32.
const fn const_parse_u32(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte < b'0' || byte > b'9' {
            return None;
        }
        let digit = (byte - b'0') as u32;

        // Check for overflow before multiplying
        if let Some(temp) = result.checked_mul(10) {
            if let Some(new_result) = temp.checked_add(digit) {
                result = new_result;
            } else {
                return None; // Overflow on add
            }
        } else {
            return None; // Overflow on multiply
        }

        i += 1;
    }

    Some(result)
}

// =============================================================================
// Algorithm Version Constants for Determinism Tracking
// =============================================================================

/// Parser algorithm version for determinism tracking.
///
/// Increment when parsing/sorting logic changes in a way that would
/// produce different file ordering for the same directory tree.
///
/// # Version History
/// - v1: Initial implementation with `path.cmp()` sorting (platform-dependent)
/// - v2: Normalized path separators for cross-platform consistency
pub const PARSER_ALGORITHM_VERSION: u32 = 2;

/// Path normalization version for cross-platform determinism.
///
/// Increment when path normalization logic changes in a way that would
/// produce different sort order for the same paths.
///
/// # Version History
/// - v1: Platform-native separators (non-deterministic across platforms)
/// - v2: Unix-style forward slashes everywhere, collapsed doubles, NFC Unicode
pub const PATH_NORMALIZATION_VERSION: u32 = 2;

/// Hash algorithm version for dataset content hashing.
///
/// Mirrors `hash_algorithm_version` in the training_datasets table.
/// Increment when content hashing logic changes.
///
/// # Version History
/// - v1: Original hash implementation
/// - v2: Normalized filenames (lowercase + NFD + trim)
pub const HASH_ALGORITHM_VERSION: u32 = 2;

/// Re-export HKDF algorithm version from seed module.
///
/// This value comes from `seed::HKDF_ALGORITHM_VERSION`.
/// We re-export it here for completeness in `AlgorithmVersionBundle`.
pub use crate::seed::HKDF_ALGORITHM_VERSION;

// =============================================================================
// Algorithm Version Bundle
// =============================================================================

/// Bundle of algorithm versions affecting determinism.
///
/// This struct captures all algorithm versions that affect hash computation
/// and seed derivation. Storing this bundle with datasets and training jobs
/// enables cross-version replay by detecting when algorithm versions differ.
///
/// # Usage
///
/// ```ignore
/// use adapteros_core::version::AlgorithmVersionBundle;
///
/// // Get current compile-time versions
/// let bundle = AlgorithmVersionBundle::current();
///
/// // Store with dataset for later replay validation
/// let params = CreateDatasetHashInputsParams {
///     hkdf_version: Some(bundle.hkdf_version),
///     parser_version: Some(bundle.parser_version),
///     ..
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlgorithmVersionBundle {
    /// HKDF seed derivation version (from seed.rs HKDF_ALGORITHM_VERSION)
    pub hkdf_version: u32,
    /// Directory parsing/sorting algorithm version
    pub parser_version: u32,
    /// Path normalization algorithm version (cross-platform consistency)
    pub path_normalization_version: u32,
    /// Codegraph symbol extraction version (crate version string)
    pub codegraph_version: Option<String>,
    /// Hash algorithm version (filename normalization)
    pub hash_algorithm_version: u32,
}

impl AlgorithmVersionBundle {
    /// Get the current runtime versions from compile-time constants.
    ///
    /// This returns the versions compiled into the current binary.
    pub fn current() -> Self {
        Self {
            hkdf_version: HKDF_ALGORITHM_VERSION,
            parser_version: PARSER_ALGORITHM_VERSION,
            path_normalization_version: PATH_NORMALIZATION_VERSION,
            codegraph_version: Some(VERSION.to_string()),
            hash_algorithm_version: HASH_ALGORITHM_VERSION,
        }
    }

    /// Create a bundle representing legacy/unknown versions.
    ///
    /// Used when loading data that doesn't have version info stored.
    /// All versions default to 1 (initial version).
    pub fn legacy() -> Self {
        Self {
            hkdf_version: 1,
            parser_version: 1,
            path_normalization_version: 1,
            codegraph_version: None,
            hash_algorithm_version: 1,
        }
    }

    /// Check if this bundle is compatible with the current runtime.
    ///
    /// Returns `Ok(())` if compatible, or an error describing the incompatibility.
    pub fn check_runtime_compatibility(&self) -> Result<(), VersionIncompatibility> {
        let current = Self::current();

        // HKDF version mismatch is always an error - affects seed derivation
        if self.hkdf_version != current.hkdf_version {
            return Err(VersionIncompatibility {
                component: "hkdf".to_string(),
                stored: self.hkdf_version,
                current: current.hkdf_version,
                severity: IncompatibilitySeverity::Breaking,
                reason: "HKDF algorithm versions must match for deterministic replay".into(),
            });
        }

        // Path normalization mismatch affects hash reproducibility
        if self.path_normalization_version != current.path_normalization_version {
            return Err(VersionIncompatibility {
                component: "path_normalization".to_string(),
                stored: self.path_normalization_version,
                current: current.path_normalization_version,
                severity: IncompatibilitySeverity::Breaking,
                reason: "Path normalization affects sort order and thus hash".into(),
            });
        }

        // Hash algorithm version mismatch affects content hash
        if self.hash_algorithm_version != current.hash_algorithm_version {
            return Err(VersionIncompatibility {
                component: "hash_algorithm".to_string(),
                stored: self.hash_algorithm_version,
                current: current.hash_algorithm_version,
                severity: IncompatibilitySeverity::Breaking,
                reason: "Hash algorithm change affects content hash computation".into(),
            });
        }

        Ok(())
    }

    /// Check compatibility and return warnings for non-breaking differences.
    ///
    /// Unlike `check_runtime_compatibility`, this returns warnings for
    /// differences that don't break determinism.
    pub fn check_with_warnings(&self) -> (bool, Vec<String>) {
        let current = Self::current();
        let mut warnings = Vec::new();

        // Check breaking incompatibilities first
        if let Err(e) = self.check_runtime_compatibility() {
            return (false, vec![e.to_string()]);
        }

        // Parser version mismatch is a warning (may affect symbol extraction)
        if self.parser_version != current.parser_version {
            warnings.push(format!(
                "Parser version mismatch: stored={}, current={}",
                self.parser_version, current.parser_version
            ));
        }

        // Codegraph version mismatch is informational
        if self.codegraph_version != current.codegraph_version {
            warnings.push(format!(
                "Codegraph version differs: stored={:?}, current={:?}",
                self.codegraph_version, current.codegraph_version
            ));
        }

        (true, warnings)
    }
}

impl Default for AlgorithmVersionBundle {
    fn default() -> Self {
        Self::current()
    }
}

/// Describes an incompatibility between stored and current algorithm versions.
#[derive(Debug, Clone)]
pub struct VersionIncompatibility {
    /// Which component has the version mismatch
    pub component: String,
    /// The version stored with the data
    pub stored: u32,
    /// The current runtime version
    pub current: u32,
    /// Severity of the incompatibility
    pub severity: IncompatibilitySeverity,
    /// Human-readable explanation
    pub reason: String,
}

impl fmt::Display for VersionIncompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} version incompatibility: stored v{}, current v{} - {}",
            self.component, self.stored, self.current, self.reason
        )
    }
}

impl std::error::Error for VersionIncompatibility {}

/// Severity level for version incompatibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncompatibilitySeverity {
    /// Breaking: Cannot reproduce deterministic results
    Breaking,
    /// Warning: May affect results but not determinism-critical
    Warning,
}

// =============================================================================
// Product Version Constants
// =============================================================================

/// Current adapterOS release version
///
/// This is the primary version number for the entire adapterOS product.
/// It's synchronized across all crates via Cargo workspace inheritance.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// API schema version for REST API compatibility tracking
///
/// Increment policy:
/// - **Major**: Breaking REST API changes (incompatible request/response formats)
/// - **Minor**: New endpoints or optional fields (backward compatible)
/// - **Patch**: Bug fixes in schema definitions
pub const API_SCHEMA_VERSION: &str = "1.0.0";

/// Database schema version (migration sequence number)
///
/// This represents the latest migration number in /migrations/.
/// Automatically computed from the migrations directory at build time.
/// Check against `_sqlx_migrations` table for current database version.
///
/// # Implementation
///
/// The version is computed by the build script (build.rs) which scans
/// the migrations/ directory and finds the highest numeric migration prefix.
/// Supports both formats:
/// - Four-digit: `0001_init.sql` through `0297_embedding_benchmarks.sql`
/// - Timestamp: `20260112125636_add_dataset_validation_json.sql`
///
/// The build script sets the `DATABASE_SCHEMA_VERSION` environment variable,
/// which is consumed here via `env!()`.
pub const DATABASE_SCHEMA_VERSION: u32 = match option_env!("DATABASE_SCHEMA_VERSION") {
    Some(v) => match const_parse_u32(v) {
        Some(n) => n,
        None => panic!("DATABASE_SCHEMA_VERSION env var is not a valid u32"),
    },
    None => panic!("DATABASE_SCHEMA_VERSION not set by build script. Run cargo build."),
};

/// RNG module version for determinism tracking
///
/// Tracks the cryptographic randomness implementation version.
/// Format: `{MAJOR}.{MINOR}.{PATCH}-{algorithm}`
pub const RNG_MODULE_VERSION: &str = "1.0.0-chacha20";

/// Git commit hash at build time (short form)
///
/// Set via `CARGO_GIT_HASH` environment variable during build.
/// Falls back to "unknown" if not available.
pub const GIT_COMMIT_HASH: &str = match option_env!("CARGO_GIT_HASH") {
    Some(hash) => hash,
    None => "unknown",
};

/// Build timestamp in compact format (YYYYMMDDHHmmss)
///
/// Set via `BUILD_TIMESTAMP` environment variable during build.
/// Falls back to "unknown" if not available.
pub const BUILD_TIMESTAMP: &str = match option_env!("BUILD_TIMESTAMP") {
    Some(ts) => ts,
    None => "unknown",
};

/// Combined build identifier: `{git_hash}-{timestamp}`
///
/// Set via `AOS_BUILD_ID` environment variable during build.
/// Example: "a6922d2-20260205143045"
pub const BUILD_ID: &str = match option_env!("AOS_BUILD_ID") {
    Some(id) => id,
    None => "unknown",
};

/// Rust compiler version used for compilation
///
/// Set via `RUSTC_VERSION` environment variable during build.
/// Example: "rustc 1.84.0 (9fc6b4312 2025-01-07)"
pub const RUSTC_VERSION: &str = match option_env!("RUSTC_VERSION") {
    Some(v) => v,
    None => "unknown",
};

/// Build profile (debug or release)
pub const BUILD_PROFILE: &str = if cfg!(debug_assertions) {
    "debug"
} else {
    "release"
};

/// Target architecture (e.g., aarch64, x86_64)
pub const TARGET_ARCH: &str = {
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "unknown"
    }
};

/// Target operating system (e.g., macos, linux)
pub const TARGET_OS: &str = {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "unknown"
    }
};

/// Comprehensive version information
///
/// Contains all version-related metadata for diagnostics and auditing.
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// Product release version (e.g., "1.0.0-alpha")
    pub release: String,
    /// API schema version for compatibility checks
    pub api_schema: String,
    /// Database schema version (migration number)
    pub database_schema: u32,
    /// RNG module version for determinism tracking
    pub rng_module: String,
    /// Git commit hash (short)
    pub git_commit: String,
    /// Build timestamp
    pub build_timestamp: String,
    /// Combined build identifier ({hash}-{timestamp})
    pub build_id: String,
    /// Rust compiler version
    pub rustc_version: String,
    /// Build profile (debug/release)
    pub build_profile: String,
    /// Target architecture
    pub target_arch: String,
    /// Target OS
    pub target_os: String,
}

impl VersionInfo {
    /// Get current version information
    ///
    /// # Examples
    ///
    /// ```
    /// use adapteros_core::version::VersionInfo;
    ///
    /// let info = VersionInfo::current();
    /// println!("adapterOS v{}", info.release);
    /// println!("API Schema: v{}", info.api_schema);
    /// println!("DB Schema: v{}", info.database_schema);
    /// ```
    pub fn current() -> Self {
        Self {
            release: VERSION.to_string(),
            api_schema: API_SCHEMA_VERSION.to_string(),
            database_schema: DATABASE_SCHEMA_VERSION,
            rng_module: RNG_MODULE_VERSION.to_string(),
            git_commit: GIT_COMMIT_HASH.to_string(),
            build_timestamp: BUILD_TIMESTAMP.to_string(),
            build_id: BUILD_ID.to_string(),
            rustc_version: RUSTC_VERSION.to_string(),
            build_profile: BUILD_PROFILE.to_string(),
            target_arch: TARGET_ARCH.to_string(),
            target_os: TARGET_OS.to_string(),
        }
    }

    /// Get short version string for display (e.g., "1.0.0-alpha")
    pub fn short(&self) -> &str {
        &self.release
    }

    /// Get full version string with build ID (e.g., "1.0.0-alpha (a6922d2-20260205)")
    pub fn full(&self) -> String {
        if self.build_id == "unknown" {
            if self.git_commit == "unknown" {
                self.release.clone()
            } else {
                format!(
                    "{} ({})",
                    self.release,
                    &self.git_commit[..7.min(self.git_commit.len())]
                )
            }
        } else {
            format!("{} ({})", self.release, self.build_id)
        }
    }

    /// Check if current database schema version matches expected
    ///
    /// # Arguments
    ///
    /// * `actual_version` - The database's current schema version from `_sqlx_migrations`
    ///
    /// # Returns
    ///
    /// * `Ok(())` if versions match
    /// * `Err(String)` with diagnostic message if mismatch detected
    pub fn verify_database_version(&self, actual_version: u32) -> Result<(), String> {
        if actual_version == self.database_schema {
            Ok(())
        } else if actual_version < self.database_schema {
            Err(format!(
                "Database schema is BEHIND: database at v{}, expected v{}. \
                 Missing {} migrations. Run: aosctl db migrate",
                actual_version,
                self.database_schema,
                self.database_schema - actual_version
            ))
        } else {
            Err(format!(
                "Database schema is AHEAD: database at v{}, expected v{}. \
                 Code rollback or migration file removal detected. \
                 This binary cannot safely operate with this database schema.",
                actual_version, self.database_schema
            ))
        }
    }
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "adapterOS {}", self.release)?;
        writeln!(f, "Build ID:        {}", self.build_id)?;
        writeln!(f, "API Schema:      v{}", self.api_schema)?;
        writeln!(f, "Database Schema: v{}", self.database_schema)?;
        writeln!(f, "RNG Module:      v{}", self.rng_module)?;
        writeln!(f, "Git Commit:      {}", self.git_commit)?;
        writeln!(f, "Build Time:      {}", self.build_timestamp)?;
        writeln!(f, "Rustc:           {}", self.rustc_version)?;
        writeln!(f, "Build Profile:   {}", self.build_profile)?;
        writeln!(
            f,
            "Target:          {}-{}",
            self.target_os, self.target_arch
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constants() {
        // Ensure version constants are populated
        assert!(!VERSION.is_empty(), "VERSION must not be empty");
        assert!(
            !API_SCHEMA_VERSION.is_empty(),
            "API_SCHEMA_VERSION must not be empty"
        );
        const _: () = assert!(DATABASE_SCHEMA_VERSION > 0);
        assert!(
            !RNG_MODULE_VERSION.is_empty(),
            "RNG_MODULE_VERSION must not be empty"
        );

        // DATABASE_SCHEMA_VERSION should be computed from migrations at build time
        // It should be at least 297 (as of Jan 2026)
        println!(
            "DATABASE_SCHEMA_VERSION computed at build time: {}",
            DATABASE_SCHEMA_VERSION
        );
        // Use const block for compile-time assertion on constants
        const { assert!(DATABASE_SCHEMA_VERSION >= 297) };
    }

    #[test]
    fn test_version_format() {
        // VERSION should follow semver format (MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-PHASE)
        let parts: Vec<&str> = VERSION.split(&['.', '-'][..]).collect();
        assert!(
            parts.len() >= 3,
            "VERSION should have at least MAJOR.MINOR.PATCH: {}",
            VERSION
        );

        // API_SCHEMA_VERSION should be strict semver (MAJOR.MINOR.PATCH)
        let api_parts: Vec<&str> = API_SCHEMA_VERSION.split('.').collect();
        assert_eq!(
            api_parts.len(),
            3,
            "API_SCHEMA_VERSION should be MAJOR.MINOR.PATCH: {}",
            API_SCHEMA_VERSION
        );
    }

    #[test]
    fn test_version_info_creation() {
        let info = VersionInfo::current();
        assert_eq!(info.release, VERSION);
        assert_eq!(info.api_schema, API_SCHEMA_VERSION);
        assert_eq!(info.database_schema, DATABASE_SCHEMA_VERSION);
        assert_eq!(info.rng_module, RNG_MODULE_VERSION);
    }

    #[test]
    fn test_version_info_display() {
        let info = VersionInfo::current();
        let display = format!("{}", info);
        assert!(display.contains("adapterOS"));
        assert!(display.contains("API Schema"));
        assert!(display.contains("Database Schema"));
    }

    #[test]
    fn test_version_short_full() {
        let info = VersionInfo::current();
        assert_eq!(info.short(), VERSION);

        let full = info.full();
        assert!(full.contains(VERSION));
    }

    #[test]
    fn test_build_metadata_populated() {
        // After build.rs changes, these should no longer be "unknown"
        assert_ne!(
            GIT_COMMIT_HASH, "unknown",
            "GIT_COMMIT_HASH should be set by build.rs"
        );
        assert_ne!(
            BUILD_TIMESTAMP, "unknown",
            "BUILD_TIMESTAMP should be set by build.rs"
        );
        assert_ne!(BUILD_ID, "unknown", "BUILD_ID should be set by build.rs");
        assert_ne!(
            RUSTC_VERSION, "unknown",
            "RUSTC_VERSION should be set by build.rs"
        );

        // BUILD_ID should contain a dash (hash-timestamp format)
        assert!(
            BUILD_ID.contains('-'),
            "BUILD_ID should be in hash-timestamp format: {}",
            BUILD_ID
        );

        let info = VersionInfo::current();
        assert_eq!(info.build_id, BUILD_ID);
        assert_eq!(info.rustc_version, RUSTC_VERSION);
    }

    #[test]
    fn test_database_version_verification() {
        let info = VersionInfo::current();

        // Matching version should succeed
        assert!(info
            .verify_database_version(DATABASE_SCHEMA_VERSION)
            .is_ok());

        // Behind version should error with helpful message
        let behind_result = info.verify_database_version(DATABASE_SCHEMA_VERSION - 1);
        assert!(behind_result.is_err());
        let behind_err = behind_result.unwrap_err();
        assert!(behind_err.contains("BEHIND"));
        assert!(behind_err.contains("aosctl db migrate"));

        // Ahead version should error with safety warning
        let ahead_result = info.verify_database_version(DATABASE_SCHEMA_VERSION + 1);
        assert!(ahead_result.is_err());
        let ahead_err = ahead_result.unwrap_err();
        assert!(ahead_err.contains("AHEAD"));
        assert!(ahead_err.contains("cannot safely operate"));
    }

    // =========================================================================
    // Algorithm Version Bundle Tests
    // =========================================================================

    #[test]
    fn test_algorithm_version_constants() {
        // Ensure algorithm version constants are at expected values
        const { assert!(PARSER_ALGORITHM_VERSION >= 2) };
        const { assert!(PATH_NORMALIZATION_VERSION >= 2) };
        const { assert!(HASH_ALGORITHM_VERSION >= 2) };
        const { assert!(HKDF_ALGORITHM_VERSION >= 2) };
    }

    #[test]
    fn test_algorithm_version_bundle_current() {
        let bundle = AlgorithmVersionBundle::current();
        assert_eq!(bundle.hkdf_version, HKDF_ALGORITHM_VERSION);
        assert_eq!(bundle.parser_version, PARSER_ALGORITHM_VERSION);
        assert_eq!(
            bundle.path_normalization_version,
            PATH_NORMALIZATION_VERSION
        );
        assert_eq!(bundle.hash_algorithm_version, HASH_ALGORITHM_VERSION);
        assert!(bundle.codegraph_version.is_some());
    }

    #[test]
    fn test_algorithm_version_bundle_legacy() {
        let bundle = AlgorithmVersionBundle::legacy();
        assert_eq!(bundle.hkdf_version, 1);
        assert_eq!(bundle.parser_version, 1);
        assert_eq!(bundle.path_normalization_version, 1);
        assert_eq!(bundle.hash_algorithm_version, 1);
        assert!(bundle.codegraph_version.is_none());
    }

    #[test]
    fn test_algorithm_version_bundle_compatibility_current() {
        // Current bundle should always be compatible with itself
        let bundle = AlgorithmVersionBundle::current();
        assert!(bundle.check_runtime_compatibility().is_ok());
    }

    #[test]
    fn test_algorithm_version_bundle_hkdf_mismatch() {
        let mut bundle = AlgorithmVersionBundle::current();
        bundle.hkdf_version = 1; // Simulate old version
        let result = bundle.check_runtime_compatibility();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.component, "hkdf");
        assert_eq!(err.severity, IncompatibilitySeverity::Breaking);
    }

    #[test]
    fn test_algorithm_version_bundle_path_normalization_mismatch() {
        let mut bundle = AlgorithmVersionBundle::current();
        bundle.path_normalization_version = 1; // Simulate old version
        let result = bundle.check_runtime_compatibility();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.component, "path_normalization");
    }

    #[test]
    fn test_algorithm_version_bundle_hash_mismatch() {
        let mut bundle = AlgorithmVersionBundle::current();
        bundle.hash_algorithm_version = 1; // Simulate old version
        let result = bundle.check_runtime_compatibility();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.component, "hash_algorithm");
    }

    #[test]
    fn test_algorithm_version_bundle_parser_mismatch_is_warning() {
        let mut bundle = AlgorithmVersionBundle::current();
        bundle.parser_version = 1; // Simulate old version

        // Should still pass compatibility check (parser is not breaking)
        assert!(bundle.check_runtime_compatibility().is_ok());

        // But should produce a warning
        let (compatible, warnings) = bundle.check_with_warnings();
        assert!(compatible);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Parser version mismatch"));
    }

    #[test]
    fn test_algorithm_version_bundle_serialization() {
        let bundle = AlgorithmVersionBundle::current();
        let json = serde_json::to_string(&bundle).expect("should serialize");
        let deserialized: AlgorithmVersionBundle =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(bundle, deserialized);
    }

    #[test]
    fn test_version_incompatibility_display() {
        let err = VersionIncompatibility {
            component: "hkdf".to_string(),
            stored: 1,
            current: 2,
            severity: IncompatibilitySeverity::Breaking,
            reason: "test reason".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("hkdf"));
        assert!(display.contains("v1"));
        assert!(display.contains("v2"));
        assert!(display.contains("test reason"));
    }

    // =========================================================================
    // Const Parse Tests
    // =========================================================================

    #[test]
    fn test_const_parse_u32_valid() {
        assert_eq!(const_parse_u32("0"), Some(0));
        assert_eq!(const_parse_u32("1"), Some(1));
        assert_eq!(const_parse_u32("297"), Some(297));
        assert_eq!(const_parse_u32("1234"), Some(1234));
        assert_eq!(const_parse_u32("4294967295"), Some(u32::MAX));
    }

    #[test]
    fn test_const_parse_u32_invalid() {
        assert_eq!(const_parse_u32(""), None);
        assert_eq!(const_parse_u32("abc"), None);
        assert_eq!(const_parse_u32("12a34"), None);
        assert_eq!(const_parse_u32("-123"), None);
        assert_eq!(const_parse_u32("1.23"), None);
    }

    #[test]
    fn test_const_parse_u32_overflow() {
        // u32::MAX + 1
        assert_eq!(const_parse_u32("4294967296"), None);
        // Much larger number
        assert_eq!(const_parse_u32("999999999999999"), None);
    }
}
