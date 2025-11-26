//! Core types for Git subsystem

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of change detected in a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// File was created
    Create,
    /// File was modified
    Modify,
    /// File was deleted
    Delete,
}

impl ChangeType {
    /// Convert from git2 delta status
    pub fn from_git_delta(delta: git2::Delta) -> Option<Self> {
        match delta {
            git2::Delta::Added => Some(ChangeType::Create),
            git2::Delta::Modified => Some(ChangeType::Modify),
            git2::Delta::Deleted => Some(ChangeType::Delete),
            git2::Delta::Renamed => Some(ChangeType::Modify),
            git2::Delta::Copied => Some(ChangeType::Create),
            git2::Delta::Typechange => Some(ChangeType::Modify),
            _ => None,
        }
    }
}

/// Represents a file change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    /// Path to the changed file relative to repository root
    pub path: PathBuf,

    /// Type of change (create, modify, delete)
    pub change_type: ChangeType,

    /// Associated adapter ID if the change is adapter-specific
    pub adapter_id: Option<String>,

    /// Timestamp when the change was detected
    pub timestamp: DateTime<Utc>,
}

impl FileChangeEvent {
    /// Create a new file change event
    pub fn new(path: PathBuf, change_type: ChangeType, adapter_id: Option<String>) -> Self {
        Self {
            path,
            change_type,
            adapter_id,
            timestamp: Utc::now(),
        }
    }

    /// Check if this change should be included based on patterns
    pub fn matches_patterns(
        &self,
        include_extensions: &[String],
        exclude_patterns: &[String],
    ) -> bool {
        let path_str = self.path.to_string_lossy();

        // Check exclude patterns first
        for pattern in exclude_patterns {
            if glob_match(&path_str, pattern) {
                return false;
            }
        }

        // If no include extensions specified, include all
        if include_extensions.is_empty() {
            return true;
        }

        // Check if file extension matches any include patterns
        if let Some(ext) = self.path.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy());
            include_extensions.iter().any(|inc_ext| inc_ext == &ext_str)
        } else {
            false
        }
    }
}

/// Batch of changes to be committed together
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeBatch {
    /// Adapter ID associated with this batch
    pub adapter_id: String,

    /// List of file changes in this batch
    pub changes: Vec<FileChangeEvent>,

    /// When this batch was created
    pub created_at: DateTime<Utc>,
}

impl ChangeBatch {
    /// Create a new empty change batch
    pub fn new(adapter_id: String) -> Self {
        Self {
            adapter_id,
            changes: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Add a change to this batch
    pub fn add_change(&mut self, change: FileChangeEvent) {
        self.changes.push(change);
    }

    /// Check if this batch is empty
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Get the number of changes in this batch
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Get a summary of changes by type
    pub fn summary(&self) -> ChangeSummary {
        let mut creates = 0;
        let mut modifies = 0;
        let mut deletes = 0;

        for change in &self.changes {
            match change.change_type {
                ChangeType::Create => creates += 1,
                ChangeType::Modify => modifies += 1,
                ChangeType::Delete => deletes += 1,
            }
        }

        ChangeSummary {
            creates,
            modifies,
            deletes,
            total: self.changes.len(),
        }
    }

    /// Generate a commit message for this batch
    pub fn generate_commit_message(&self) -> String {
        let summary = self.summary();
        let mut parts = Vec::new();

        if summary.creates > 0 {
            parts.push(format!("{} created", summary.creates));
        }
        if summary.modifies > 0 {
            parts.push(format!("{} modified", summary.modifies));
        }
        if summary.deletes > 0 {
            parts.push(format!("{} deleted", summary.deletes));
        }

        let summary_str = parts.join(", ");
        format!("AdapterOS: {} files {}", summary.total, summary_str)
    }
}

/// Summary of changes in a batch
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub creates: usize,
    pub modifies: usize,
    pub deletes: usize,
    pub total: usize,
}

/// Simple glob pattern matching (basic implementation)
fn glob_match(path: &str, pattern: &str) -> bool {
    // Simple pattern matching - handles * and **
    if pattern.contains("**") {
        // Match any number of directories
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1].trim_start_matches('/');
            return path.starts_with(prefix) && (suffix.is_empty() || path.ends_with(suffix));
        }
    }

    if pattern.contains('*') {
        // Simple wildcard matching
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    // Exact match
    path == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_batch_summary() {
        let mut batch = ChangeBatch::new("test-adapter".to_string());
        batch.add_change(FileChangeEvent::new(
            PathBuf::from("file1.txt"),
            ChangeType::Create,
            None,
        ));
        batch.add_change(FileChangeEvent::new(
            PathBuf::from("file2.txt"),
            ChangeType::Modify,
            None,
        ));
        batch.add_change(FileChangeEvent::new(
            PathBuf::from("file3.txt"),
            ChangeType::Delete,
            None,
        ));

        let summary = batch.summary();
        assert_eq!(summary.creates, 1);
        assert_eq!(summary.modifies, 1);
        assert_eq!(summary.deletes, 1);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("file.tmp", "*.tmp"));
        assert!(glob_match("target/debug/build", "target/**"));
        assert!(glob_match("node_modules/pkg/index.js", "node_modules/**"));
        assert!(!glob_match("src/main.rs", "*.tmp"));
    }

    #[test]
    fn test_file_change_event_patterns() {
        let event = FileChangeEvent::new(PathBuf::from("src/main.rs"), ChangeType::Modify, None);

        // Should match .rs extension
        assert!(event.matches_patterns(&[".rs".to_string()], &[]));

        // Should not match .tmp extension
        assert!(!event.matches_patterns(&[".tmp".to_string()], &[]));

        // Should be excluded by pattern
        let event2 =
            FileChangeEvent::new(PathBuf::from("target/debug/main"), ChangeType::Create, None);
        assert!(!event2.matches_patterns(&[], &["target/**".to_string()]));
    }
}
