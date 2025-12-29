/// Metadata validation functions
///
/// This module provides validation for adapter and stack metadata changes,
/// enforcing lifecycle state transition rules and version format validation.
///
/// # Single-Active Adapter Enforcement
///
/// This module enforces that only one adapter can be active per repository at a time
/// (scoped by branch). This prevents conflicts from multiple active adapters serving
/// the same codebase.
///
/// ## Enforcement Rules
///
/// 1. **Per-Repository/Branch Uniqueness**: Only one adapter may be in 'active' state
///    for a given (repo_id, branch) combination at any time.
///
/// 2. **Branch Resolution**: The branch is extracted from adapter metadata_json
///    (fields: "branch" or "git_branch"). If no branch is specified, the adapter
///    is treated as targeting all branches (conservative conflict detection).
///
/// 3. **Conflict Detection**: When activating an adapter:
///    - If the new adapter has no branch specified, it conflicts with ALL active adapters on that repo
///    - If the existing active adapter has no branch specified, it conflicts with any new activation
///    - If both have branches specified, conflict only if branches match
///
/// 4. **Resolution**: Callers must explicitly deprecate or deactivate conflicting adapters
///    before activating a new one.
use crate::lifecycle_rules::{
    LifecycleRuleEvaluation, LifecycleRuleFilter, LifecycleRuleType, TransitionValidationResult,
};
use crate::metadata::{validate_state_transition, validate_version, LifecycleState};
use crate::Db;
use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::str::FromStr;
use tracing::{debug, info, warn};

/// Result of single-active adapter validation
#[derive(Debug, Clone)]
pub struct SingleActiveValidationResult {
    /// Whether the validation passed (no conflicts)
    pub is_valid: bool,
    /// List of conflicting adapter IDs if validation failed
    pub conflicting_adapters: Vec<String>,
    /// Human-readable explanation of the conflict
    pub conflict_reason: Option<String>,
}

impl SingleActiveValidationResult {
    /// Create a successful validation result (no conflicts)
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            conflicting_adapters: Vec::new(),
            conflict_reason: None,
        }
    }

    /// Create a failed validation result with conflicts
    pub fn conflict(adapters: Vec<String>, reason: String) -> Self {
        Self {
            is_valid: false,
            conflicting_adapters: adapters,
            conflict_reason: Some(reason),
        }
    }
}

/// A lifecycle transition event representing a state change in the audit trail.
///
/// This struct captures the complete context of a lifecycle state transition
/// for audit and debugging purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransitionEvent {
    /// The adapter ID that underwent the transition
    pub adapter_id: String,
    /// The previous lifecycle state before the transition
    pub from_state: String,
    /// The new lifecycle state after the transition
    pub to_state: String,
    /// Optional reason describing why the transition occurred
    pub reason: Option<String>,
    /// Optional identifier of who/what initiated the transition (user, system, etc.)
    pub initiated_by: Option<String>,
    /// Timestamp when the transition occurred
    pub timestamp: DateTime<Utc>,
}

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
        #[allow(deprecated)]
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
        ) && (adapter
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
                .unwrap_or(true))
        {
            return Err(AosError::Validation(
                "Immutable .aos artifact (path, hash, content hash) required before entering ready/active/deprecated/retired"
                    .to_string(),
            ));
        }

        if matches!(new_state, LifecycleState::Active) {
            let snapshot_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM adapter_training_snapshots WHERE adapter_id = ? LIMIT 1",
            )
            .bind(adapter_id)
            .fetch_optional(self.pool())
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
                    .fetch_all(self.pool())
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle state: {}", e)))?;

        info!(
            adapter_id = %adapter_id,
            from_state = %current_state.as_str(),
            to_state = %new_state.as_str(),
            "Lifecycle state transition completed"
        );

        Ok(())
    }

    /// Get lifecycle transition history for an adapter
    ///
    /// Returns all lifecycle transitions for the specified adapter,
    /// ordered by timestamp (newest first).
    ///
    /// **Example**:
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let history = db.get_adapter_lifecycle_history("adapter-123").await?;
    /// for event in history {
    ///     println!("{} -> {} at {}", event.from_state, event.to_state, event.timestamp);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    /// Get adapter lifecycle history as transition events (validation-centric view)
    pub async fn get_adapter_lifecycle_transitions(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<LifecycleTransitionEvent>> {
        let rows = sqlx::query(
            "SELECT
                a.adapter_id,
                alh.previous_lifecycle_state,
                alh.lifecycle_state,
                alh.reason,
                alh.initiated_by,
                alh.created_at
             FROM adapter_lifecycle_history alh
             JOIN adapters a ON alh.adapter_pk = a.id
             WHERE a.adapter_id = ?
             ORDER BY alh.created_at DESC",
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query lifecycle history: {}", e)))?;

        let events = rows
            .into_iter()
            .map(|row| {
                let adapter_id: String = row.get(0);
                let from_state: Option<String> = row.get(1);
                let to_state: String = row.get(2);
                let reason: Option<String> = row.get(3);
                let initiated_by: Option<String> = row.get(4);
                let created_at: String = row.get(5);

                // Parse the timestamp, falling back to current time if parsing fails
                let timestamp = DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|_| {
                        // Try SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
                        chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                            .map(|ndt| ndt.and_utc())
                    })
                    .unwrap_or_else(|_| Utc::now());

                LifecycleTransitionEvent {
                    adapter_id,
                    from_state: from_state.unwrap_or_else(|| "unknown".to_string()),
                    to_state,
                    reason,
                    initiated_by,
                    timestamp,
                }
            })
            .collect();

        Ok(events)
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
        .execute(self.pool())
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle state: {}", e)))?;

        // KV update (dual-write mode)
        if let Some(kv_backend) = self.get_stack_kv_repo() {
            // Convert adapteros_core::LifecycleState to adapteros_storage::entities::stack::LifecycleState
            use adapteros_storage::entities::stack::LifecycleState as KvLifecycleState;
            let kv_state = KvLifecycleState::parse_state(new_state.as_str()).ok_or_else(|| {
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
        .execute(self.pool())
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

    /// Validate single-active adapter constraint for a repository
    ///
    /// This method checks whether activating an adapter would violate the
    /// single-active-per-repository constraint. It returns detailed information
    /// about any conflicts found.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `repo_id` - The repository ID to check against
    /// * `requested_branch` - Optional branch from the adapter's metadata
    ///
    /// # Returns
    /// A `SingleActiveValidationResult` indicating whether activation is allowed
    /// and listing any conflicting adapters if not.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let result = db.validate_single_active_per_repo(
    ///     "adapter-456",
    ///     "repo-123",
    ///     Some("main".to_string()),
    /// ).await?;
    ///
    /// if !result.is_valid {
    ///     println!("Conflict with adapters: {:?}", result.conflicting_adapters);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn validate_single_active_per_repo(
        &self,
        adapter_id: &str,
        repo_id: &str,
        requested_branch: Option<String>,
    ) -> Result<SingleActiveValidationResult> {
        let rows = sqlx::query(
            "SELECT adapter_id, metadata_json FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters: {}", e)))?;

        let mut conflicting_adapters = Vec::new();

        for row in rows {
            let other_id: String = row.get(0);
            if other_id == adapter_id {
                // Skip self - re-activation is not a conflict
                continue;
            }

            let other_metadata: Option<String> = row.get(1);
            let other_branch = branch_from_metadata(&other_metadata);

            // Determine if branches conflict
            let branches_conflict = match (&requested_branch, &other_branch) {
                // Both have explicit branches - conflict only if they match
                (Some(req), Some(other)) => req == other,
                // New adapter has no branch - conflicts with any existing active
                (None, _) => true,
                // Existing has no branch - conflicts with any new activation
                (Some(_), None) => true,
            };

            if branches_conflict {
                debug!(
                    adapter_id = %adapter_id,
                    conflicting_adapter = %other_id,
                    repo_id = %repo_id,
                    requested_branch = ?requested_branch,
                    other_branch = ?other_branch,
                    "Single-active conflict detected"
                );
                conflicting_adapters.push(other_id);
            }
        }

        if conflicting_adapters.is_empty() {
            Ok(SingleActiveValidationResult::valid())
        } else {
            let branch_desc = requested_branch
                .as_deref()
                .unwrap_or("(unspecified branch)");
            let reason = format!(
                "Cannot activate adapter '{}' for repo '{}' on branch '{}': \
                 adapter(s) {} already active. Only one adapter can be active per repository/branch.",
                adapter_id,
                repo_id,
                branch_desc,
                conflicting_adapters.join(", ")
            );

            warn!(
                adapter_id = %adapter_id,
                repo_id = %repo_id,
                conflicting_count = conflicting_adapters.len(),
                "Single-active adapter validation failed"
            );

            Ok(SingleActiveValidationResult::conflict(
                conflicting_adapters,
                reason,
            ))
        }
    }

    /// Enforce single-active adapter constraint, returning an error on violation
    ///
    /// This is a convenience wrapper around `validate_single_active_per_repo` that
    /// returns an error if the constraint would be violated, making it suitable
    /// for use in activation workflows.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `repo_id` - The repository ID to check against
    /// * `requested_branch` - Optional branch from the adapter's metadata
    ///
    /// # Errors
    /// Returns `AosError::Validation` if another adapter is already active for
    /// the same repository/branch combination.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// // This will return an error if a conflict exists
    /// db.enforce_single_active_per_repo(
    ///     "adapter-456",
    ///     "repo-123",
    ///     Some("main".to_string()),
    /// ).await?;
    ///
    /// // Safe to activate the adapter now
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enforce_single_active_per_repo(
        &self,
        adapter_id: &str,
        repo_id: &str,
        requested_branch: Option<String>,
    ) -> Result<()> {
        let result = self
            .validate_single_active_per_repo(adapter_id, repo_id, requested_branch)
            .await?;

        if !result.is_valid {
            return Err(AosError::Validation(result.conflict_reason.unwrap_or_else(
                || format!("Single-active constraint violated for repo '{}'", repo_id),
            )));
        }

        Ok(())
    }

    /// List all active adapters for a given repository
    ///
    /// Useful for debugging and understanding the current active adapter state
    /// for a repository before making changes.
    ///
    /// # Arguments
    /// * `repo_id` - The repository ID to query
    ///
    /// # Returns
    /// A vector of tuples containing (adapter_id, branch) for each active adapter.
    /// Branch may be None if not specified in the adapter's metadata.
    pub async fn list_active_adapters_for_repo(
        &self,
        repo_id: &str,
    ) -> Result<Vec<(String, Option<String>)>> {
        let rows = sqlx::query(
            "SELECT adapter_id, metadata_json FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list active adapters: {}", e)))?;

        let results: Vec<(String, Option<String>)> = rows
            .into_iter()
            .map(|row| {
                let adapter_id: String = row.get(0);
                let metadata: Option<String> = row.get(1);
                let branch = branch_from_metadata(&metadata);
                (adapter_id, branch)
            })
            .collect();

        info!(
            repo_id = %repo_id,
            active_count = results.len(),
            "Listed active adapters for repository"
        );

        Ok(results)
    }

    /// Check if a repository has any active adapters
    ///
    /// A quick check to determine if single-active enforcement might apply.
    ///
    /// # Arguments
    /// * `repo_id` - The repository ID to check
    ///
    /// # Returns
    /// `true` if at least one adapter is active for this repository
    pub async fn has_active_adapter_for_repo(&self, repo_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters: {}", e)))?;

        Ok(count > 0)
    }

    /// Validate single-active adapter constraint by repository path
    ///
    /// This method checks whether activating an adapter would violate the
    /// single-active constraint based on the repository path (file system location).
    /// This is useful when adapters may target the same repository via different
    /// identifiers but the same physical location.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `repo_path` - The canonicalized repository path to check against
    /// * `requested_branch` - Optional branch from the adapter's metadata
    ///
    /// # Returns
    /// A `SingleActiveValidationResult` indicating whether activation is allowed.
    pub async fn validate_single_active_per_repo_path(
        &self,
        adapter_id: &str,
        repo_path: &str,
        requested_branch: Option<String>,
    ) -> Result<SingleActiveValidationResult> {
        let rows = sqlx::query(
            "SELECT adapter_id, metadata_json FROM adapters WHERE repo_path = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_path)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters by path: {}", e)))?;

        let mut conflicting_adapters = Vec::new();

        for row in rows {
            let other_id: String = row.get(0);
            if other_id == adapter_id {
                continue;
            }

            let other_metadata: Option<String> = row.get(1);
            let other_branch = branch_from_metadata(&other_metadata);

            let branches_conflict = match (&requested_branch, &other_branch) {
                (Some(req), Some(other)) => req == other,
                (None, _) => true,
                (Some(_), None) => true,
            };

            if branches_conflict {
                debug!(
                    adapter_id = %adapter_id,
                    conflicting_adapter = %other_id,
                    repo_path = %repo_path,
                    requested_branch = ?requested_branch,
                    other_branch = ?other_branch,
                    "Single-active conflict detected (by repo_path)"
                );
                conflicting_adapters.push(other_id);
            }
        }

        if conflicting_adapters.is_empty() {
            Ok(SingleActiveValidationResult::valid())
        } else {
            let branch_desc = requested_branch
                .as_deref()
                .unwrap_or("(unspecified branch)");
            let reason = format!(
                "Cannot activate adapter '{}' for repo path '{}' on branch '{}': \
                 adapter(s) {} already active. Only one adapter can be active per repository path/branch.",
                adapter_id,
                repo_path,
                branch_desc,
                conflicting_adapters.join(", ")
            );

            warn!(
                adapter_id = %adapter_id,
                repo_path = %repo_path,
                conflicting_count = conflicting_adapters.len(),
                "Single-active adapter validation failed (by repo_path)"
            );

            Ok(SingleActiveValidationResult::conflict(
                conflicting_adapters,
                reason,
            ))
        }
    }

    /// Enforce single-active adapter constraint by repository path
    ///
    /// Returns an error if another adapter is already active for the same
    /// repository path/branch combination.
    pub async fn enforce_single_active_per_repo_path(
        &self,
        adapter_id: &str,
        repo_path: &str,
        requested_branch: Option<String>,
    ) -> Result<()> {
        let result = self
            .validate_single_active_per_repo_path(adapter_id, repo_path, requested_branch)
            .await?;

        if !result.is_valid {
            return Err(AosError::Validation(
                result.conflict_reason.unwrap_or_else(|| {
                    format!(
                        "Single-active constraint violated for repo path '{}'",
                        repo_path
                    )
                }),
            ));
        }

        Ok(())
    }

    /// Comprehensive active uniqueness enforcement
    ///
    /// This method enforces active adapter uniqueness across multiple dimensions:
    /// 1. By `repo_id` + `branch` (if repo_id is set)
    /// 2. By `repo_path` + `branch` (if repo_path is set)
    ///
    /// Both constraints must pass for activation to be allowed. This prevents
    /// scenarios where the same repository is referenced by different identifiers.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `repo_id` - Optional repository ID
    /// * `repo_path` - Optional repository file system path
    /// * `requested_branch` - Optional branch from the adapter's metadata
    ///
    /// # Returns
    /// A combined `SingleActiveValidationResult` with all conflicts.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let result = db.validate_active_uniqueness(
    ///     "adapter-456",
    ///     Some("repo-123".to_string()),
    ///     Some("/path/to/repo".to_string()),
    ///     Some("main".to_string()),
    /// ).await?;
    ///
    /// if !result.is_valid {
    ///     println!("Conflicts: {:?}", result.conflicting_adapters);
    ///     println!("Reason: {:?}", result.conflict_reason);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn validate_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        requested_branch: Option<String>,
    ) -> Result<SingleActiveValidationResult> {
        let mut all_conflicts = Vec::new();
        let mut reasons = Vec::new();

        // Check by repo_id if available
        if let Some(ref rid) = repo_id {
            let result = self
                .validate_single_active_per_repo(adapter_id, rid, requested_branch.clone())
                .await?;
            if !result.is_valid {
                all_conflicts.extend(result.conflicting_adapters);
                if let Some(reason) = result.conflict_reason {
                    reasons.push(reason);
                }
            }
        }

        // Check by repo_path if available
        if let Some(ref rpath) = repo_path {
            let result = self
                .validate_single_active_per_repo_path(adapter_id, rpath, requested_branch.clone())
                .await?;
            if !result.is_valid {
                // Deduplicate conflicts (same adapter might conflict on both dimensions)
                for conflict in result.conflicting_adapters {
                    if !all_conflicts.contains(&conflict) {
                        all_conflicts.push(conflict);
                    }
                }
                if let Some(reason) = result.conflict_reason {
                    reasons.push(reason);
                }
            }
        }

        if all_conflicts.is_empty() {
            Ok(SingleActiveValidationResult::valid())
        } else {
            let combined_reason = if reasons.len() == 1 {
                reasons.into_iter().next().unwrap()
            } else {
                format!(
                    "Multiple active uniqueness violations:\n{}",
                    reasons.join("\n")
                )
            };

            info!(
                adapter_id = %adapter_id,
                conflict_count = all_conflicts.len(),
                repo_id = ?repo_id,
                repo_path = ?repo_path,
                "Active uniqueness validation failed"
            );

            Ok(SingleActiveValidationResult::conflict(
                all_conflicts,
                combined_reason,
            ))
        }
    }

    /// Enforce comprehensive active uniqueness constraints
    ///
    /// Returns an error if any active uniqueness constraint would be violated.
    /// This is the recommended method to call before activating an adapter.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// // This will return an error if any conflict exists
    /// db.enforce_active_uniqueness(
    ///     "adapter-456",
    ///     Some("repo-123".to_string()),
    ///     Some("/path/to/repo".to_string()),
    ///     Some("main".to_string()),
    /// ).await?;
    ///
    /// // Safe to activate the adapter now
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enforce_active_uniqueness(
        &self,
        adapter_id: &str,
        repo_id: Option<String>,
        repo_path: Option<String>,
        requested_branch: Option<String>,
    ) -> Result<()> {
        let result = self
            .validate_active_uniqueness(adapter_id, repo_id.clone(), repo_path.clone(), requested_branch)
            .await?;

        if !result.is_valid {
            return Err(AosError::Validation(
                result.conflict_reason.unwrap_or_else(|| {
                    format!(
                        "Active uniqueness constraint violated (repo_id: {:?}, repo_path: {:?})",
                        repo_id, repo_path
                    )
                }),
            ));
        }

        Ok(())
    }

    /// Check if a repository path has any active adapters
    ///
    /// A quick check to determine if single-active enforcement might apply
    /// based on file system path.
    ///
    /// # Arguments
    /// * `repo_path` - The repository path to check
    ///
    /// # Returns
    /// `true` if at least one adapter is active for this repository path
    pub async fn has_active_adapter_for_repo_path(&self, repo_path: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM adapters WHERE repo_path = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_path)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters by path: {}", e)))?;

        Ok(count > 0)
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

    #[test]
    fn test_single_active_validation_result_valid() {
        let result = SingleActiveValidationResult::valid();
        assert!(result.is_valid);
        assert!(result.conflicting_adapters.is_empty());
        assert!(result.conflict_reason.is_none());
    }

    #[test]
    fn test_single_active_validation_result_conflict() {
        let result = SingleActiveValidationResult::conflict(
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
            "Test conflict reason".to_string(),
        );
        assert!(!result.is_valid);
        assert_eq!(result.conflicting_adapters.len(), 2);
        assert_eq!(
            result.conflict_reason,
            Some("Test conflict reason".to_string())
        );
    }

    #[test]
    fn test_branch_from_metadata_with_branch() {
        let metadata = Some(r#"{"branch": "main", "other": "value"}"#.to_string());
        let branch = branch_from_metadata(&metadata);
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn test_branch_from_metadata_with_git_branch() {
        let metadata = Some(r#"{"git_branch": "feature/test"}"#.to_string());
        let branch = branch_from_metadata(&metadata);
        assert_eq!(branch, Some("feature/test".to_string()));
    }

    #[test]
    fn test_branch_from_metadata_prefers_branch_over_git_branch() {
        let metadata = Some(r#"{"branch": "main", "git_branch": "develop"}"#.to_string());
        let branch = branch_from_metadata(&metadata);
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn test_branch_from_metadata_none() {
        let metadata: Option<String> = None;
        let branch = branch_from_metadata(&metadata);
        assert!(branch.is_none());
    }

    #[test]
    fn test_branch_from_metadata_no_branch_field() {
        let metadata = Some(r#"{"other": "value"}"#.to_string());
        let branch = branch_from_metadata(&metadata);
        assert!(branch.is_none());
    }

    #[test]
    fn test_branch_from_metadata_invalid_json() {
        let metadata = Some("not valid json".to_string());
        let branch = branch_from_metadata(&metadata);
        assert!(branch.is_none());
    }
}
