//! Commit Delta Pack (CDP) core types and functionality
//!
//! CDPs capture git diffs, test results, linter output, and symbol changes
//! for ephemeral adapter training and evidence grounding.

use adapteros_core::{B3Hash, Result};
use adapteros_lora_worker::{LinterResult, TestResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod assembler;
pub mod diff_analyzer;
pub mod language_detector;
pub mod metadata;
pub mod store;

pub use assembler::CdpAssembler;
pub use diff_analyzer::DiffAnalyzer;
pub use language_detector::LanguageDetector;
pub use metadata::CdpMetadata;
pub use store::CdpStore;

/// Unique identifier for a Commit Delta Pack
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CdpId(String);

impl CdpId {
    /// Create a new CDP ID from components
    pub fn new(repo_id: &str, commit_sha: &str) -> Self {
        let combined = format!("{}:{}", repo_id, commit_sha);
        let hash = B3Hash::hash(combined.as_bytes());
        Self(hash.to_hex())
    }

    /// Get the CDP ID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the CDP ID as bytes
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for CdpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Summary of git diff changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Files that were added
    pub added_files: Vec<PathBuf>,
    /// Files that were modified
    pub modified_files: Vec<PathBuf>,
    /// Files that were deleted
    pub deleted_files: Vec<PathBuf>,
    /// Total lines added
    pub lines_added: usize,
    /// Total lines removed
    pub lines_removed: usize,
    /// Language distribution of changes
    pub language_changes: HashMap<String, usize>,
}

impl DiffSummary {
    /// Create a new diff summary
    pub fn new() -> Self {
        Self {
            added_files: Vec::new(),
            modified_files: Vec::new(),
            deleted_files: Vec::new(),
            lines_added: 0,
            lines_removed: 0,
            language_changes: HashMap::new(),
        }
    }

    /// Get total number of changed files
    pub fn total_files(&self) -> usize {
        self.added_files.len() + self.modified_files.len() + self.deleted_files.len()
    }

    /// Check if diff is empty
    pub fn is_empty(&self) -> bool {
        self.total_files() == 0
    }
}

impl Default for DiffSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// A symbol that was changed in the commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind (function, struct, trait, etc.)
    pub kind: SymbolKind,
    /// File path where symbol is defined
    pub file_path: PathBuf,
    /// Change type
    pub change_type: SymbolChangeType,
    /// Line number where symbol is defined
    pub line: usize,
    /// Optional column range
    pub column_range: Option<(usize, usize)>,
}

/// Type of symbol change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolChangeType {
    Added,
    Modified,
    Deleted,
    Moved,
}

/// Symbol kind enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Trait,
    Impl,
    Enum,
    Module,
    Constant,
    Type,
    Macro,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Impl => write!(f, "impl"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Constant => write!(f, "constant"),
            SymbolKind::Type => write!(f, "type"),
            SymbolKind::Macro => write!(f, "macro"),
        }
    }
}

/// A complete Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaPack {
    /// Unique CDP identifier
    pub cdp_id: CdpId,
    /// Repository identifier
    pub repo_id: String,
    /// Commit SHA
    pub commit_sha: String,
    /// Parent commit SHA
    pub parent_sha: String,
    /// Summary of git diff changes
    pub diff_summary: DiffSummary,
    /// Symbols that were changed
    pub changed_symbols: Vec<ChangedSymbol>,
    /// Test execution results
    pub test_results: Vec<TestResult>,
    /// Linter execution results
    pub linter_results: Vec<LinterResult>,
    /// CDP metadata
    pub metadata: CdpMetadata,
    /// Content hash for determinism
    pub content_hash: B3Hash,
}

impl CommitDeltaPack {
    /// Create a new CDP
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo_id: String,
        commit_sha: String,
        parent_sha: String,
        diff_summary: DiffSummary,
        changed_symbols: Vec<ChangedSymbol>,
        test_results: Vec<TestResult>,
        linter_results: Vec<LinterResult>,
        metadata: CdpMetadata,
    ) -> Self {
        let cdp_id = CdpId::new(&repo_id, &commit_sha);

        Self {
            cdp_id,
            repo_id,
            commit_sha,
            parent_sha,
            diff_summary,
            changed_symbols,
            test_results,
            linter_results,
            metadata,
            content_hash: B3Hash::hash(b""), // Will be computed during assembly
        }
    }

    /// Check if CDP has any test failures
    pub fn has_test_failures(&self) -> bool {
        self.test_results.iter().any(|result| result.failed > 0)
    }

    /// Check if CDP has any linter issues
    pub fn has_linter_issues(&self) -> bool {
        self.linter_results
            .iter()
            .any(|result| !result.errors.is_empty() || !result.warnings.is_empty())
    }

    /// Get total number of linter issues
    pub fn total_linter_issues(&self) -> usize {
        self.linter_results
            .iter()
            .map(|result| result.errors.len() + result.warnings.len())
            .sum()
    }

    /// Get languages detected in the changes
    pub fn languages(&self) -> Vec<String> {
        self.diff_summary.language_changes.keys().cloned().collect()
    }

    /// Check if CDP is suitable for ephemeral adapter training
    pub fn is_suitable_for_training(&self) -> bool {
        // Require at least one symbol change and no test failures
        !self.changed_symbols.is_empty() && !self.has_test_failures()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_cdp_id_generation() {
        let id1 = CdpId::new("repo1", "abc123");
        let id2 = CdpId::new("repo1", "abc123");
        let id3 = CdpId::new("repo2", "abc123");

        // Same repo and commit should generate same ID
        assert_eq!(id1, id2);

        // Different repo should generate different ID
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_diff_summary() {
        let mut summary = DiffSummary::new();
        summary.added_files.push(PathBuf::from("new_file.rs"));
        summary
            .modified_files
            .push(PathBuf::from("existing_file.rs"));
        summary.lines_added = 10;
        summary.lines_removed = 5;

        assert_eq!(summary.total_files(), 2);
        assert!(!summary.is_empty());
    }

    #[test]
    fn test_changed_symbol() {
        let symbol = ChangedSymbol {
            name: "test_function".to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("src/lib.rs"),
            change_type: SymbolChangeType::Modified,
            line: 42,
            column_range: Some((10, 20)),
        };

        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::Function);
    }

    #[test]
    fn test_cdp_creation() {
        let metadata = CdpMetadata {
            author: "test@example.com".to_string(),
            message: "Test commit".to_string(),
            timestamp: chrono::Utc::now(),
            branch: "main".to_string(),
            remote_url: None,
            repo_path: std::path::PathBuf::from("/tmp/test-repo"),
            author_name: None,
            committer: None,
            committer_name: None,
        };

        let cdp = CommitDeltaPack::new(
            "test-repo".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            DiffSummary::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            metadata,
        );

        assert_eq!(cdp.repo_id, "test-repo");
        assert_eq!(cdp.commit_sha, "abc123");
        assert_eq!(cdp.parent_sha, "def456");
        assert!(!cdp.has_test_failures());
        assert!(!cdp.has_linter_issues());
    }
}
