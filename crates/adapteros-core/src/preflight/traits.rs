//! Traits for preflight data access
//!
//! These traits abstract the adapter data and database operations needed
//! for preflight checks, allowing the same validation logic to work in
//! both CLI and Server API contexts.

use async_trait::async_trait;

/// Trait for adapter data required by preflight checks
///
/// This trait provides a uniform interface to adapter data regardless
/// of whether it comes from CLI direct DB access or API state.
pub trait PreflightAdapterData {
    /// Get the adapter ID
    fn id(&self) -> &str;

    /// Get the tenant ID
    fn tenant_id(&self) -> &str;

    /// Get the current lifecycle state string
    fn lifecycle_state(&self) -> &str;

    /// Get the adapter tier (ephemeral, warm, persistent)
    fn tier(&self) -> &str;

    /// Get the .aos file path (if set)
    fn aos_file_path(&self) -> Option<&str>;

    /// Get the .aos file hash (if set)
    fn aos_file_hash(&self) -> Option<&str>;

    /// Get the content hash (BLAKE3 of manifest + payload)
    fn content_hash_b3(&self) -> Option<&str>;

    /// Get the manifest hash (BLAKE3 of manifest bytes)
    fn manifest_hash(&self) -> Option<&str>;

    /// Get the repository ID (if linked to a repo)
    fn repo_id(&self) -> Option<&str>;

    /// Get the repository path (if linked to a repo)
    fn repo_path(&self) -> Option<&str>;

    /// Get the codebase scope (for Set 4 adapters)
    fn codebase_scope(&self) -> Option<&str>;

    /// Get the raw metadata JSON (for extracting branch, etc.)
    fn metadata_json(&self) -> Option<&str>;

    /// Get the adapter label (adapter_id or id, for display)
    fn label(&self) -> &str {
        self.id()
    }
}

/// Result of active uniqueness validation
#[derive(Debug, Clone, Default)]
pub struct ActiveUniquenessResult {
    /// Whether the validation passed (no conflicts)
    pub is_valid: bool,

    /// IDs of conflicting active adapters
    pub conflicting_adapters: Vec<String>,

    /// Reason for conflict (if any)
    pub conflict_reason: Option<String>,
}

impl ActiveUniquenessResult {
    /// Create a valid (no conflicts) result
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            conflicting_adapters: Vec::new(),
            conflict_reason: None,
        }
    }

    /// Create an invalid (has conflicts) result
    pub fn conflict(adapters: Vec<String>, reason: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            conflicting_adapters: adapters,
            conflict_reason: Some(reason.into()),
        }
    }
}

/// Trait for preflight database operations
///
/// This trait abstracts the database operations needed for preflight checks,
/// allowing the same validation logic to work with different DB implementations.
#[async_trait]
pub trait PreflightDbOps: Send + Sync {
    /// Check if a training snapshot exists for an adapter
    ///
    /// Returns Ok(true) if evidence exists, Ok(false) if not, Err on DB failure.
    async fn has_training_snapshot(&self, adapter_id: &str) -> Result<bool, String>;

    /// Validate active uniqueness (single-path logic)
    ///
    /// Checks if there are any conflicting active adapters for the same
    /// repo/path/scope/branch combination.
    async fn validate_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        codebase_scope: Option<String>,
        branch: Option<String>,
    ) -> Result<ActiveUniquenessResult, String>;
}

/// Blanket implementation for references
#[async_trait]
impl<T: PreflightDbOps + ?Sized> PreflightDbOps for &T {
    async fn has_training_snapshot(&self, adapter_id: &str) -> Result<bool, String> {
        (*self).has_training_snapshot(adapter_id).await
    }

    async fn validate_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        codebase_scope: Option<String>,
        branch: Option<String>,
    ) -> Result<ActiveUniquenessResult, String> {
        (*self)
            .validate_active_uniqueness(adapter_id, repo_id, repo_path, codebase_scope, branch)
            .await
    }
}

/// Blanket implementation for Arc
#[async_trait]
impl<T: PreflightDbOps + ?Sized> PreflightDbOps for std::sync::Arc<T> {
    async fn has_training_snapshot(&self, adapter_id: &str) -> Result<bool, String> {
        self.as_ref().has_training_snapshot(adapter_id).await
    }

    async fn validate_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        codebase_scope: Option<String>,
        branch: Option<String>,
    ) -> Result<ActiveUniquenessResult, String> {
        self.as_ref()
            .validate_active_uniqueness(adapter_id, repo_id, repo_path, codebase_scope, branch)
            .await
    }
}

/// Simple adapter data wrapper for testing or when data is already extracted
#[derive(Debug, Clone, Default)]
pub struct SimpleAdapterData {
    pub id: String,
    pub tenant_id: String,
    pub lifecycle_state: String,
    pub tier: String,
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    pub content_hash_b3: Option<String>,
    pub manifest_hash: Option<String>,
    pub repo_id: Option<String>,
    pub repo_path: Option<String>,
    pub codebase_scope: Option<String>,
    pub metadata_json: Option<String>,
}

impl PreflightAdapterData for SimpleAdapterData {
    fn id(&self) -> &str {
        &self.id
    }

    fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    fn lifecycle_state(&self) -> &str {
        &self.lifecycle_state
    }

    fn tier(&self) -> &str {
        &self.tier
    }

    fn aos_file_path(&self) -> Option<&str> {
        self.aos_file_path.as_deref()
    }

    fn aos_file_hash(&self) -> Option<&str> {
        self.aos_file_hash.as_deref()
    }

    fn content_hash_b3(&self) -> Option<&str> {
        self.content_hash_b3.as_deref()
    }

    fn manifest_hash(&self) -> Option<&str> {
        self.manifest_hash.as_deref()
    }

    fn repo_id(&self) -> Option<&str> {
        self.repo_id.as_deref()
    }

    fn repo_path(&self) -> Option<&str> {
        self.repo_path.as_deref()
    }

    fn codebase_scope(&self) -> Option<&str> {
        self.codebase_scope.as_deref()
    }

    fn metadata_json(&self) -> Option<&str> {
        self.metadata_json.as_deref()
    }
}

/// Mock DB operations for testing
#[cfg(test)]
pub mod mock {
    use super::*;

    /// Mock DB that always returns success
    #[derive(Debug, Clone, Default)]
    pub struct MockPreflightDb {
        pub has_snapshot: bool,
        pub conflict_result: Option<ActiveUniquenessResult>,
    }

    impl MockPreflightDb {
        pub fn with_snapshot() -> Self {
            Self {
                has_snapshot: true,
                conflict_result: None,
            }
        }

        pub fn without_snapshot() -> Self {
            Self {
                has_snapshot: false,
                conflict_result: None,
            }
        }

        pub fn with_conflict(adapters: Vec<String>, reason: &str) -> Self {
            Self {
                has_snapshot: true,
                conflict_result: Some(ActiveUniquenessResult::conflict(adapters, reason)),
            }
        }
    }

    #[async_trait]
    impl PreflightDbOps for MockPreflightDb {
        async fn has_training_snapshot(&self, _adapter_id: &str) -> Result<bool, String> {
            Ok(self.has_snapshot)
        }

        async fn validate_active_uniqueness(
            &self,
            _adapter_id: &str,
            _repo_id: Option<String>,
            _repo_path: Option<String>,
            _codebase_scope: Option<String>,
            _branch: Option<String>,
        ) -> Result<ActiveUniquenessResult, String> {
            Ok(self
                .conflict_result
                .clone()
                .unwrap_or_else(ActiveUniquenessResult::valid))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockPreflightDb;
    use super::*;

    #[test]
    fn test_simple_adapter_data() {
        let data = SimpleAdapterData {
            id: "test-adapter".to_string(),
            tenant_id: "tenant-1".to_string(),
            lifecycle_state: "ready".to_string(),
            tier: "warm".to_string(),
            content_hash_b3: Some("abc123".to_string()),
            ..Default::default()
        };

        assert_eq!(data.id(), "test-adapter");
        assert_eq!(data.content_hash_b3(), Some("abc123"));
        assert_eq!(data.manifest_hash(), None);
    }

    #[tokio::test]
    async fn test_mock_db() {
        let db = MockPreflightDb::with_snapshot();
        assert!(db.has_training_snapshot("any").await.unwrap());

        let db = MockPreflightDb::without_snapshot();
        assert!(!db.has_training_snapshot("any").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_conflict() {
        let db =
            MockPreflightDb::with_conflict(vec!["other-adapter".to_string()], "Same repo/branch");

        let result = db
            .validate_active_uniqueness("test", None, None, None, None)
            .await
            .unwrap();

        assert!(!result.is_valid);
        assert_eq!(result.conflicting_adapters, vec!["other-adapter"]);
    }
}
