//! Lifecycle management database operations
//!
//! Handles lifecycle state transitions and version history for adapters and stacks.

use crate::Db;
use adapteros_core::lifecycle::{LifecycleState, LifecycleTransition};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::str::FromStr;

/// A lifecycle transition event from version history
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LifecycleHistoryEvent {
    pub id: String,
    pub entity_id: String, // adapter_id or stack_id
    pub version: String,
    pub lifecycle_state: String,
    pub previous_lifecycle_state: Option<String>,
    pub reason: Option<String>,
    pub initiated_by: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

/// Stack reference information for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackReference {
    pub stack_id: String,
    pub stack_name: String,
    pub lifecycle_state: String,
}

impl Db {
    /// Transition an adapter to a new lifecycle state
    ///
    /// This method:
    /// 1. Validates the transition is allowed
    /// 2. Updates the adapter's lifecycle_state
    /// 3. Bumps the version
    /// 4. Records the transition in history
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to transition
    /// * `new_state` - The new lifecycle state (draft, active, deprecated, retired)
    /// * `reason` - Human-readable reason for the transition
    /// * `initiated_by` - User or system that initiated the transition
    ///
    /// # Returns
    /// The new version string after the transition
    ///
    /// **CRITICAL FIX:** Uses IMMEDIATE transaction to prevent race conditions
    /// between SELECT and UPDATE operations.
    pub async fn transition_adapter_lifecycle(
        &self,
        adapter_id: &str,
        new_state: &str,
        reason: &str,
        initiated_by: &str,
    ) -> Result<String> {
        let new_state_enum = LifecycleState::from_str(new_state).map_err(|e| {
            AosError::Validation(format!("Invalid lifecycle state '{}': {}", new_state, e))
        })?;

        // Use transaction to ensure atomicity of read-modify-write
        let mut tx = self.begin_write_tx().await?;

        // Get current state, version, artifact references, and repo metadata
        let row = sqlx::query(
            "SELECT id, tenant_id, lifecycle_state, version, aos_file_path, aos_file_hash, content_hash_b3, repo_id, metadata_json
             FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        let adapter_pk: String = row.get(0); // adapters.id (PK) for FK reference
        let tenant_id: String = row.get(1);
        let current_state_str: String = row.get(2);
        let current_version: String = row.get(3);
        let aos_file_path: Option<String> = row.get(4);
        let aos_file_hash: Option<String> = row.get(5);
        let content_hash_b3: Option<String> = row.get(6);
        let repo_id: Option<String> = row.get(7);
        let metadata_json: Option<String> = row.get(8);

        let current_state = LifecycleState::from_str(&current_state_str).map_err(|_| {
            AosError::Validation(format!(
                "Invalid current lifecycle state: {}",
                current_state_str
            ))
        })?;

        // Validate requested transition follows the lifecycle graph
        LifecycleTransition::new(current_state, new_state_enum)
            .validate()
            .map_err(|e| AosError::Validation(e.to_string()))?;

        // Immutable artifact required for ready/active/deprecated/retired
        if matches!(
            new_state_enum,
            LifecycleState::Ready
                | LifecycleState::Active
                | LifecycleState::Deprecated
                | LifecycleState::Retired
        ) && (aos_file_path.as_deref().map(str::is_empty).unwrap_or(true)
            || aos_file_hash.as_deref().map(str::is_empty).unwrap_or(true)
            || content_hash_b3
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true))
        {
            return Err(AosError::Validation(
                "Immutable .aos artifact (path, hash, content hash) required before entering ready/active/deprecated/retired"
                    .to_string(),
            ));
        }

        // Active requires prior metrics/policy evidence and uniqueness per repo/branch
        if matches!(new_state_enum, LifecycleState::Active) {
            // Require at least one training snapshot (acts as metrics/provenance evidence)
            let snapshot_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM adapter_training_snapshots WHERE adapter_id = ? LIMIT 1",
            )
            .bind(adapter_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to check training snapshot: {}", e)))?;

            if snapshot_exists.is_none() {
                return Err(AosError::Validation(
                    "Active state requires a training snapshot/metrics evidence".to_string(),
                ));
            }

            // Enforce single active per (repo_id, branch)
            if let Some(repo_id_val) = repo_id.as_deref() {
                let requested_branch = branch_from_metadata(&metadata_json);
                let rows = sqlx::query("SELECT adapter_id, metadata_json FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'")
                    .bind(repo_id_val)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to check active adapters: {}", e)))?;

                for row in rows {
                    let other_id: String = row.get(0);
                    if other_id == adapter_id {
                        continue;
                    }
                    let other_branch = branch_from_metadata(&row.get::<Option<String>, _>(1));
                    let branches_conflict = match (&requested_branch, &other_branch) {
                        (Some(req), Some(other)) => req == other,
                        (Some(_), None) => true, // unknown branch -> treat as conflict
                        (None, _) => true,       // unknown requested branch -> conservative
                    };
                    if branches_conflict {
                        return Err(AosError::Validation(format!(
                            "Active state requires uniqueness per repo/branch; adapter {} is already active for repo {}",
                            other_id, repo_id_val
                        )));
                    }
                }
            }
        }

        // Validate transition (done in application layer via LifecycleTransition)
        // This is a simple check to prevent obviously invalid transitions
        if current_state == new_state_enum {
            // No-op transition, just record it but don't bump version
            sqlx::query(
                "INSERT INTO adapter_lifecycle_history
                 (adapter_pk, tenant_id, version, lifecycle_state, previous_lifecycle_state, reason, initiated_by)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&adapter_pk)  // Use adapters.id (PK) for FK reference
            .bind(&tenant_id)
            .bind(&current_version)
            .bind(new_state_enum.as_str())
            .bind(current_state.as_str())
            .bind(reason)
            .bind(initiated_by)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            tx.commit()
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            return Ok(current_version);
        }

        // Bump version (simple increment for now)
        let new_version = self.bump_version(&current_version)?;

        // Update adapter
        sqlx::query(
            "UPDATE adapters
             SET lifecycle_state = ?, version = ?
             WHERE adapter_id = ?",
        )
        .bind(new_state_enum.as_str())
        .bind(&new_version)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // Record in history
        sqlx::query(
            "INSERT INTO adapter_lifecycle_history
             (adapter_pk, tenant_id, version, lifecycle_state, previous_lifecycle_state, reason, initiated_by)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&adapter_pk) // Use adapters.id (PK) for FK reference
        .bind(&tenant_id)
        .bind(&new_version)
        .bind(new_state_enum.as_str())
        .bind(current_state.as_str())
        .bind(reason)
        .bind(initiated_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(new_version)
    }

    /// Transition a stack to a new lifecycle state
    ///
    /// Similar to adapter transitions but for stacks.
    ///
    /// **CRITICAL FIX:** Uses IMMEDIATE transaction to prevent race conditions.
    pub async fn transition_stack_lifecycle(
        &self,
        stack_id: &str,
        new_state: &str,
        reason: &str,
        initiated_by: &str,
    ) -> Result<String> {
        // CRITICAL: Use IMMEDIATE transaction to acquire write lock immediately
        let mut tx = self.begin_write_tx().await?;

        // SQLite: Set IMMEDIATE mode on the transaction
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to acquire write lock: {}", e)))?;

        // Get current state, version, and adapter composition
        let row = sqlx::query(
            "SELECT lifecycle_state, version, adapter_ids_json FROM adapter_stacks WHERE id = ?",
        )
        .bind(stack_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("Stack not found: {}", stack_id)))?;

        let current_state: String = row.get(0);
        let current_version: String = row.get(1);
        let adapter_ids_json: String = row.get(2);

        // No-op transition check
        if current_state == new_state {
            sqlx::query(
                "INSERT INTO stack_version_history
                 (stack_id, version, lifecycle_state, previous_lifecycle_state, adapter_ids_json, reason, initiated_by)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(stack_id)
            .bind(&current_version)
            .bind(new_state)
            .bind(&current_state)
            .bind(&adapter_ids_json)
            .bind(reason)
            .bind(initiated_by)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            tx.commit()
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            return Ok(current_version);
        }

        // Bump version
        let new_version = self.bump_version(&current_version)?;

        // Update stack
        sqlx::query(
            "UPDATE adapter_stacks
             SET lifecycle_state = ?, version = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(new_state)
        .bind(&new_version)
        .bind(stack_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // Record in history (with adapter composition snapshot)
        sqlx::query(
            "INSERT INTO stack_version_history
             (stack_id, version, lifecycle_state, previous_lifecycle_state, adapter_ids_json, reason, initiated_by)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(stack_id)
        .bind(&new_version)
        .bind(new_state)
        .bind(&current_state)
        .bind(&adapter_ids_json)
        .bind(reason)
        .bind(initiated_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(new_version)
    }

    /// Get lifecycle history for an adapter
    ///
    /// Returns all lifecycle transitions ordered by timestamp (newest first).
    pub async fn get_adapter_lifecycle_history(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<LifecycleHistoryEvent>> {
        let events = sqlx::query_as::<_, LifecycleHistoryEvent>(
            "SELECT
                alh.id,
                a.adapter_id as entity_id,
                alh.version,
                alh.lifecycle_state,
                alh.previous_lifecycle_state,
                alh.reason,
                alh.initiated_by,
                alh.metadata_json,
                alh.created_at
             FROM adapter_lifecycle_history alh
             JOIN adapters a ON alh.adapter_pk = a.id
             WHERE a.adapter_id = ?
             ORDER BY alh.created_at DESC",
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(events)
    }

    /// Get lifecycle history for a stack
    ///
    /// Returns all lifecycle transitions ordered by timestamp (newest first).
    pub async fn get_stack_lifecycle_history(
        &self,
        stack_id: &str,
    ) -> Result<Vec<LifecycleHistoryEvent>> {
        let events = sqlx::query_as::<_, LifecycleHistoryEvent>(
            "SELECT
                id,
                stack_id as entity_id,
                version,
                lifecycle_state,
                previous_lifecycle_state,
                reason,
                initiated_by,
                metadata_json,
                created_at
             FROM stack_version_history
             WHERE stack_id = ?
             ORDER BY created_at DESC",
        )
        .bind(stack_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(events)
    }

    /// Check which active stacks reference an adapter
    ///
    /// Used for validation before transitioning an adapter to deprecated/retired.
    /// Returns stacks that are in 'active' or 'draft' state that reference this adapter.
    pub async fn check_active_stack_references(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<StackReference>> {
        let refs = sqlx::query(
            "SELECT id, name, lifecycle_state
             FROM adapter_stacks
             WHERE lifecycle_state IN ('active', 'draft')
               AND adapter_ids_json LIKE ?",
        )
        .bind(format!("%{}%", adapter_id))
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .into_iter()
        .map(|row| StackReference {
            stack_id: row.get(0),
            stack_name: row.get(1),
            lifecycle_state: row.get(2),
        })
        .collect();

        Ok(refs)
    }

    /// Bump a semantic version string
    ///
    /// Increments the patch version by default.
    /// For example: "1.2.3" -> "1.2.4"
    fn bump_version(&self, current_version: &str) -> Result<String> {
        let parts: Vec<&str> = current_version.split('.').collect();
        if parts.len() != 3 {
            // Fallback: treat as monotonic version
            return Ok(format!("{}.0.1", current_version));
        }

        let major: u32 = parts[0].parse().unwrap_or(1);
        let minor: u32 = parts[1].parse().unwrap_or(0);
        let patch: u32 = parts[2].parse().unwrap_or(0);

        Ok(format!("{}.{}.{}", major, minor, patch + 1))
    }

    /// Get all adapters in a specific lifecycle state
    pub async fn get_adapters_by_lifecycle_state(
        &self,
        lifecycle_state: &str,
    ) -> Result<Vec<crate::adapters::Adapter>> {
        let adapters = sqlx::query_as::<_, crate::adapters::Adapter>(
            "SELECT * FROM adapters WHERE lifecycle_state = ? ORDER BY created_at DESC",
        )
        .bind(lifecycle_state)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(adapters)
    }

    /// Get all stacks in a specific lifecycle state
    pub async fn get_stacks_by_lifecycle_state(
        &self,
        lifecycle_state: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
        let stacks = sqlx::query(
            "SELECT * FROM adapter_stacks WHERE lifecycle_state = ? ORDER BY created_at DESC",
        )
        .bind(lifecycle_state)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(stacks)
    }
}

fn branch_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    metadata_json.as_ref().and_then(|raw| {
        let parsed: Value = serde_json::from_str(raw).ok()?;
        parsed
            .get("branch")
            .and_then(|v| v.as_str())
            .or_else(|| parsed.get("git_branch").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bump_version() {
        // Create a temporary Db instance just for testing the bump_version helper
        use sqlx::SqlitePool;
        use std::str::FromStr;

        // Use a simple connection for this test (doesn't need migrations)
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();

        let db = Db::new(pool, None, crate::StorageMode::SqlOnly);

        assert_eq!(db.bump_version("1.0.0").unwrap(), "1.0.1");
        assert_eq!(db.bump_version("1.2.3").unwrap(), "1.2.4");
        assert_eq!(db.bump_version("2.5.99").unwrap(), "2.5.100");
    }

    #[tokio::test]
    async fn test_adapter_lifecycle_transition() {
        let db = Db::new_in_memory().await.unwrap();

        // Create tenant for FK constraint
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("Failed to create tenant");

        // Register a test adapter
        let params = crate::adapters::AdapterRegistrationBuilder::new()
            .adapter_id("test-adapter")
            .tenant_id(&tenant_id)
            .name("Test Adapter")
            .hash_b3("abc123")
            .rank(8)
            .tier("warm")
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();

        // Seed artifact and provenance required for ready/active/...
        sqlx::query(
            "UPDATE adapters SET aos_file_path = 'path/to.aos', aos_file_hash = 'hash123', content_hash_b3 = 'content123', repo_id = 'repo-1', metadata_json = '{\"branch\":\"main\"}' WHERE adapter_id = ?",
        )
        .bind("test-adapter")
        .execute(db.pool())
        .await
        .unwrap();

        // Set adapter to active state for transition test
        sqlx::query("UPDATE adapters SET lifecycle_state = 'active' WHERE adapter_id = ?")
            .bind("test-adapter")
            .execute(db.pool())
            .await
            .unwrap();

        // Transition from active to deprecated
        let new_version = db
            .transition_adapter_lifecycle(
                "test-adapter",
                "deprecated",
                "End of life",
                "admin@example.com",
            )
            .await
            .unwrap();

        assert_eq!(new_version, "1.0.1");

        // Check history
        let history = db
            .get_adapter_lifecycle_history("test-adapter")
            .await
            .unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].lifecycle_state, "deprecated");
        assert_eq!(
            history[0].previous_lifecycle_state,
            Some("active".to_string())
        );
    }

    #[tokio::test]
    async fn test_no_op_transition() {
        let db = Db::new_in_memory().await.unwrap();

        // Create tenant for FK constraint
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("Failed to create tenant");

        let params = crate::adapters::AdapterRegistrationBuilder::new()
            .adapter_id("test-adapter")
            .tenant_id(&tenant_id)
            .name("Test Adapter")
            .hash_b3("abc123")
            .rank(8)
            .tier("warm")
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();

        sqlx::query(
            "UPDATE adapters SET aos_file_path = 'path/to.aos', aos_file_hash = 'hash123', content_hash_b3 = 'content123', repo_id = 'repo-1', metadata_json = '{\"branch\":\"main\"}' WHERE adapter_id = ?",
        )
        .bind("test-adapter")
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO adapter_training_snapshots (id, tenant_id, adapter_id, training_job_id, collection_id, documents_json, chunk_manifest_hash, chunking_config_json, created_at)
             VALUES ('snap-1', ?, ?, 'job-1', NULL, '[]', 'chunk-hash', '{}', datetime('now'))",
        )
        .bind(&tenant_id)
        .bind("test-adapter")
        .execute(db.pool())
        .await
        .unwrap();

        // Set adapter to active state for no-op transition test
        sqlx::query("UPDATE adapters SET lifecycle_state = 'active' WHERE adapter_id = ?")
            .bind("test-adapter")
            .execute(db.pool())
            .await
            .unwrap();

        // Transition to same state (no-op)
        let version = db
            .transition_adapter_lifecycle("test-adapter", "active", "No change", "system")
            .await
            .unwrap();

        // Version should remain unchanged
        assert_eq!(version, "1.0.0");
    }

    #[tokio::test]
    async fn test_check_active_stack_references() {
        let db = Db::new_in_memory().await.unwrap();

        // Create tenant for FK constraint
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("Failed to create tenant");

        // Register adapter
        let params = crate::adapters::AdapterRegistrationBuilder::new()
            .adapter_id("adapter-1")
            .tenant_id(&tenant_id)
            .name("Adapter 1")
            .hash_b3("abc123")
            .rank(8)
            .tier("warm")
            .build()
            .unwrap();
        db.register_adapter(params).await.unwrap();

        // Create a stack referencing this adapter
        sqlx::query(
            "INSERT INTO adapter_stacks (id, tenant_id, name, adapter_ids_json, workflow_type, lifecycle_state)
             VALUES ('stack-1', ?, 'stack.test.stack1', '[\"adapter-1\"]', 'Sequential', 'active')"
        )
        .bind(&tenant_id)
        .execute(db.pool())
        .await
        .unwrap();

        // Check references
        let refs = db.check_active_stack_references("adapter-1").await.unwrap();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].stack_id, "stack-1");
        assert_eq!(refs[0].lifecycle_state, "active");
    }
}
