//! Version Range Matching Logic for CVE Integration
//!
//! Provides comprehensive version matching capabilities for CVE databases including:
//! - Semantic versioning (semver) ranges (^1.2.3, ~1.2.3, >=1.2.3 <2.0.0)
//! - Cargo-specific syntax (1.2.*, =1.2.3, >1.2.3,<2.0.0)
//! - OSV ecosystem ranges (ECOSYSTEM format)
//! - CPE version strings for NVD
//! - Fuzzy matching for patch levels

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use tracing::{debug, warn};

/// Represents a semantic version with major, minor, patch, and optional pre-release/build metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre_release: Option<String>,
    pub build: Option<String>,
}

impl Version {
    /// Parse a version string into a Version struct
    ///
    /// Supports formats:
    /// - "1.2.3"
    /// - "1.2.3-alpha.1"
    /// - "1.2.3+build.123"
    /// - "1.2.3-rc.1+build.123"
    /// - "v1.2.3" (with optional 'v' prefix)
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim_start_matches('v');

        // Split on '+' for build metadata
        let (version_part, build) = if let Some(idx) = s.find('+') {
            let (v, b) = s.split_at(idx);
            (v, Some(b[1..].to_string()))
        } else {
            (s, None)
        };

        // Split on '-' for pre-release
        let (numeric_part, pre_release) = if let Some(idx) = version_part.find('-') {
            let (n, p) = version_part.split_at(idx);
            (n, Some(p[1..].to_string()))
        } else {
            (version_part, None)
        };

        // Split numeric part on '.'
        let parts: Vec<&str> = numeric_part.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(AosError::Validation(format!(
                "Invalid version format: {}",
                s
            )));
        }

        let major = parts
            .get(0)
            .and_then(|p| p.parse::<u32>().ok())
            .ok_or_else(|| AosError::Validation(format!("Invalid major version in: {}", s)))?;

        let minor = parts
            .get(1)
            .and_then(|p| p.parse::<u32>().ok())
            .unwrap_or(0);

        let patch = parts
            .get(2)
            .and_then(|p| p.parse::<u32>().ok())
            .unwrap_or(0);

        Ok(Version {
            major,
            minor,
            patch,
            pre_release,
            build,
        })
    }

    /// Compare versions according to semver rules
    fn compare_core(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }

    /// Check if version is a wildcard version (e.g., 1.2.*)
    pub fn is_wildcard(&self) -> bool {
        // This is typically represented by having patch = u32::MAX in parsed format
        // But we track it separately in VersionRange variants
        false
    }

    /// Get version as tuple for easy comparison (major, minor, patch)
    pub fn as_tuple(&self) -> (u32, u32, u32) {
        (self.major, self.minor, self.patch)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.pre_release {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let core_cmp = self.compare_core(other);
        if core_cmp != Ordering::Equal {
            return core_cmp;
        }

        // Pre-release versions have lower precedence
        match (&self.pre_release, &other.pre_release) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

/// Represents a version range constraint for matching CVE-affected versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionRange {
    /// Exact version match (e.g., "=1.2.3")
    Exact(Version),

    /// Inclusive range (e.g., ">=1.0.0,<2.0.0")
    Range(Version, Version),

    /// Greater than or equal (e.g., ">=1.2.3")
    GreaterOrEqual(Version),

    /// Greater than (e.g., ">1.2.3")
    GreaterThan(Version),

    /// Less than or equal (e.g., "<=1.2.3")
    LessOrEqual(Version),

    /// Less than (e.g., "<1.2.3")
    LessThan(Version),

    /// Caret range - compatible with version (e.g., "^1.2.3" = ">=1.2.3,<2.0.0")
    Caret(Version),

    /// Tilde range - reasonably close to version (e.g., "~1.2.3" = ">=1.2.3,<1.3.0")
    Tilde(Version),

    /// Wildcard range (e.g., "1.2.*" = ">=1.2.0,<1.3.0")
    Wildcard(u32, u32),

    /// Any version
    Any,
}

impl VersionRange {
    /// Parse a version range from a string
    ///
    /// Supports:
    /// - "=1.2.3" or "1.2.3" (exact)
    /// - "^1.2.3" (caret)
    /// - "~1.2.3" (tilde)
    /// - "1.2.*" (wildcard)
    /// - ">1.2.3" (greater than)
    /// - ">=1.2.3" (greater or equal)
    /// - "<2.0.0" (less than)
    /// - "<=2.0.0" (less or equal)
    /// - ">=1.2.3,<2.0.0" (range)
    /// - "*" (any version)
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        if s == "*" {
            return Ok(VersionRange::Any);
        }

        // Handle comma-separated ranges (e.g., ">=1.0.0,<2.0.0")
        if s.contains(',') {
            return Self::parse_compound_range(s);
        }

        // Handle caret range (^1.2.3)
        if let Some(stripped) = s.strip_prefix('^') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::Caret(version));
        }

        // Handle tilde range (~1.2.3)
        if let Some(stripped) = s.strip_prefix('~') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::Tilde(version));
        }

        // Handle exact match (=1.2.3)
        if let Some(stripped) = s.strip_prefix('=') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::Exact(version));
        }

        // Handle greater than or equal (>=1.2.3)
        if let Some(stripped) = s.strip_prefix(">=") {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::GreaterOrEqual(version));
        }

        // Handle greater than (>1.2.3)
        if let Some(stripped) = s.strip_prefix('>') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::GreaterThan(version));
        }

        // Handle less than or equal (<=1.2.3)
        if let Some(stripped) = s.strip_prefix("<=") {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::LessOrEqual(version));
        }

        // Handle less than (<2.0.0)
        if let Some(stripped) = s.strip_prefix('<') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::LessThan(version));
        }

        // Handle wildcard (1.2.*)
        if s.ends_with(".*") {
            let parts: Vec<&str> = s.trim_end_matches(".*").split('.').collect();
            if parts.len() == 2 {
                let major = parts[0].parse::<u32>().map_err(|_| {
                    AosError::Validation(format!("Invalid wildcard version: {}", s))
                })?;
                let minor = parts[1].parse::<u32>().map_err(|_| {
                    AosError::Validation(format!("Invalid wildcard version: {}", s))
                })?;
                return Ok(VersionRange::Wildcard(major, minor));
            }
        }

        // Default to exact match (no prefix)
        let version = Version::parse(s)?;
        Ok(VersionRange::Exact(version))
    }

    /// Parse compound range like ">=1.0.0,<2.0.0"
    fn parse_compound_range(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
        if parts.len() != 2 {
            return Err(AosError::Validation(format!(
                "Invalid compound range: {}",
                s
            )));
        }

        let first = Self::parse(parts[0])?;
        let second = Self::parse(parts[1])?;

        // Extract min and max versions
        let (min, max) = match (first, second) {
            (VersionRange::GreaterOrEqual(min), VersionRange::LessThan(max)) => (min, max),
            (VersionRange::GreaterOrEqual(min), VersionRange::LessOrEqual(max)) => (min, max),
            (VersionRange::GreaterThan(min), VersionRange::LessThan(max)) => (min, max),
            (VersionRange::GreaterThan(min), VersionRange::LessOrEqual(max)) => (min, max),
            (VersionRange::LessThan(max), VersionRange::GreaterOrEqual(min)) => (min, max),
            (VersionRange::LessOrEqual(max), VersionRange::GreaterOrEqual(min)) => (min, max),
            (VersionRange::LessThan(max), VersionRange::GreaterThan(min)) => (min, max),
            (VersionRange::LessOrEqual(max), VersionRange::GreaterThan(min)) => (min, max),
            _ => {
                return Err(AosError::Validation(format!(
                    "Unsupported compound range: {}",
                    s
                )))
            }
        };

        Ok(VersionRange::Range(min, max))
    }

    /// Check if a version matches this range
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            VersionRange::Exact(v) => version == v,
            VersionRange::Range(min, max) => version >= min && version <= max,
            VersionRange::GreaterOrEqual(v) => version >= v,
            VersionRange::GreaterThan(v) => version > v,
            VersionRange::LessOrEqual(v) => version <= v,
            VersionRange::LessThan(v) => version < v,
            VersionRange::Caret(v) => {
                // ^1.2.3 means >=1.2.3, <2.0.0
                if v.major == 0 {
                    // ^0.2.3 means >=0.2.3, <0.3.0
                    version >= v && version.major == 0 && version.minor == v.minor
                } else {
                    version >= v && version.major == v.major
                }
            }
            VersionRange::Tilde(v) => {
                // ~1.2.3 means >=1.2.3, <1.3.0
                version >= v && version.major == v.major && version.minor == v.minor
            }
            VersionRange::Wildcard(major, minor) => {
                version.major == *major && version.minor == *minor
            }
            VersionRange::Any => true,
        }
    }

    /// Check if a version matches with fuzzy patch matching
    /// Allows matching versions with different patch levels if they're close
    pub fn matches_fuzzy(&self, version: &Version, patch_tolerance: u32) -> bool {
        match self {
            VersionRange::Exact(v) => {
                version.major == v.major
                    && version.minor == v.minor
                    && version.patch.saturating_sub(v.patch) <= patch_tolerance
                    && v.patch.saturating_sub(version.patch) <= patch_tolerance
            }
            _ => self.matches(version),
        }
    }

    /// Get the minimum version covered by this range
    pub fn min_version(&self) -> Option<&Version> {
        match self {
            VersionRange::Exact(v) => Some(v),
            VersionRange::Range(min, _) => Some(min),
            VersionRange::GreaterOrEqual(v) => Some(v),
            VersionRange::GreaterThan(v) => Some(v),
            VersionRange::Caret(v) => Some(v),
            VersionRange::Tilde(v) => Some(v),
            VersionRange::Wildcard(_, _) => None,
            VersionRange::Any => None,
            VersionRange::LessOrEqual(_) | VersionRange::LessThan(_) => None,
        }
    }

    /// Get the maximum version covered by this range
    pub fn max_version(&self) -> Option<&Version> {
        match self {
            VersionRange::Exact(v) => Some(v),
            VersionRange::Range(_, max) => Some(max),
            VersionRange::LessOrEqual(v) => Some(v),
            VersionRange::LessThan(v) => Some(v),
            VersionRange::Any => None,
            VersionRange::GreaterOrEqual(_)
            | VersionRange::GreaterThan(_)
            | VersionRange::Caret(_)
            | VersionRange::Tilde(_)
            | VersionRange::Wildcard(_, _) => None,
        }
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionRange::Exact(v) => write!(f, "={}", v),
            VersionRange::Range(min, max) => write!(f, ">={},<{}", min, max),
            VersionRange::GreaterOrEqual(v) => write!(f, ">={}", v),
            VersionRange::GreaterThan(v) => write!(f, ">{}", v),
            VersionRange::LessOrEqual(v) => write!(f, "<={}", v),
            VersionRange::LessThan(v) => write!(f, "<{}", v),
            VersionRange::Caret(v) => write!(f, "^{}", v),
            VersionRange::Tilde(v) => write!(f, "~{}", v),
            VersionRange::Wildcard(major, minor) => write!(f, "{}.{}.*", major, minor),
            VersionRange::Any => write!(f, "*"),
        }
    }
}

/// OSV ecosystem-specific version range handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvVersionRange {
    /// The affected version string from OSV
    pub affected: String,
    /// OSV ecosystem (e.g., "npm", "pypi", "crates.io")
    pub ecosystem: String,
    /// Parsed version constraint
    pub constraint: VersionRange,
}

impl OsvVersionRange {
    /// Parse an OSV affected version string
    ///
    /// OSV uses ecosystem-specific version syntax:
    /// - npm: semver ranges (^, ~, ranges)
    /// - pypi: PEP 440 ranges (>, >=, <, <=, ==, !=)
    /// - crates.io: cargo semver (same as npm)
    pub fn parse(affected: &str, ecosystem: &str) -> Result<Self> {
        let constraint = match ecosystem {
            "npm" | "crates.io" | "cargo" => VersionRange::parse(affected)?,
            "pypi" => Self::parse_pep440(affected)?,
            "nuget" => Self::parse_nuget(affected)?,
            "maven" => Self::parse_maven(affected)?,
            _ => {
                // Fallback to generic parsing
                debug!(
                    "Unknown OSV ecosystem: {}, attempting generic parse",
                    ecosystem
                );
                VersionRange::parse(affected)?
            }
        };

        Ok(OsvVersionRange {
            affected: affected.to_string(),
            ecosystem: ecosystem.to_string(),
            constraint,
        })
    }

    /// Parse PEP 440 version specifiers
    fn parse_pep440(s: &str) -> Result<VersionRange> {
        let s = s.trim();

        // Handle comma-separated specifiers
        if s.contains(',') {
            return VersionRange::parse_compound_range(s);
        }

        // PEP 440 uses similar syntax to semver
        if let Some(stripped) = s.strip_prefix("==") {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::Exact(version));
        }

        if let Some(stripped) = s.strip_prefix(">=") {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::GreaterOrEqual(version));
        }

        if let Some(stripped) = s.strip_prefix('>') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::GreaterThan(version));
        }

        if let Some(stripped) = s.strip_prefix("<=") {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::LessOrEqual(version));
        }

        if let Some(stripped) = s.strip_prefix('<') {
            let version = Version::parse(stripped)?;
            return Ok(VersionRange::LessThan(version));
        }

        if let Some(_stripped) = s.strip_prefix("!=") {
            // Not equal not directly supported, treat conservatively
            warn!("PEP 440 '!=' specifier not directly supported: {}", s);
            return Err(AosError::Validation(format!(
                "Unsupported PEP 440 specifier: {}",
                s
            )));
        }

        // Default
        let version = Version::parse(s)?;
        Ok(VersionRange::Exact(version))
    }

    /// Parse NuGet version syntax
    fn parse_nuget(s: &str) -> Result<VersionRange> {
        // NuGet uses similar syntax but with some extensions
        // For now, parse as generic semver
        VersionRange::parse(s)
    }

    /// Parse Maven version syntax
    fn parse_maven(s: &str) -> Result<VersionRange> {
        // Maven uses different syntax: [1.0.0] (exact), (1.0.0,2.0.0) (range)
        let s = s.trim();

        if s.starts_with('[') && s.ends_with(']') {
            let inner = s.trim_start_matches('[').trim_end_matches(']');
            if inner.contains(',') {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() == 2 {
                    let min = Version::parse(parts[0].trim())?;
                    let max = Version::parse(parts[1].trim())?;
                    return Ok(VersionRange::Range(min, max));
                }
            } else {
                let version = Version::parse(inner)?;
                return Ok(VersionRange::Exact(version));
            }
        }

        if s.starts_with('(') && s.ends_with(')') {
            let inner = s.trim_start_matches('(').trim_end_matches(')');
            if inner.contains(',') {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() == 2 {
                    let min = Version::parse(parts[0].trim())?;
                    let max = Version::parse(parts[1].trim())?;
                    return Ok(VersionRange::Range(min, max));
                }
            }
        }

        // Fallback
        VersionRange::parse(s)
    }

    /// Check if version matches this OSV range
    pub fn matches(&self, version: &Version) -> bool {
        self.constraint.matches(version)
    }
}

/// CPE version string parser for NVD compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpeVersionMatcher {
    /// CPE part (e.g., "a" for application, "o" for OS, "h" for hardware)
    pub part: String,
    /// Vendor
    pub vendor: String,
    /// Product
    pub product: String,
    /// Version range constraint
    pub version_constraint: VersionRange,
}

impl CpeVersionMatcher {
    /// Parse a CPE string (simplified format without URI encoding)
    ///
    /// Format: "part:vendor:product:version_range"
    /// Example: "a:apache:log4j:>=1.0.0,<2.0.0"
    pub fn parse(cpe: &str) -> Result<Self> {
        let parts: Vec<&str> = cpe.split(':').collect();
        if parts.len() < 4 {
            return Err(AosError::Validation(format!("Invalid CPE format: {}", cpe)));
        }

        let part = parts[0].to_string();
        let vendor = parts[1].to_string();
        let product = parts[2].to_string();
        let version_range = parts[3..].join(":");

        let version_constraint = VersionRange::parse(&version_range)?;

        Ok(CpeVersionMatcher {
            part,
            vendor,
            product,
            version_constraint,
        })
    }

    /// Check if a product version matches this CPE
    pub fn matches(&self, version: &Version) -> bool {
        self.version_constraint.matches(version)
    }

    /// Check if a version string matches (parses the string first)
    pub fn matches_string(&self, version_str: &str) -> Result<bool> {
        let version = Version::parse(version_str)?;
        Ok(self.matches(&version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse_basic() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre_release.is_none());
        assert!(v.build.is_none());
    }

    #[test]
    fn test_version_parse_with_v_prefix() {
        let v = Version::parse("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_version_parse_prerelease() {
        let v = Version::parse("1.2.3-alpha.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre_release, Some("alpha.1".to_string()));
    }

    #[test]
    fn test_version_parse_with_build() {
        let v = Version::parse("1.2.3+build.123").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.build, Some("build.123".to_string()));
    }

    #[test]
    fn test_version_parse_full() {
        let v = Version::parse("1.2.3-rc.1+build.123").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre_release, Some("rc.1".to_string()));
        assert_eq!(v.build, Some("build.123".to_string()));
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }

    #[test]
    fn test_version_prerelease_ordering() {
        let stable = Version::parse("1.2.3").unwrap();
        let prerelease = Version::parse("1.2.3-alpha").unwrap();
        assert!(prerelease < stable);
    }

    #[test]
    fn test_version_range_exact() {
        let range = VersionRange::parse("=1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();

        assert!(range.matches(&v1));
        assert!(!range.matches(&v2));
    }

    #[test]
    fn test_version_range_exact_implicit() {
        let range = VersionRange::parse("1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();

        assert!(range.matches(&v1));
        assert!(!range.matches(&v2));
    }

    #[test]
    fn test_version_range_caret() {
        let range = VersionRange::parse("^1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();
        let v3 = Version::parse("1.5.0").unwrap();
        let v4 = Version::parse("2.0.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(range.matches(&v3));
        assert!(!range.matches(&v4));
    }

    #[test]
    fn test_version_range_caret_zero_major() {
        let range = VersionRange::parse("^0.2.3").unwrap();
        let v1 = Version::parse("0.2.3").unwrap();
        let v2 = Version::parse("0.2.4").unwrap();
        let v3 = Version::parse("0.3.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_version_range_tilde() {
        let range = VersionRange::parse("~1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();
        let v3 = Version::parse("1.3.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_version_range_greater_or_equal() {
        let range = VersionRange::parse(">=1.2.3").unwrap();
        let v1 = Version::parse("1.2.2").unwrap();
        let v2 = Version::parse("1.2.3").unwrap();
        let v3 = Version::parse("1.2.4").unwrap();

        assert!(!range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(range.matches(&v3));
    }

    #[test]
    fn test_version_range_greater_than() {
        let range = VersionRange::parse(">1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();

        assert!(!range.matches(&v1));
        assert!(range.matches(&v2));
    }

    #[test]
    fn test_version_range_less_than() {
        let range = VersionRange::parse("<2.0.0").unwrap();
        let v1 = Version::parse("1.9.9").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(range.matches(&v1));
        assert!(!range.matches(&v2));
    }

    #[test]
    fn test_version_range_less_or_equal() {
        let range = VersionRange::parse("<=2.0.0").unwrap();
        let v1 = Version::parse("1.9.9").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("2.0.1").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_version_range_wildcard() {
        let range = VersionRange::parse("1.2.*").unwrap();
        let v1 = Version::parse("1.2.0").unwrap();
        let v2 = Version::parse("1.2.99").unwrap();
        let v3 = Version::parse("1.3.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_version_range_compound() {
        let range = VersionRange::parse(">=1.0.0,<2.0.0").unwrap();
        let v1 = Version::parse("0.9.9").unwrap();
        let v2 = Version::parse("1.0.0").unwrap();
        let v3 = Version::parse("1.5.0").unwrap();
        let v4 = Version::parse("2.0.0").unwrap();

        assert!(!range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(range.matches(&v3));
        assert!(!range.matches(&v4));
    }

    #[test]
    fn test_version_range_any() {
        let range = VersionRange::parse("*").unwrap();
        let v1 = Version::parse("0.0.1").unwrap();
        let v2 = Version::parse("1.2.3").unwrap();
        let v3 = Version::parse("999.999.999").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(range.matches(&v3));
    }

    #[test]
    fn test_version_fuzzy_matching() {
        let range = VersionRange::parse("=1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();
        let v3 = Version::parse("1.2.5").unwrap();

        assert!(range.matches_fuzzy(&v1, 0));
        assert!(range.matches_fuzzy(&v2, 1));
        assert!(!range.matches_fuzzy(&v2, 0));
        assert!(range.matches_fuzzy(&v3, 2));
        assert!(!range.matches_fuzzy(&v3, 1));
    }

    #[test]
    fn test_osv_version_range_npm() {
        let osv = OsvVersionRange::parse("^1.2.3", "npm").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.5.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();

        assert!(osv.matches(&v1));
        assert!(osv.matches(&v2));
        assert!(!osv.matches(&v3));
    }

    #[test]
    fn test_osv_version_range_crates() {
        let osv = OsvVersionRange::parse(">=1.0.0,<2.0.0", "crates.io").unwrap();
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.5.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();

        assert!(osv.matches(&v1));
        assert!(osv.matches(&v2));
        assert!(!osv.matches(&v3));
    }

    #[test]
    fn test_osv_version_range_pypi() {
        let osv = OsvVersionRange::parse(">=1.0.0,<2.0.0", "pypi").unwrap();
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.5.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();

        assert!(osv.matches(&v1));
        assert!(osv.matches(&v2));
        assert!(!osv.matches(&v3));
    }

    #[test]
    fn test_cpe_version_matcher_parse() {
        let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0").unwrap();
        assert_eq!(cpe.part, "a");
        assert_eq!(cpe.vendor, "apache");
        assert_eq!(cpe.product, "log4j");
    }

    #[test]
    fn test_cpe_version_matcher_matches() {
        let cpe = CpeVersionMatcher::parse("a:apache:log4j:>=1.0.0,<2.0.0").unwrap();
        assert!(cpe.matches_string("1.0.0").unwrap());
        assert!(cpe.matches_string("1.5.0").unwrap());
        assert!(!cpe.matches_string("2.0.0").unwrap());
    }

    #[test]
    fn test_version_range_display() {
        assert_eq!(
            format!("{}", VersionRange::parse("=1.2.3").unwrap()),
            "=1.2.3"
        );
        assert_eq!(
            format!("{}", VersionRange::parse("^1.2.3").unwrap()),
            "^1.2.3"
        );
        assert_eq!(
            format!("{}", VersionRange::parse("~1.2.3").unwrap()),
            "~1.2.3"
        );
        assert_eq!(
            format!("{}", VersionRange::parse("1.2.*").unwrap()),
            "1.2.*"
        );
        assert_eq!(format!("{}", VersionRange::parse("*").unwrap()), "*");
    }

    #[test]
    fn test_real_world_log4j_cve() {
        // CVE-2021-44228: Apache Log4j 2.0-beta9 through 2.15.0
        let range = VersionRange::parse(">=2.0.0,<2.16.0").unwrap();
        let vulnerable_versions = vec!["2.0.0", "2.8.1", "2.13.0", "2.14.0", "2.15.0"];
        let safe_versions = vec!["1.2.17", "2.16.0", "2.16.1", "3.0.0"];

        for v_str in vulnerable_versions {
            let v = Version::parse(v_str).unwrap();
            assert!(range.matches(&v), "Expected {} to be vulnerable", v_str);
        }

        for v_str in safe_versions {
            let v = Version::parse(v_str).unwrap();
            assert!(!range.matches(&v), "Expected {} to be safe", v_str);
        }
    }

    #[test]
    fn test_real_world_spring_rce() {
        // CVE-2022-22965: Spring Framework >=3.2.0, <5.2.25, >=5.3.0, <5.3.14
        let range1 = VersionRange::parse(">=3.2.0,<5.2.25").unwrap();
        let range2 = VersionRange::parse(">=5.3.0,<5.3.14").unwrap();

        let v1 = Version::parse("3.2.0").unwrap();
        let v2 = Version::parse("5.2.24").unwrap();
        let v3 = Version::parse("5.3.0").unwrap();
        let v4 = Version::parse("5.3.13").unwrap();
        let v5 = Version::parse("5.2.25").unwrap();
        let v6 = Version::parse("5.3.14").unwrap();

        assert!(range1.matches(&v1));
        assert!(range1.matches(&v2));
        assert!(!range1.matches(&v5));

        assert!(range2.matches(&v3));
        assert!(range2.matches(&v4));
        assert!(!range2.matches(&v6));
    }

    #[test]
    fn test_real_world_nodejs_regex_dos() {
        // CVE-2023-38545: curl <8.0.0
        let range = VersionRange::parse("<8.0.0").unwrap();
        let vulnerable = Version::parse("7.99.9").unwrap();
        let safe = Version::parse("8.0.0").unwrap();

        assert!(range.matches(&vulnerable));
        assert!(!range.matches(&safe));
    }

    #[test]
    fn test_cargo_syntax_exact() {
        let range = VersionRange::parse("1.2.3").unwrap();
        let v = Version::parse("1.2.3").unwrap();
        assert!(range.matches(&v));
    }

    #[test]
    fn test_cargo_syntax_caret() {
        let range = VersionRange::parse("^1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.9.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_cargo_syntax_tilde() {
        let range = VersionRange::parse("~1.2.3").unwrap();
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.99").unwrap();
        let v3 = Version::parse("1.3.0").unwrap();

        assert!(range.matches(&v1));
        assert!(range.matches(&v2));
        assert!(!range.matches(&v3));
    }

    #[test]
    fn test_version_range_min_max() {
        let exact = VersionRange::parse("=1.2.3").unwrap();
        assert_eq!(
            exact.min_version().map(|v| v.to_string()),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            exact.max_version().map(|v| v.to_string()),
            Some("1.2.3".to_string())
        );

        let range = VersionRange::parse(">=1.0.0,<2.0.0").unwrap();
        assert_eq!(
            range.min_version().map(|v| v.to_string()),
            Some("1.0.0".to_string())
        );
        assert_eq!(
            range.max_version().map(|v| v.to_string()),
            Some("2.0.0".to_string())
        );

        let any = VersionRange::parse("*").unwrap();
        assert_eq!(any.min_version(), None);
        assert_eq!(any.max_version(), None);
    }
}
