//! CDP storage implementation using content-addressed storage

use adapteros_artifacts::CasStore;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cdp::{CdpId, CommitDeltaPack};

/// CDP storage implementation
pub struct CdpStore {
    /// Content-addressed store for CDP artifacts
    cas_store: CasStore,
    /// Metadata index for querying CDPs
    metadata_index: HashMap<CdpId, CdpMetadata>,
}

/// CDP metadata for indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpMetadata {
    /// CDP ID
    pub cdp_id: CdpId,
    /// Repository ID
    pub repo_id: String,
    /// Commit SHA
    pub commit_sha: String,
    /// Content hash
    pub content_hash: B3Hash,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Test pass status
    pub test_passed: bool,
    /// Number of linter issues
    pub linter_issues: usize,
    /// Number of changed symbols
    pub changed_symbols: usize,
    /// Branch name
    pub branch: String,
    /// Author email
    pub author: String,
    /// Commit message (first line)
    pub message: String,
}

impl CdpStore {
    /// Create a new CDP store
    pub fn new<P: AsRef<Path>>(root_path: P) -> Result<Self> {
        let cas_store = CasStore::new(root_path)?;
        Ok(Self {
            cas_store,
            metadata_index: HashMap::new(),
        })
    }

    /// Store a CDP and return its content hash
    pub fn store_cdp(&mut self, cdp: &CommitDeltaPack) -> Result<B3Hash> {
        // Serialize CDP to canonical JSON
        let json_bytes = serde_json::to_vec(cdp)
            .map_err(|e| AosError::Other(format!("Failed to serialize CDP: {}", e)))?;

        // Store in CAS
        let content_hash = self.cas_store.store("cdp", &json_bytes)?;

        // Create metadata index entry
        let metadata = CdpMetadata {
            cdp_id: cdp.cdp_id.clone(),
            repo_id: cdp.repo_id.clone(),
            commit_sha: cdp.commit_sha.clone(),
            content_hash,
            created_at: chrono::Utc::now(),
            test_passed: !cdp.has_test_failures(),
            linter_issues: cdp.total_linter_issues(),
            changed_symbols: cdp.changed_symbols.len(),
            branch: cdp.metadata.branch.clone(),
            author: cdp.metadata.author.clone(),
            message: cdp.metadata.short_description(),
        };

        // Update metadata index
        self.metadata_index.insert(cdp.cdp_id.clone(), metadata);

        Ok(content_hash)
    }

    /// Load a CDP by its ID
    pub fn load_cdp(&self, cdp_id: &CdpId) -> Result<CommitDeltaPack> {
        // Get metadata to find content hash
        let metadata = self.metadata_index.get(cdp_id)
            .ok_or_else(|| AosError::Other(format!("CDP not found: {}", cdp_id)))?;

        // Load from CAS
        let json_bytes = self.cas_store.load("cdp", &metadata.content_hash)?;

        // Deserialize CDP
        let cdp: CommitDeltaPack = serde_json::from_slice(&json_bytes)
            .map_err(|e| AosError::Other(format!("Failed to deserialize CDP: {}", e)))?;

        Ok(cdp)
    }

    /// Check if a CDP exists
    pub fn exists(&self, cdp_id: &CdpId) -> bool {
        self.metadata_index.contains_key(cdp_id)
    }

    /// List all CDPs for a repository
    pub fn list_for_repo(&self, repo_id: &str) -> Vec<&CdpMetadata> {
        self.metadata_index
            .values()
            .filter(|metadata| metadata.repo_id == repo_id)
            .collect()
    }

    /// List all CDPs for a specific commit
    pub fn list_for_commit(&self, repo_id: &str, commit_sha: &str) -> Vec<&CdpMetadata> {
        self.metadata_index
            .values()
            .filter(|metadata| metadata.repo_id == repo_id && metadata.commit_sha == commit_sha)
            .collect()
    }

    /// List recent CDPs (most recent first)
    pub fn list_recent(&self, limit: usize) -> Vec<&CdpMetadata> {
        let mut entries: Vec<&CdpMetadata> = self.metadata_index.values().collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries.truncate(limit);
        entries
    }

    /// Search CDPs by author
    pub fn search_by_author(&self, author: &str) -> Vec<&CdpMetadata> {
        self.metadata_index
            .values()
            .filter(|metadata| metadata.author.contains(author))
            .collect()
    }

    /// Search CDPs by commit message
    pub fn search_by_message(&self, query: &str) -> Vec<&CdpMetadata> {
        self.metadata_index
            .values()
            .filter(|metadata| metadata.message.to_lowercase().contains(&query.to_lowercase()))
            .collect()
    }

    /// Get CDP statistics
    pub fn get_stats(&self) -> CdpStats {
        let total_cdps = self.metadata_index.len();
        let test_passed = self.metadata_index.values().filter(|m| m.test_passed).count();
        let with_linter_issues = self.metadata_index.values().filter(|m| m.linter_issues > 0).count();
        let total_symbols = self.metadata_index.values().map(|m| m.changed_symbols).sum();

        CdpStats {
            total_cdps,
            test_passed,
            test_failed: total_cdps - test_passed,
            with_linter_issues,
            total_symbols,
        }
    }

    /// Remove a CDP (for cleanup)
    pub fn remove_cdp(&mut self, cdp_id: &CdpId) -> Result<()> {
        if let Some(metadata) = self.metadata_index.remove(cdp_id) {
            // Note: We don't remove from CAS as it might be referenced elsewhere
            // In a production system, you'd want reference counting or garbage collection
            tracing::info!("Removed CDP metadata: {}", cdp_id);
        }
        Ok(())
    }

    /// Load metadata index from disk (for persistence)
    pub fn load_metadata_index<P: AsRef<Path>>(&mut self, index_path: P) -> Result<()> {
        let path = index_path.as_ref();
        if !path.exists() {
            return Ok(()); // No existing index
        }

        let json_bytes = std::fs::read(path)
            .map_err(|e| AosError::Other(format!("Failed to read metadata index: {}", e)))?;

        let index: HashMap<CdpId, CdpMetadata> = serde_json::from_slice(&json_bytes)
            .map_err(|e| AosError::Other(format!("Failed to deserialize metadata index: {}", e)))?;

        self.metadata_index = index;
        Ok(())
    }

    /// Save metadata index to disk (for persistence)
    pub fn save_metadata_index<P: AsRef<Path>>(&self, index_path: P) -> Result<()> {
        let json_bytes = serde_json::to_vec_pretty(&self.metadata_index)
            .map_err(|e| AosError::Other(format!("Failed to serialize metadata index: {}", e)))?;

        std::fs::write(index_path, json_bytes)
            .map_err(|e| AosError::Other(format!("Failed to write metadata index: {}", e)))?;

        Ok(())
    }
}

/// CDP statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpStats {
    /// Total number of CDPs
    pub total_cdps: usize,
    /// Number of CDPs with passing tests
    pub test_passed: usize,
    /// Number of CDPs with failing tests
    pub test_failed: usize,
    /// Number of CDPs with linter issues
    pub with_linter_issues: usize,
    /// Total number of changed symbols across all CDPs
    pub total_symbols: usize,
}

impl CdpStats {
    /// Get test pass rate as percentage
    pub fn test_pass_rate(&self) -> f64 {
        if self.total_cdps == 0 {
            0.0
        } else {
            (self.test_passed as f64 / self.total_cdps as f64) * 100.0
        }
    }

    /// Get average symbols per CDP
    pub fn avg_symbols_per_cdp(&self) -> f64 {
        if self.total_cdps == 0 {
            0.0
        } else {
            self.total_symbols as f64 / self.total_cdps as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdp::{DiffSummary, SymbolChangeType, SymbolKind};
    use adapteros_lora_worker::{LinterResult, LinterType, TestResult, TestFramework};
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    fn create_test_cdp() -> CommitDeltaPack {
        use crate::cdp::metadata::CdpMetadata;
        use chrono::Utc;

        let metadata = CdpMetadata::new(
            "test@example.com".to_string(),
            "Test commit".to_string(),
            Utc::now(),
            "main".to_string(),
            PathBuf::from("var/test-repo"),
        );

        CommitDeltaPack::new(
            "test-repo".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            DiffSummary::new(),
            vec![],
            vec![],
            vec![],
            metadata,
        )
    }

    #[test]
    fn test_cdp_store_creation() {
        let temp_dir = new_test_tempdir();
        let store = CdpStore::new(temp_dir.path()).unwrap();
        assert_eq!(store.metadata_index.len(), 0);
    }

    #[test]
    fn test_store_and_load_cdp() {
        let temp_dir = new_test_tempdir();
        let mut store = CdpStore::new(temp_dir.path()).unwrap();
        
        let cdp = create_test_cdp();
        let cdp_id = cdp.cdp_id.clone();
        
        // Store CDP
        let content_hash = store.store_cdp(&cdp).unwrap();
        assert!(!content_hash.to_hex().is_empty());
        
        // Check exists
        assert!(store.exists(&cdp_id));
        
        // Load CDP
        let loaded_cdp = store.load_cdp(&cdp_id).unwrap();
        assert_eq!(loaded_cdp.cdp_id, cdp_id);
        assert_eq!(loaded_cdp.repo_id, "test-repo");
        assert_eq!(loaded_cdp.commit_sha, "abc123");
    }

    #[test]
    fn test_list_for_repo() {
        let temp_dir = new_test_tempdir();
        let mut store = CdpStore::new(temp_dir.path()).unwrap();
        
        let cdp = create_test_cdp();
        store.store_cdp(&cdp).unwrap();
        
        let repo_cdps = store.list_for_repo("test-repo");
        assert_eq!(repo_cdps.len(), 1);
        assert_eq!(repo_cdps[0].repo_id, "test-repo");
    }

    #[test]
    fn test_search_by_author() {
        let temp_dir = new_test_tempdir();
        let mut store = CdpStore::new(temp_dir.path()).unwrap();
        
        let cdp = create_test_cdp();
        store.store_cdp(&cdp).unwrap();
        
        let author_cdps = store.search_by_author("test@example.com");
        assert_eq!(author_cdps.len(), 1);
        assert_eq!(author_cdps[0].author, "test@example.com");
    }

    #[test]
    fn test_get_stats() {
        let temp_dir = new_test_tempdir();
        let mut store = CdpStore::new(temp_dir.path()).unwrap();
        
        let cdp = create_test_cdp();
        store.store_cdp(&cdp).unwrap();
        
        let stats = store.get_stats();
        assert_eq!(stats.total_cdps, 1);
        assert_eq!(stats.test_passed, 1);
        assert_eq!(stats.test_failed, 0);
    }
}
