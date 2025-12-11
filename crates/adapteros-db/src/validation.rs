/// Metadata validation functions
///
/// This module provides validation for adapter and stack metadata changes,
/// enforcing lifecycle state transition rules and version format validation.
use crate::metadata::{validate_state_transition, validate_version, LifecycleState};
use crate::Db;
use adapteros_core::Result;
use serde_json::Value;
use sqlx::Row;
use std::str::FromStr;
use tracing::{debug, warn};

impl Db {
    /// Validate and update adapter lifecycle state
    ///
    /// This method enforces lifecycle state transition rules and tier-specific constraints.
    ///
    /// **Rules**:
    /// - State graph: draft → training → ready → active → deprecated → retired
    /// - Rollback: active → ready is permitted when rolling back a rollout
    /// - Failure: any state may transition to failed
    /// - Ephemeral adapters cannot be deprecated (must go directly to retired)
    /// - Retired and failed are terminal states (no transitions out)
    ///
    /// **Example**:
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// use adapteros_db::metadata::LifecycleState;
    /// db.update_adapter_lifecycle_state("adapter-123", LifecycleState::Deprecated).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_adapter_lifecycle_state(
        &self,
        adapter_id: &str,
        new_state: LifecycleState,
    ) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::{debug, warn};

        // Fetch current adapter state
        let adapter = self
            .get_adapter(adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        let current_state = LifecycleState::from_str(&adapter.lifecycle_state).map_err(|_| {
            AosError::Validation(format!(
                "Invalid current lifecycle state: {}",
                adapter.lifecycle_state
            ))
        })?;

        // Validate transition
        validate_state_transition(current_state, new_state, &adapter.tier).map_err(|e| {
            warn!(
                adapter_id = %adapter_id,
                current_state = ?current_state,
                new_state = ?new_state,
                tier = %adapter.tier,
                error = %e,
                "Lifecycle state transition validation failed"
            );
            AosError::PolicyViolation(format!(
                "Invalid lifecycle state transition for adapter '{}': {}",
                adapter_id, e
            ))
        })?;

        // Update state in database
        debug!(
            adapter_id = %adapter_id,
            current_state = ?current_state,
            new_state = ?new_state,
            "Updating adapter lifecycle state"
        );

        if matches!(
            new_state,
            LifecycleState::Ready
                | LifecycleState::Active
                | LifecycleState::Deprecated
                | LifecycleState::Retired
        ) {
            if adapter
                .aos_file_path
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true)
                || adapter
                    .aos_file_hash
                    .as_deref()
                    .map(str::is_empty)
                    .unwrap_or(true)
                || adapter
                    .content_hash_b3
                    .as_deref()
                    .map(str::is_empty)
                    .unwrap_or(true)
            {
                return Err(AosError::Validation(
                    "Immutable .aos artifact (path, hash, content hash) required before entering ready/active/deprecated/retired"
                        .to_string(),
                ));
            }
        }

        if matches!(new_state, LifecycleState::Active) {
            let snapshot_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM adapter_training_snapshots WHERE adapter_id = ? LIMIT 1",
            )
            .bind(adapter_id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to check training snapshot: {}", e)))?;

            if snapshot_exists.is_none() {
                return Err(AosError::Validation(
                    "Active state requires a training snapshot/metrics evidence".to_string(),
                ));
            }

            if let Some(repo_id) = adapter.repo_id.as_deref() {
                let requested_branch = branch_from_metadata(&adapter.metadata_json);
                let rows = sqlx::query("SELECT adapter_id, metadata_json FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'")
                    .bind(repo_id)
                    .fetch_all(&*self.pool())
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
                        (Some(_), None) => true,
                        (None, _) => true,
                    };
                    if branches_conflict {
                        return Err(AosError::Validation(format!(
                            "Active state requires uniqueness per repo/branch; adapter {} is already active for repo {}",
                            other_id, repo_id
                        )));
                    }
                }
            }
        }

        sqlx::query(
            "UPDATE adapters SET lifecycle_state = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(new_state.as_str())
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle state: {}", e)))?;

        Ok(())
    }

    /// Validate and update adapter version
    ///
    /// This method validates version format (SemVer or monotonic) before updating.
    ///
    /// **Example**:
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// db.update_adapter_version("adapter-123", "2.0.0").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_adapter_version(&self, adapter_id: &str, new_version: &str) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::debug;

        // Validate version format
        validate_version(new_version).map_err(|e| {
            AosError::Validation(format!(
                "Invalid version format for adapter '{}': {}",
                adapter_id, e
            ))
        })?;

        // Update version in database
        debug!(
            adapter_id = %adapter_id,
            new_version = %new_version,
            "Updating adapter version"
        );

        sqlx::query(
            "UPDATE adapters SET version = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(new_version)
        .bind(adapter_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update version: {}", e)))?;

        Ok(())
    }

    /// Validate and update stack lifecycle state
    ///
    /// Similar to adapter lifecycle state updates but for stacks.
    pub async fn update_stack_lifecycle_state(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_state: LifecycleState,
    ) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::{debug, warn};

        // Fetch current stack state
        let stack = self
            .get_stack(tenant_id, stack_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Stack not found: {}", stack_id)))?;

        let current_state = LifecycleState::from_str(&stack.lifecycle_state).map_err(|_| {
            AosError::Validation(format!(
                "Invalid current lifecycle state: {}",
                stack.lifecycle_state
            ))
        })?;

        // Validate transition (stacks don't have tier restrictions)
        if !current_state.can_transition_to(new_state) {
            warn!(
                stack_id = %stack_id,
                current_state = ?current_state,
                new_state = ?new_state,
                "Stack lifecycle state transition validation failed"
            );
            return Err(AosError::PolicyViolation(format!(
                "Invalid lifecycle state transition for stack '{}': {} -> {}",
                stack_id,
                current_state.as_str(),
                new_state.as_str()
            ))
            .into());
        }

        // Update state in database
        debug!(
            stack_id = %stack_id,
            current_state = ?current_state,
            new_state = ?new_state,
            "Updating stack lifecycle state"
        );

        // SQL update (always happens)
        sqlx::query(
            "UPDATE adapter_stacks SET lifecycle_state = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?",
        )
        .bind(new_state.as_str())
        .bind(stack_id)
        .bind(tenant_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle state: {}", e)))?;

        // KV update (dual-write mode)
        if let Some(kv_backend) = self.get_stack_kv_repo() {
            // Convert adapteros_core::LifecycleState to adapteros_storage::entities::stack::LifecycleState
            use adapteros_storage::entities::stack::LifecycleState as KvLifecycleState;
            let kv_state = KvLifecycleState::from_str(new_state.as_str()).ok_or_else(|| {
                adapteros_core::AosError::Database(format!(
                    "Invalid lifecycle state: {}",
                    new_state.as_str()
                ))
            })?;

            if let Err(e) = kv_backend
                .update_lifecycle_state(tenant_id, stack_id, kv_state)
                .await
            {
                warn!(error = %e, stack_id = %stack_id, "Failed to update stack lifecycle state in KV backend (dual-write)");
            } else {
                debug!(stack_id = %stack_id, state = ?new_state, "Stack lifecycle state updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Validate and update stack version
    pub async fn update_stack_version(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_version: &str,
    ) -> Result<()> {
        use adapteros_core::AosError;
        use tracing::debug;

        // Validate version format
        validate_version(new_version).map_err(|e| {
            AosError::Validation(format!(
                "Invalid version format for stack '{}': {}",
                stack_id, e
            ))
        })?;

        // Update version in database
        debug!(
            stack_id = %stack_id,
            new_version = %new_version,
            "Updating stack version"
        );

        // SQL update (always happens)
        sqlx::query(
            "UPDATE adapter_stacks SET version = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?",
        )
        .bind(new_version)
        .bind(stack_id)
        .bind(tenant_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update version: {}", e)))?;

        // KV update (dual-write mode)
        if let Some(kv_backend) = self.get_stack_kv_repo() {
            if let Err(e) = kv_backend
                .update_version(tenant_id, stack_id, new_version)
                .await
            {
                warn!(error = %e, stack_id = %stack_id, "Failed to update stack version in KV backend (dual-write)");
            } else {
                debug!(stack_id = %stack_id, version = %new_version, "Stack version updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Validate adapter metadata before registration
    ///
    /// This method checks for illegal field combinations before an adapter is registered.
    ///
    /// **Checks**:
    /// - If tier=ephemeral, lifecycle_state cannot be deprecated
    /// - Version format must be valid (SemVer or monotonic)
    pub fn validate_adapter_metadata(
        tier: &str,
        lifecycle_state: &str,
        version: &str,
    ) -> Result<()> {
        use adapteros_core::AosError;

        // Validate lifecycle state
        let state = LifecycleState::from_str(lifecycle_state).map_err(|_| {
            AosError::Validation(format!("Invalid lifecycle state: {}", lifecycle_state))
        })?;

        // Check tier-specific rules
        if !state.is_valid_for_tier(tier) {
            return Err(AosError::PolicyViolation(format!(
                "Lifecycle state '{}' is not valid for tier '{}'",
                lifecycle_state, tier
            ))
            .into());
        }

        // Validate version format
        validate_version(version).map_err(|e| AosError::Validation(e.to_string()))?;

        Ok(())
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

    #[test]
    fn test_validate_adapter_metadata_valid() {
        // Valid combinations
        assert!(Db::validate_adapter_metadata("persistent", "active", "1.0.0").is_ok());
        assert!(Db::validate_adapter_metadata("ephemeral", "active", "42").is_ok());
        assert!(Db::validate_adapter_metadata("warm", "deprecated", "2.1.3").is_ok());
    }

    #[test]
    fn test_validate_adapter_metadata_invalid_tier_state() {
        // ephemeral + deprecated is invalid
        let result = Db::validate_adapter_metadata("ephemeral", "deprecated", "1.0.0");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not valid for tier"));
    }

    #[test]
    fn test_validate_adapter_metadata_invalid_version() {
        // Invalid version format
        let result = Db::validate_adapter_metadata("persistent", "active", "invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_validate_adapter_metadata_invalid_state() {
        // Invalid lifecycle state
        let result = Db::validate_adapter_metadata("persistent", "unknown_state", "1.0.0");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid lifecycle state"));
    }
}
