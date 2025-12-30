//! Preflight Integration for Server API
//!
//! This module provides implementations of the core preflight traits
//! for the server API context, allowing the shared preflight logic
//! to work with the API's database and adapter types.

use adapteros_core::preflight::{ActiveUniquenessResult, PreflightAdapterData, PreflightDbOps};
use adapteros_db::adapters::Adapter;
use adapteros_db::Db;
use async_trait::async_trait;

// ============================================================================
// PreflightAdapterData implementation for Adapter
// ============================================================================

/// Wrapper to implement PreflightAdapterData for the DB Adapter type
pub struct AdapterPreflightData<'a> {
    adapter: &'a Adapter,
}

impl<'a> AdapterPreflightData<'a> {
    pub fn new(adapter: &'a Adapter) -> Self {
        Self { adapter }
    }
}

impl<'a> From<&'a Adapter> for AdapterPreflightData<'a> {
    fn from(adapter: &'a Adapter) -> Self {
        Self::new(adapter)
    }
}

impl<'a> PreflightAdapterData for AdapterPreflightData<'a> {
    fn id(&self) -> &str {
        self.adapter
            .adapter_id
            .as_deref()
            .unwrap_or(&self.adapter.id)
    }

    fn tenant_id(&self) -> &str {
        &self.adapter.tenant_id
    }

    fn lifecycle_state(&self) -> &str {
        &self.adapter.lifecycle_state
    }

    fn tier(&self) -> &str {
        &self.adapter.tier
    }

    fn aos_file_path(&self) -> Option<&str> {
        self.adapter.aos_file_path.as_deref()
    }

    fn aos_file_hash(&self) -> Option<&str> {
        self.adapter.aos_file_hash.as_deref()
    }

    fn content_hash_b3(&self) -> Option<&str> {
        self.adapter.content_hash_b3.as_deref()
    }

    fn manifest_hash(&self) -> Option<&str> {
        self.adapter.manifest_hash.as_deref()
    }

    fn repo_id(&self) -> Option<&str> {
        self.adapter.repo_id.as_deref()
    }

    fn repo_path(&self) -> Option<&str> {
        self.adapter.repo_path.as_deref()
    }

    fn codebase_scope(&self) -> Option<&str> {
        self.adapter.codebase_scope.as_deref()
    }

    fn metadata_json(&self) -> Option<&str> {
        self.adapter.metadata_json.as_deref()
    }

    fn label(&self) -> &str {
        self.adapter
            .adapter_id
            .as_deref()
            .unwrap_or(&self.adapter.id)
    }
}

// ============================================================================
// PreflightDbOps implementation for Db
// ============================================================================

/// Wrapper to implement PreflightDbOps for the DB type
pub struct DbPreflightOps<'a> {
    db: &'a Db,
}

impl<'a> DbPreflightOps<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }
}

impl<'a> From<&'a Db> for DbPreflightOps<'a> {
    fn from(db: &'a Db) -> Self {
        Self::new(db)
    }
}

#[async_trait]
impl<'a> PreflightDbOps for DbPreflightOps<'a> {
    async fn has_training_snapshot(&self, adapter_id: &str) -> Result<bool, String> {
        match self.db.get_adapter_training_snapshot(adapter_id).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(format!("Failed to check training snapshot: {}", e)),
        }
    }

    async fn validate_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        codebase_scope: Option<String>,
        branch: Option<String>,
    ) -> Result<ActiveUniquenessResult, String> {
        // If no scope fields are set, skip uniqueness check
        if repo_id.is_none() && repo_path.is_none() && codebase_scope.is_none() {
            return Ok(ActiveUniquenessResult::valid());
        }

        // Query for active adapters with matching scope
        match self
            .db
            .validate_active_uniqueness(adapter_id, repo_id, repo_path, codebase_scope, branch)
            .await
        {
            Ok(result) => {
                if result.is_valid {
                    Ok(ActiveUniquenessResult::valid())
                } else {
                    Ok(ActiveUniquenessResult::conflict(
                        result.conflicting_adapters,
                        result
                            .conflict_reason
                            .unwrap_or_else(|| "Conflicting active adapter exists".to_string()),
                    ))
                }
            }
            Err(e) => Err(format!("Failed to validate uniqueness: {}", e)),
        }
    }
}

// ============================================================================
// Convenience functions
// ============================================================================

use adapteros_core::preflight::{run_preflight, PreflightConfig, PreflightResult};

/// Run preflight checks on an adapter using the server API context
///
/// This is the main entry point for running preflight checks in API handlers.
pub async fn run_api_preflight(
    adapter: &Adapter,
    db: &Db,
    config: &PreflightConfig,
) -> PreflightResult {
    let adapter_data = AdapterPreflightData::new(adapter);
    let db_ops = DbPreflightOps::new(db);
    run_preflight(&adapter_data, &db_ops, config).await
}

/// Run preflight with default strict configuration
#[allow(dead_code)]
pub async fn run_api_preflight_strict(
    adapter: &Adapter,
    db: &Db,
    tenant_id: &str,
    actor: &str,
) -> PreflightResult {
    let config = PreflightConfig::with_actor(tenant_id, actor);
    run_api_preflight(adapter, db, &config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests would require a test database setup
    // Unit tests for the wrapper implementations are in the core preflight module
}
