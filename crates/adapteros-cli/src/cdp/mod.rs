//! Commit Delta Pack (CDP) core types and functionality
//!
//! CDPs capture git diffs, test results, linter output, and symbol changes
//! for ephemeral adapter training and evidence grounding.

use adapteros_cdp::{CdpId, CommitDeltaPack, CdpMetadata};
use adapteros_core::{B3Hash, Result};
use adapteros_git::{DiffAnalyzer, DiffSummary, ChangedSymbol, SymbolChangeType, SymbolKind};
use adapteros_lora_worker::{LinterResult, TestResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod language_detector;
pub mod store;

pub use language_detector::LanguageDetector;
pub use store::CdpStore;
pub use adapteros_git::DiffAnalyzer;
pub use adapteros_cdp::{CdpId, CommitDeltaPack, CdpMetadata};

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
        summary.modified_files.push(PathBuf::from("existing_file.rs"));
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
