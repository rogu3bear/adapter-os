//! Path traversal protection
//!
//! Implements path traversal protection to prevent directory traversal attacks.

use adapteros_core::{AosError, Result};
use std::path::{Component, Path, PathBuf};
use tracing::debug;

/// Path traversal protection configuration
#[derive(Debug, Clone)]
pub struct TraversalProtection {
    /// Enable traversal protection
    pub enabled: bool,
    /// Maximum path depth
    pub max_depth: u32,
    /// Blocked components
    pub blocked_components: Vec<String>,
    /// Allowed components
    pub allowed_components: Vec<String>,
}

impl Default for TraversalProtection {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 20,
            blocked_components: vec![
                "..".to_string(),
                "~".to_string(),
                "$HOME".to_string(),
                "$USER".to_string(),
            ],
            allowed_components: vec![],
        }
    }
}

/// Check if a path is safe from traversal attacks
pub fn check_path_traversal(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let protection = TraversalProtection::default();

    if !protection.enabled {
        return Ok(());
    }

    // Check path components
    let components: Vec<Component> = path.components().collect();

    // Check path depth
    if components.len() > protection.max_depth as usize {
        return Err(AosError::Security(format!(
            "Path depth {} exceeds maximum {}",
            components.len(),
            protection.max_depth
        )));
    }

    // Check for blocked components
    for component in &components {
        match component {
            Component::ParentDir => {
                return Err(AosError::Security(
                    "Parent directory traversal detected".to_string(),
                ));
            }
            Component::Normal(name) => {
                let name_str = name.to_string_lossy().to_string();

                // Check blocked components
                if protection.blocked_components.contains(&name_str) {
                    return Err(AosError::Security(format!(
                        "Blocked component detected: {}",
                        name_str
                    )));
                }

                // Check allowed components (if specified)
                if !protection.allowed_components.is_empty()
                    && !protection.allowed_components.contains(&name_str)
                {
                    return Err(AosError::Security(format!(
                        "Component not allowed: {}",
                        name_str
                    )));
                }
            }
            Component::RootDir => {
                // Root directory is generally safe
            }
            Component::CurDir => {
                // Current directory is generally safe
            }
            Component::Prefix(_) => {
                // Windows prefix - generally safe
            }
        }
    }

    // Check for suspicious patterns
    check_suspicious_patterns(path)?;

    debug!("Path traversal check passed for: {}", path.display());
    Ok(())
}

/// Check for suspicious patterns in path
fn check_suspicious_patterns(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy().to_string();

    // Check for common traversal patterns
    let suspicious_patterns = vec![
        "../",
        "..\\",
        "..%2f",
        "..%5c",
        "..%252f",
        "..%255c",
        "....//",
        "....\\\\",
        "%2e%2e%2f",
        "%2e%2e%5c",
        "..%c0%af",
        "..%c1%9c",
    ];

    for pattern in suspicious_patterns {
        if path_str.contains(pattern) {
            return Err(AosError::Security(format!(
                "Suspicious pattern detected: {}",
                pattern
            )));
        }
    }

    // Check for absolute paths in relative context
    if path.is_absolute() {
        return Err(AosError::Security(
            "Absolute path not allowed in relative context".to_string(),
        ));
    }

    Ok(())
}

/// Normalize a path safely
pub fn normalize_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();

    // Check for traversal attacks first
    check_path_traversal(path)?;

    // Normalize the path
    let normalized = path
        .canonicalize()
        .map_err(|e| AosError::Security(format!("Failed to canonicalize path: {}", e)))?;

    Ok(normalized)
}

/// Join paths safely
pub fn join_paths_safe(base: impl AsRef<Path>, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let base = base.as_ref();
    let relative = relative.as_ref();

    // Check relative path for traversal
    check_path_traversal(relative)?;

    // Join paths
    let joined = base.join(relative);

    // Check the result
    check_path_traversal(&joined)?;

    Ok(joined)
}

/// Get relative path safely
pub fn get_relative_path_safe(base: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<PathBuf> {
    let base = base.as_ref();
    let target = target.as_ref();

    // Check both paths
    check_path_traversal(base)?;
    check_path_traversal(target)?;

    // Get relative path
    let relative = target
        .strip_prefix(base)
        .map_err(|e| AosError::Security(format!("Failed to get relative path: {}", e)))?;

    // Check the result
    check_path_traversal(relative)?;

    Ok(relative.to_path_buf())
}

/// Check if path is within base directory
pub fn is_path_within_base(path: impl AsRef<Path>, base: impl AsRef<Path>) -> Result<bool> {
    let path = path.as_ref();
    let base = base.as_ref();

    // Check both paths
    check_path_traversal(path)?;
    check_path_traversal(base)?;

    // Check if path is within base
    let is_within = path.starts_with(base);

    Ok(is_within)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_path_traversal_protection() -> Result<()> {
        // Test normal path
        check_path_traversal("test/file.txt")?;

        // Test parent directory traversal
        let result = check_path_traversal("../test/file.txt");
        assert!(result.is_err());

        // Test suspicious patterns
        let result = check_path_traversal("test/..%2f/etc/passwd");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_path_normalization() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let normalized = normalize_path(&test_file)?;
        assert!(normalized.exists());

        Ok(())
    }

    #[test]
    fn test_path_joining() -> Result<()> {
        let base = PathBuf::from("/safe/base");
        let relative = PathBuf::from("test/file.txt");

        let joined = join_paths_safe(&base, &relative)?;
        assert_eq!(joined, PathBuf::from("/safe/base/test/file.txt"));

        // Test traversal attempt
        let result = join_paths_safe(&base, "../etc/passwd");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_path_within_base() -> Result<()> {
        let base = PathBuf::from("/safe/base");

        assert!(is_path_within_base("/safe/base/test.txt", &base)?);
        assert!(!is_path_within_base("/safe/other/test.txt", &base)?);

        Ok(())
    }
}
