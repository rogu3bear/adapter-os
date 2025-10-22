//! Capability-based access control
//!
//! Implements capability-based access control for secure filesystem operations.

use adapteros_core::{AosError, Result};
use cap_std::fs::{Dir, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path as StdPath;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// Capability-based access control
pub struct Capabilities {
    /// Root directory capability
    root_dir: Option<Dir>,
    /// Directory capabilities
    dir_caps: HashMap<PathBuf, Dir>,
    /// File capabilities
    file_caps: HashMap<PathBuf, File>,
    /// Access control list
    acl: AccessControlList,
}

/// Access control list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlList {
    /// Allowed operations
    pub allowed_operations: Vec<Operation>,
    /// Denied operations
    pub denied_operations: Vec<Operation>,
    /// Allowed paths
    pub allowed_paths: Vec<PathPattern>,
    /// Denied paths
    pub denied_paths: Vec<PathPattern>,
}

/// File system operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Execute operation
    Execute,
    /// Delete operation
    Delete,
    /// Create operation
    Create,
    /// List operation
    List,
    /// Stat operation
    Stat,
}

/// Path pattern for access control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPattern {
    /// Pattern string
    pub pattern: String,
    /// Pattern type
    pub pattern_type: PatternType,
    /// Recursive flag
    pub recursive: bool,
}

/// Pattern type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    /// Exact match
    Exact,
    /// Glob pattern
    Glob,
    /// Regex pattern
    Regex,
}

impl Capabilities {
    /// Create new capabilities
    pub fn new() -> Result<Self> {
        Ok(Self {
            root_dir: None,
            dir_caps: HashMap::new(),
            file_caps: HashMap::new(),
            acl: AccessControlList::default(),
        })
    }

    /// Set root directory capability
    pub fn set_root_dir(&mut self, path: impl AsRef<StdPath>) -> Result<()> {
        let root_dir = Dir::open_ambient_dir(path, cap_std::ambient_authority())
            .map_err(|e| AosError::Security(format!("Failed to open root directory: {}", e)))?;

        self.root_dir = Some(root_dir);
        info!("Set root directory capability");
        Ok(())
    }

    /// Grant directory capability
    pub fn grant_dir_capability(&mut self, path: PathBuf, dir: Dir) -> Result<()> {
        self.dir_caps.insert(path.clone(), dir);
        debug!("Granted directory capability for: {}", path.display());
        Ok(())
    }

    /// Grant file capability
    pub fn grant_file_capability(&mut self, path: PathBuf, file: File) -> Result<()> {
        self.file_caps.insert(path.clone(), file);
        debug!("Granted file capability for: {}", path.display());
        Ok(())
    }

    /// Revoke directory capability
    pub fn revoke_dir_capability(&mut self, path: &PathBuf) -> Result<()> {
        if self.dir_caps.remove(path).is_some() {
            debug!("Revoked directory capability for: {}", path.display());
        }
        Ok(())
    }

    /// Revoke file capability
    pub fn revoke_file_capability(&mut self, path: &PathBuf) -> Result<()> {
        if self.file_caps.remove(path).is_some() {
            debug!("Revoked file capability for: {}", path.display());
        }
        Ok(())
    }

    /// Check if operation is allowed
    pub fn is_operation_allowed(&self, operation: &Operation, path: &PathBuf) -> Result<bool> {
        // Check denied operations first
        if self.acl.denied_operations.contains(operation) {
            return Ok(false);
        }

        // Check allowed operations
        if !self.acl.allowed_operations.is_empty()
            && !self.acl.allowed_operations.contains(operation)
        {
            return Ok(false);
        }

        // Check path patterns
        let path_str = path.to_string_lossy().to_string();

        // Check denied paths
        for pattern in &self.acl.denied_paths {
            if self.matches_pattern(&path_str, pattern)? {
                return Ok(false);
            }
        }

        // Check allowed paths (if specified)
        if !self.acl.allowed_paths.is_empty() {
            let mut matches_allowed = false;
            for pattern in &self.acl.allowed_paths {
                if self.matches_pattern(&path_str, pattern)? {
                    matches_allowed = true;
                    break;
                }
            }

            if !matches_allowed {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get directory capability
    pub fn get_dir_capability(&self, path: &PathBuf) -> Option<&Dir> {
        self.dir_caps.get(path)
    }

    /// Get file capability
    pub fn get_file_capability(&self, path: &PathBuf) -> Option<&File> {
        self.file_caps.get(path)
    }

    /// Set access control list
    pub fn set_acl(&mut self, acl: AccessControlList) {
        self.acl = acl;
        info!("Updated access control list");
    }

    /// Check if a string matches a pattern
    fn matches_pattern(&self, text: &str, pattern: &PathPattern) -> Result<bool> {
        match pattern.pattern_type {
            PatternType::Exact => Ok(text == pattern.pattern),
            PatternType::Glob => {
                // Simple glob pattern matching
                if pattern.pattern.contains('*') {
                    let regex_pattern = pattern.pattern.replace('*', ".*");
                    if let Ok(regex) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
                        Ok(regex.is_match(text))
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(text == pattern.pattern)
                }
            }
            PatternType::Regex => {
                if let Ok(regex) = regex::Regex::new(&pattern.pattern) {
                    Ok(regex.is_match(text))
                } else {
                    Err(AosError::Security(format!(
                        "Invalid regex pattern: {}",
                        pattern.pattern
                    )))
                }
            }
        }
    }

    /// List all granted capabilities
    pub fn list_capabilities(&self) -> CapabilityList {
        CapabilityList {
            dir_capabilities: self.dir_caps.keys().cloned().collect(),
            file_capabilities: self.file_caps.keys().cloned().collect(),
            acl: self.acl.clone(),
        }
    }
}

/// List of granted capabilities
#[derive(Debug, Clone)]
pub struct CapabilityList {
    /// Directory capabilities
    pub dir_capabilities: Vec<PathBuf>,
    /// File capabilities
    pub file_capabilities: Vec<PathBuf>,
    /// Access control list
    pub acl: AccessControlList,
}

impl Default for AccessControlList {
    fn default() -> Self {
        Self {
            allowed_operations: vec![
                Operation::Read,
                Operation::Write,
                Operation::Create,
                Operation::List,
                Operation::Stat,
            ],
            denied_operations: vec![Operation::Execute],
            allowed_paths: vec![],
            denied_paths: vec![
                PathPattern {
                    pattern: "*.exe".to_string(),
                    pattern_type: PatternType::Glob,
                    recursive: true,
                },
                PathPattern {
                    pattern: "*.bat".to_string(),
                    pattern_type: PatternType::Glob,
                    recursive: true,
                },
                PathPattern {
                    pattern: "*.sh".to_string(),
                    pattern_type: PatternType::Glob,
                    recursive: true,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_capabilities() -> Result<()> {
        let mut caps = Capabilities::new()?;

        // Test operation checking
        assert!(caps.is_operation_allowed(&Operation::Read, &PathBuf::from("test.txt"))?);
        assert!(!caps.is_operation_allowed(&Operation::Execute, &PathBuf::from("test.exe"))?);

        Ok(())
    }

    #[test]
    fn test_path_patterns() -> Result<()> {
        let caps = Capabilities::new()?;

        let pattern = PathPattern {
            pattern: "*.txt".to_string(),
            pattern_type: PatternType::Glob,
            recursive: true,
        };

        assert!(caps.matches_pattern("test.txt", &pattern)?);
        assert!(!caps.matches_pattern("test.exe", &pattern)?);

        Ok(())
    }
}
