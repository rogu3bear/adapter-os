//! Symlink protection
//!
//! Implements symlink protection to prevent directory traversal attacks.

use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Symlink protection configuration
#[derive(Debug, Clone)]
pub struct SymlinkProtection {
    /// Enable symlink protection
    pub enabled: bool,
    /// Maximum symlink depth
    pub max_depth: u32,
    /// Allowed symlink targets
    pub allowed_targets: Vec<PathBuf>,
    /// Blocked symlink targets
    pub blocked_targets: Vec<PathBuf>,
}

impl Default for SymlinkProtection {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 5,
            allowed_targets: vec![],
            blocked_targets: vec![
                PathBuf::from("/etc"),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/var"),
                PathBuf::from("/tmp"),
                PathBuf::from("/root"),
                PathBuf::from("/home"),
            ],
        }
    }
}

/// Check if a path is safe from symlink attacks
pub fn check_symlink_safety(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let protection = SymlinkProtection::default();

    if !protection.enabled {
        return Ok(());
    }

    // Check if path contains symlinks
    let symlink_chain = resolve_symlink_chain(path)?;

    // Check symlink depth
    if symlink_chain.len() > protection.max_depth as usize {
        return Err(AosError::Security(format!(
            "Symlink chain depth {} exceeds maximum {}",
            symlink_chain.len(),
            protection.max_depth
        )));
    }

    // Check if any symlink target is blocked
    for symlink_path in &symlink_chain {
        if let Ok(target) = std::fs::read_link(symlink_path) {
            if is_target_blocked(&target, &protection)? {
                return Err(AosError::Security(format!(
                    "Symlink target {} is blocked by security policy",
                    target.display()
                )));
            }
        }
    }

    debug!("Symlink safety check passed for: {}", path.display());
    Ok(())
}

/// Resolve symlink chain for a path
fn resolve_symlink_chain(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let mut chain = Vec::new();
    let mut current_path = path.to_path_buf();
    let mut visited = std::collections::HashSet::new();

    // Follow symlinks up to maximum depth
    for _ in 0..10 {
        // Prevent infinite loops
        if visited.contains(&current_path) {
            return Err(AosError::Security("Circular symlink detected".to_string()));
        }
        visited.insert(current_path.clone());

        if current_path.is_symlink() {
            chain.push(current_path.clone());
            let target = std::fs::read_link(&current_path)
                .map_err(|e| AosError::Security(format!("Failed to read symlink: {}", e)))?;

            if target.is_absolute() {
                current_path = target;
            } else {
                current_path = current_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(target);
            }
        } else {
            break;
        }
    }

    Ok(chain)
}

/// Check if a symlink target is blocked
fn is_target_blocked(target: &Path, protection: &SymlinkProtection) -> Result<bool> {
    // Check blocked targets
    for blocked_target in &protection.blocked_targets {
        if target.starts_with(blocked_target) {
            return Ok(true);
        }
    }

    // Check allowed targets (if specified)
    if !protection.allowed_targets.is_empty() {
        let mut matches_allowed = false;
        for allowed_target in &protection.allowed_targets {
            if target.starts_with(allowed_target) {
                matches_allowed = true;
                break;
            }
        }

        if !matches_allowed {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if a path is a symlink
pub fn is_symlink(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_symlink()
}

/// Get symlink target
pub fn get_symlink_target(path: impl AsRef<Path>) -> Result<PathBuf> {
    std::fs::read_link(path)
        .map_err(|e| AosError::Security(format!("Failed to read symlink: {}", e)))
}

/// Create a safe symlink
pub fn create_safe_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    let target = target.as_ref();
    let link = link.as_ref();

    // Check if target is safe
    check_symlink_safety(target)?;

    // Create the symlink
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| AosError::Security(format!("Failed to create symlink: {}", e)))?;

    debug!(
        "Created safe symlink: {} -> {}",
        link.display(),
        target.display()
    );
    Ok(())
}

/// Remove a symlink safely
pub fn remove_safe_symlink(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if !path.is_symlink() {
        return Err(AosError::Security("Path is not a symlink".to_string()));
    }

    std::fs::remove_file(path)
        .map_err(|e| AosError::Security(format!("Failed to remove symlink: {}", e)))?;

    debug!("Removed symlink: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_symlink_safety() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        // Test normal file
        check_symlink_safety(&test_file)?;

        // Test symlink
        let symlink_path = temp_dir.path().join("test_link");
        std::os::unix::fs::symlink(&test_file, &symlink_path)?;
        check_symlink_safety(&symlink_path)?;

        Ok(())
    }

    #[test]
    fn test_blocked_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let blocked_target = PathBuf::from("/etc/passwd");
        let symlink_path = temp_dir.path().join("blocked_link");

        // This should fail because /etc is blocked
        let result = std::os::unix::fs::symlink(&blocked_target, &symlink_path);
        if result.is_ok() {
            let check_result = check_symlink_safety(&symlink_path);
            assert!(check_result.is_err());
        }
    }
}
