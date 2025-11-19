//! Version Information Module
//!
//! This module provides a single source of truth for all AdapterOS version information.
//! All version numbers should be read from this module to ensure consistency across
//! the codebase.
//!
//! # Version Strategy
//!
//! AdapterOS uses Semantic Versioning (SemVer) with an optional phase suffix:
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
//! - **Product Version**: Overall AdapterOS release version
//! - **API Schema Version**: REST API compatibility version
//! - **Database Schema Version**: Migration sequence number
//! - **Build Metadata**: Git commit, build timestamp, etc.

use std::fmt;

/// Current AdapterOS release version
///
/// This is the primary version number for the entire AdapterOS product.
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
/// Updated automatically when new migrations are added.
/// Check against `_sqlx_migrations` table for current database version.
pub const DATABASE_SCHEMA_VERSION: u32 = 71;

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

/// Build timestamp in ISO 8601 format
///
/// Set via `BUILD_TIMESTAMP` environment variable during build.
/// Falls back to "unknown" if not available.
pub const BUILD_TIMESTAMP: &str = match option_env!("BUILD_TIMESTAMP") {
    Some(ts) => ts,
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
    /// Build timestamp (ISO 8601)
    pub build_timestamp: String,
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
    /// println!("AdapterOS v{}", info.release);
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
            build_profile: BUILD_PROFILE.to_string(),
            target_arch: TARGET_ARCH.to_string(),
            target_os: TARGET_OS.to_string(),
        }
    }

    /// Get short version string for display (e.g., "1.0.0-alpha")
    pub fn short(&self) -> &str {
        &self.release
    }

    /// Get full version string with git commit (e.g., "1.0.0-alpha (abc1234)")
    pub fn full(&self) -> String {
        if self.git_commit == "unknown" {
            self.release.clone()
        } else {
            format!("{} ({})", self.release, &self.git_commit[..7.min(self.git_commit.len())])
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
        writeln!(f, "AdapterOS {}", self.release)?;
        writeln!(f, "API Schema:      v{}", self.api_schema)?;
        writeln!(f, "Database Schema: v{}", self.database_schema)?;
        writeln!(f, "RNG Module:      v{}", self.rng_module)?;
        writeln!(f, "Git Commit:      {}", self.git_commit)?;
        writeln!(f, "Build Time:      {}", self.build_timestamp)?;
        writeln!(f, "Build Profile:   {}", self.build_profile)?;
        writeln!(f, "Target:          {}-{}", self.target_os, self.target_arch)?;
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
        assert!(!API_SCHEMA_VERSION.is_empty(), "API_SCHEMA_VERSION must not be empty");
        assert!(DATABASE_SCHEMA_VERSION > 0, "DATABASE_SCHEMA_VERSION must be > 0");
        assert!(!RNG_MODULE_VERSION.is_empty(), "RNG_MODULE_VERSION must not be empty");
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
        assert!(display.contains("AdapterOS"));
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
    fn test_database_version_verification() {
        let info = VersionInfo::current();

        // Matching version should succeed
        assert!(info.verify_database_version(DATABASE_SCHEMA_VERSION).is_ok());

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
}
