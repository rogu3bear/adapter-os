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
use crate::metadata::{validate_version, LifecycleState};
use crate::Db;
use adapteros_core::lifecycle::{
    validate_transition_with_context, PreflightStatus, ValidationContext,
};
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

        let enforcement = self
            .enforce_lifecycle_transition(
                adapter_id,
                current_state.as_str(),
                new_state.as_str(),
                &adapter.tier,
                LifecycleEnforcementOptions {
                    fail_on_warnings: true,
                    ..Default::default()
                },
            )
            .await?;

        if !enforcement.allowed {
            let reason = enforcement
                .denial_reason
                .unwrap_or_else(|| "transition denied".to_string());
            warn!(
                adapter_id = %adapter_id,
                current_state = ?current_state,
                new_state = ?new_state,
                tier = %adapter.tier,
                reason = %reason,
                "Lifecycle state transition validation failed"
            );
            return Err(AosError::PolicyViolation(format!(
                "Invalid lifecycle state transition for adapter '{}': {}",
                adapter_id, reason
            )));
        }

        if !enforcement.warnings.is_empty() {
            warn!(
                adapter_id = %adapter_id,
                warnings = ?enforcement.warnings,
                "Lifecycle transition allowed with warnings"
            );
        }

        // Update state in database
        debug!(
            adapter_id = %adapter_id,
            current_state = ?current_state,
            new_state = ?new_state,
            "Updating adapter lifecycle state"
        );

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
            return Err(AosError::Validation(result.conflict_reason.unwrap_or_else(
                || {
                    format!(
                        "Single-active constraint violated for repo path '{}'",
                        repo_path
                    )
                },
            )));
        }

        Ok(())
    }

    /// Comprehensive active uniqueness enforcement
    ///
    /// This method enforces active adapter uniqueness across multiple dimensions:
    /// 1. By `repo_id` + `branch` (if repo_id is set)
    /// 2. By `repo_path` + `branch` (if repo_path is set)
    /// 3. By `codebase_scope` + `branch` (if codebase_scope is set)
    ///
    /// All constraints must pass for activation to be allowed. This prevents
    /// scenarios where the same repository is referenced by different identifiers
    /// or the same codebase is served by multiple active adapters.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `repo_id` - Optional repository ID
    /// * `repo_path` - Optional repository file system path
    /// * `codebase_scope` - Optional codebase scope identifier (for codebase adapters)
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
    ///     Some("github.com/myorg/myrepo".to_string()),
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
        codebase_scope: Option<String>,
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

        // Check by codebase_scope if available (for codebase adapters)
        if let Some(ref scope) = codebase_scope {
            let result = self
                .validate_single_active_per_codebase_scope(
                    adapter_id,
                    scope,
                    requested_branch.clone(),
                )
                .await?;
            if !result.is_valid {
                // Deduplicate conflicts
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
                codebase_scope = ?codebase_scope,
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
    ///     Some("github.com/myorg/myrepo".to_string()),
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
        codebase_scope: Option<String>,
        requested_branch: Option<String>,
    ) -> Result<()> {
        let result = self
            .validate_active_uniqueness(
                adapter_id,
                repo_id.clone(),
                repo_path.clone(),
                codebase_scope.clone(),
                requested_branch,
            )
            .await?;

        if !result.is_valid {
            return Err(AosError::Validation(
                result.conflict_reason.unwrap_or_else(|| {
                    format!(
                        "Active uniqueness constraint violated (repo_id: {:?}, repo_path: {:?}, codebase_scope: {:?})",
                        repo_id, repo_path, codebase_scope
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
        .map_err(|e| {
            AosError::Database(format!("Failed to check active adapters by path: {}", e))
        })?;

        Ok(count > 0)
    }

    // ========================================================================
    // Codebase Adapter Active Uniqueness
    // ========================================================================

    /// Validate single-active adapter constraint by codebase scope
    ///
    /// For codebase adapters (adapters trained on a specific codebase), this method
    /// ensures that only one adapter can be active per codebase_scope + branch
    /// combination. This prevents conflicts when multiple adapters are trained
    /// on the same codebase but should not be served simultaneously.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `codebase_scope` - The codebase scope identifier (e.g., "github.com/org/repo")
    /// * `requested_branch` - Optional branch from the adapter's metadata
    ///
    /// # Returns
    /// A `SingleActiveValidationResult` indicating whether activation is allowed.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let result = db.validate_single_active_per_codebase_scope(
    ///     "adapter-456",
    ///     "github.com/myorg/myrepo",
    ///     Some("main".to_string()),
    /// ).await?;
    ///
    /// if !result.is_valid {
    ///     println!("Conflict with adapters: {:?}", result.conflicting_adapters);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn validate_single_active_per_codebase_scope(
        &self,
        adapter_id: &str,
        codebase_scope: &str,
        requested_branch: Option<String>,
    ) -> Result<SingleActiveValidationResult> {
        let rows = sqlx::query(
            "SELECT adapter_id, metadata_json FROM adapters WHERE codebase_scope = ? AND lifecycle_state = 'active'",
        )
        .bind(codebase_scope)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters by codebase_scope: {}", e)))?;

        let mut conflicting_adapters = Vec::new();

        for row in rows {
            let other_id: String = row.get(0);
            if other_id == adapter_id {
                // Skip self - re-activation is not a conflict
                continue;
            }

            let other_metadata: Option<String> = row.get(1);
            let other_branch = branch_from_metadata(&other_metadata);

            // Determine if branches conflict using the same rules as repo-based validation
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
                    codebase_scope = %codebase_scope,
                    requested_branch = ?requested_branch,
                    other_branch = ?other_branch,
                    "Single-active conflict detected (by codebase_scope)"
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
                "Cannot activate adapter '{}' for codebase scope '{}' on branch '{}': \
                 adapter(s) {} already active. Only one codebase adapter can be active per scope/branch.",
                adapter_id,
                codebase_scope,
                branch_desc,
                conflicting_adapters.join(", ")
            );

            warn!(
                adapter_id = %adapter_id,
                codebase_scope = %codebase_scope,
                conflicting_count = conflicting_adapters.len(),
                "Single-active codebase adapter validation failed"
            );

            Ok(SingleActiveValidationResult::conflict(
                conflicting_adapters,
                reason,
            ))
        }
    }

    /// Enforce single-active adapter constraint by codebase scope
    ///
    /// Returns an error if another adapter is already active for the same
    /// codebase_scope/branch combination.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// db.enforce_single_active_per_codebase_scope(
    ///     "adapter-456",
    ///     "github.com/myorg/myrepo",
    ///     Some("main".to_string()),
    /// ).await?;
    /// // Safe to activate the adapter now
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enforce_single_active_per_codebase_scope(
        &self,
        adapter_id: &str,
        codebase_scope: &str,
        requested_branch: Option<String>,
    ) -> Result<()> {
        let result = self
            .validate_single_active_per_codebase_scope(adapter_id, codebase_scope, requested_branch)
            .await?;

        if !result.is_valid {
            return Err(AosError::Validation(result.conflict_reason.unwrap_or_else(
                || {
                    format!(
                        "Single-active constraint violated for codebase scope '{}'",
                        codebase_scope
                    )
                },
            )));
        }

        Ok(())
    }

    /// Check if a codebase scope has any active adapters
    ///
    /// A quick check to determine if single-active enforcement might apply
    /// based on codebase scope.
    ///
    /// # Arguments
    /// * `codebase_scope` - The codebase scope to check
    ///
    /// # Returns
    /// `true` if at least one adapter is active for this codebase scope
    pub async fn has_active_adapter_for_codebase_scope(
        &self,
        codebase_scope: &str,
    ) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM adapters WHERE codebase_scope = ? AND lifecycle_state = 'active'",
        )
        .bind(codebase_scope)
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to check active adapters by codebase_scope: {}",
                e
            ))
        })?;

        Ok(count > 0)
    }

    /// List all active adapters for a given codebase scope
    ///
    /// Useful for debugging and understanding the current active adapter state
    /// for a codebase scope before making changes.
    ///
    /// # Arguments
    /// * `codebase_scope` - The codebase scope to query
    ///
    /// # Returns
    /// A vector of tuples containing (adapter_id, branch) for each active adapter.
    /// Branch may be None if not specified in the adapter's metadata.
    pub async fn list_active_adapters_for_codebase_scope(
        &self,
        codebase_scope: &str,
    ) -> Result<Vec<(String, Option<String>)>> {
        let rows = sqlx::query(
            "SELECT adapter_id, metadata_json FROM adapters WHERE codebase_scope = ? AND lifecycle_state = 'active'",
        )
        .bind(codebase_scope)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list active adapters by codebase_scope: {}", e)))?;

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
            codebase_scope = %codebase_scope,
            active_count = results.len(),
            "Listed active adapters for codebase scope"
        );

        Ok(results)
    }

    // ========================================================================
    // Active Uniqueness Validation Utilities
    // ========================================================================

    /// Validate that activating an adapter won't violate uniqueness constraints
    ///
    /// This is a comprehensive pre-activation check that validates:
    /// 1. Single-active-per-repo/branch constraint
    /// 2. Training snapshot existence
    /// 3. Artifact readiness (aos_file_path, aos_file_hash, content_hash_b3)
    ///
    /// Use this before transitioning an adapter to Active state to get detailed
    /// validation errors.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to validate for activation
    ///
    /// # Returns
    /// A list of validation errors, or empty if validation passes.
    pub async fn validate_adapter_activation_readiness(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        // Fetch the adapter
        #[allow(deprecated)]
        let adapter = match self.get_adapter(adapter_id).await? {
            Some(a) => a,
            None => {
                errors.push(format!("Adapter not found: {}", adapter_id));
                return Ok(errors);
            }
        };

        // Check artifact readiness
        if adapter
            .aos_file_path
            .as_deref()
            .map(str::is_empty)
            .unwrap_or(true)
        {
            errors.push(
                "Missing aos_file_path: immutable artifact path required for activation"
                    .to_string(),
            );
        }

        if adapter
            .aos_file_hash
            .as_deref()
            .map(str::is_empty)
            .unwrap_or(true)
        {
            errors.push(
                "Missing aos_file_hash: immutable artifact hash required for activation"
                    .to_string(),
            );
        }

        if adapter
            .content_hash_b3
            .as_deref()
            .map(str::is_empty)
            .unwrap_or(true)
        {
            errors
                .push("Missing content_hash_b3: content hash required for activation".to_string());
        }

        // Check training snapshot exists
        let snapshot_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM adapter_training_snapshots WHERE adapter_id = ? LIMIT 1",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check training snapshot: {}", e)))?;

        if snapshot_exists.is_none() {
            errors.push(
                "Missing training snapshot: metrics evidence required for activation".to_string(),
            );
        }

        let requested_branch = branch_from_metadata(&adapter.metadata_json);
        let mut repo_id = adapter.repo_id.clone();
        if repo_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            repo_id = repo_id_from_metadata(&adapter.metadata_json);
        }
        let mut repo_path = adapter.repo_path.clone();
        if repo_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            repo_path = repo_path_from_metadata(&adapter.metadata_json);
        }
        let mut codebase_scope = adapter.codebase_scope.clone();
        if codebase_scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            codebase_scope = codebase_scope_from_metadata(&adapter.metadata_json);
        }
        let uniqueness = self
            .validate_active_uniqueness(
                adapter_id,
                repo_id,
                repo_path,
                codebase_scope,
                requested_branch,
            )
            .await?;

        if !uniqueness.is_valid {
            errors.push(uniqueness.conflict_reason.unwrap_or_else(|| {
                format!(
                    "Active uniqueness constraint violated for adapter '{}' (repo_id: {:?}, repo_path: {:?}, codebase_scope: {:?})",
                    adapter_id, adapter.repo_id, adapter.repo_path, adapter.codebase_scope
                )
            }));
        }

        if errors.is_empty() {
            debug!(adapter_id = %adapter_id, "Adapter activation readiness validation passed");
        } else {
            warn!(adapter_id = %adapter_id, error_count = errors.len(), "Adapter activation readiness validation failed");
        }

        Ok(errors)
    }

    /// Check if a tenant has reached the maximum number of active adapters
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant to check
    /// * `max_active` - Maximum allowed active adapters for the tenant
    ///
    /// # Returns
    /// The current count of active adapters and whether the limit is exceeded.
    pub async fn check_tenant_active_adapter_quota(
        &self,
        tenant_id: &str,
        max_active: i64,
    ) -> Result<(i64, bool)> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM adapters WHERE tenant_id = ? AND lifecycle_state = 'active'",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active adapters: {}", e)))?;

        let exceeded = count >= max_active;
        if exceeded {
            warn!(tenant_id = %tenant_id, current_count = count, max_allowed = max_active, "Tenant active adapter quota exceeded");
        }
        Ok((count, exceeded))
    }

    /// Deactivate all adapters for a repository except the specified one
    ///
    /// # Arguments
    /// * `repo_id` - The repository ID
    /// * `except_adapter_id` - The adapter ID to keep
    /// * `target_state` - The state to transition conflicting adapters to
    ///
    /// # Returns
    /// The list of adapter IDs that were deactivated.
    pub async fn deactivate_conflicting_adapters(
        &self,
        repo_id: &str,
        except_adapter_id: &str,
        target_state: LifecycleState,
    ) -> Result<Vec<String>> {
        let active_adapters = self.list_active_adapters_for_repo(repo_id).await?;
        let mut deactivated = Vec::new();

        for (adapter_id, _branch) in active_adapters {
            if adapter_id == except_adapter_id {
                continue;
            }
            match self
                .update_adapter_lifecycle_state(&adapter_id, target_state)
                .await
            {
                Ok(()) => {
                    info!(adapter_id = %adapter_id, repo_id = %repo_id, new_state = %target_state.as_str(), "Deactivated conflicting adapter");
                    deactivated.push(adapter_id);
                }
                Err(e) => {
                    warn!(adapter_id = %adapter_id, error = %e, "Failed to deactivate conflicting adapter");
                }
            }
        }
        Ok(deactivated)
    }

    /// Validate active uniqueness for an adapter by name within a tenant
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant to check within
    /// * `adapter_id` - The adapter ID to validate for activation
    /// * `adapter_name` - The name of the adapter being activated
    ///
    /// # Returns
    /// A `SingleActiveValidationResult` indicating whether activation is allowed.
    pub async fn validate_active_adapter_name_uniqueness(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        adapter_name: &str,
    ) -> Result<SingleActiveValidationResult> {
        let rows = sqlx::query(
            "SELECT adapter_id FROM adapters WHERE tenant_id = ? AND name = ? AND lifecycle_state = 'active'",
        )
        .bind(tenant_id)
        .bind(adapter_name)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check active adapters by name: {}", e)))?;

        let mut conflicting_adapters = Vec::new();
        for row in rows {
            let other_id: String = row.get(0);
            if other_id != adapter_id {
                conflicting_adapters.push(other_id);
            }
        }

        if conflicting_adapters.is_empty() {
            Ok(SingleActiveValidationResult::valid())
        } else {
            let reason = format!(
                "Cannot activate adapter '{}' with name '{}': adapter(s) {} already active for tenant '{}'",
                adapter_id, adapter_name, conflicting_adapters.join(", "), tenant_id
            );
            warn!(adapter_id = %adapter_id, adapter_name = %adapter_name, tenant_id = %tenant_id, "Active adapter name uniqueness validation failed");
            Ok(SingleActiveValidationResult::conflict(
                conflicting_adapters,
                reason,
            ))
        }
    }

    /// Enforce active adapter name uniqueness, returning an error on violation
    pub async fn enforce_active_adapter_name_uniqueness(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        adapter_name: &str,
    ) -> Result<()> {
        let result = self
            .validate_active_adapter_name_uniqueness(tenant_id, adapter_id, adapter_name)
            .await?;
        if !result.is_valid {
            return Err(AosError::Validation(result.conflict_reason.unwrap_or_else(
                || {
                    format!(
                        "Active adapter name uniqueness violated for name '{}' in tenant '{}'",
                        adapter_name, tenant_id
                    )
                },
            )));
        }
        Ok(())
    }

    /// Count active adapters for a repository
    pub async fn count_active_adapters_for_repo(&self, repo_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM adapters WHERE repo_id = ? AND lifecycle_state = 'active'",
        )
        .bind(repo_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count active adapters: {}", e)))?;
        Ok(count)
    }

    /// List all repositories with multiple active adapters (potential violations)
    pub async fn find_repos_with_multiple_active_adapters(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT repo_id, COUNT(1) as cnt FROM adapters WHERE lifecycle_state = 'active' AND repo_id IS NOT NULL GROUP BY repo_id HAVING cnt > 1",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to find repos with multiple active: {}", e)))?;

        let results: Vec<(String, i64)> = rows
            .into_iter()
            .map(|row| {
                let repo_id: String = row.get(0);
                let count: i64 = row.get(1);
                (repo_id, count)
            })
            .collect();

        if !results.is_empty() {
            warn!(
                repo_count = results.len(),
                "Found repositories with multiple active adapters"
            );
        }
        Ok(results)
    }

    /// Get a summary of active adapter uniqueness status across the system
    pub async fn get_active_uniqueness_summary(&self) -> Result<(i64, i64, i64)> {
        let total_active: i64 =
            sqlx::query_scalar("SELECT COUNT(1) FROM adapters WHERE lifecycle_state = 'active'")
                .fetch_one(self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to count total active: {}", e)))?;

        let multi_active: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT repo_id) FROM (SELECT repo_id, COUNT(1) as cnt FROM adapters WHERE lifecycle_state = 'active' AND repo_id IS NOT NULL GROUP BY repo_id HAVING cnt > 1)",
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count multi-active repos: {}", e)))?;

        let single_active: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT repo_id) FROM (SELECT repo_id, COUNT(1) as cnt FROM adapters WHERE lifecycle_state = 'active' AND repo_id IS NOT NULL GROUP BY repo_id HAVING cnt = 1)",
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count single-active repos: {}", e)))?;

        info!(
            total_active = total_active,
            repos_with_multiple = multi_active,
            repos_compliant = single_active,
            "Active uniqueness summary computed"
        );
        Ok((total_active, multi_active, single_active))
    }

    // ========================================================================
    // Lifecycle Enforcement
    // ========================================================================

    /// Enforce lifecycle state transition with full validation
    ///
    /// This is the primary entry point for enforcing lifecycle rules. It combines:
    /// 1. Core state machine validation (allowed transitions)
    /// 2. Tier-specific rules (e.g., ephemeral cannot be deprecated)
    /// 3. Custom lifecycle rules from the database
    /// 4. Prerequisite checks (artifact presence, training snapshots, etc.)
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter undergoing the transition
    /// * `from_state` - The current lifecycle state
    /// * `to_state` - The target lifecycle state
    /// * `tier` - The adapter tier (ephemeral, warm, persistent)
    /// * `options` - Additional enforcement options
    ///
    /// # Returns
    /// A `LifecycleEnforcementResult` with validation status and any warnings
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # use adapteros_db::validation::LifecycleEnforcementOptions;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let result = db.enforce_lifecycle_transition(
    ///     "adapter-123",
    ///     "ready",
    ///     "active",
    ///     "persistent",
    ///     LifecycleEnforcementOptions::default(),
    /// ).await?;
    ///
    /// if !result.allowed {
    ///     println!("Transition denied: {:?}", result.denial_reason);
    /// }
    /// for warning in &result.warnings {
    ///     println!("Warning: {}", warning);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enforce_lifecycle_transition(
        &self,
        adapter_id: &str,
        from_state: &str,
        to_state: &str,
        tier: &str,
        options: LifecycleEnforcementOptions,
    ) -> Result<LifecycleEnforcementResult> {
        // First, validate using the lifecycle_rules module
        let transition_result = self
            .validate_lifecycle_transition(adapter_id, from_state, to_state, tier)
            .await?;

        if !transition_result.allowed {
            return Ok(LifecycleEnforcementResult {
                allowed: false,
                denial_reason: transition_result.denial_reason,
                warnings: transition_result.warnings,
                evaluated_rules: transition_result.evaluated_rules,
                prerequisite_checks: Vec::new(),
            });
        }

        if options.fail_on_warnings && !transition_result.warnings.is_empty() {
            return Ok(LifecycleEnforcementResult {
                allowed: false,
                denial_reason: Some(format!(
                    "Lifecycle transition warnings: {}",
                    transition_result.warnings.join("; ")
                )),
                warnings: transition_result.warnings,
                evaluated_rules: transition_result.evaluated_rules,
                prerequisite_checks: Vec::new(),
            });
        }

        let from_state_enum = LifecycleState::from_str(from_state).map_err(|e| {
            AosError::Validation(format!("Invalid from_state '{}': {}", from_state, e))
        })?;
        let to_state_enum = LifecycleState::from_str(to_state)
            .map_err(|e| AosError::Validation(format!("Invalid to_state '{}': {}", to_state, e)))?;

        // Perform prerequisite checks if not skipped
        let mut prerequisite_checks = Vec::new();
        let mut has_artifact = true;
        let mut has_training_evidence = true;

        if !options.skip_prerequisite_checks {
            // Check artifact requirements for ready/active/deprecated/retired
            if matches!(
                to_state_enum,
                LifecycleState::Ready
                    | LifecycleState::Active
                    | LifecycleState::Deprecated
                    | LifecycleState::Retired
            ) {
                let artifact_check = self.check_artifact_prerequisites(adapter_id).await?;
                has_artifact = artifact_check.passed;
                prerequisite_checks.push(artifact_check.clone());

                if !artifact_check.passed {
                    return Ok(LifecycleEnforcementResult {
                        allowed: false,
                        denial_reason: Some(format!(
                            "Prerequisite check '{}' failed: {}",
                            artifact_check.check_name,
                            artifact_check.details.as_deref().unwrap_or("unknown")
                        )),
                        warnings: transition_result.warnings,
                        evaluated_rules: transition_result.evaluated_rules,
                        prerequisite_checks,
                    });
                }
            }

            // Check training snapshot for active state
            if matches!(to_state_enum, LifecycleState::Active) {
                let snapshot_check = self
                    .check_training_snapshot_prerequisite(adapter_id)
                    .await?;
                has_training_evidence = snapshot_check.passed;
                prerequisite_checks.push(snapshot_check.clone());

                if !snapshot_check.passed {
                    return Ok(LifecycleEnforcementResult {
                        allowed: false,
                        denial_reason: Some(format!(
                            "Prerequisite check '{}' failed: {}",
                            snapshot_check.check_name,
                            snapshot_check.details.as_deref().unwrap_or("unknown")
                        )),
                        warnings: transition_result.warnings,
                        evaluated_rules: transition_result.evaluated_rules,
                        prerequisite_checks,
                    });
                }

                // Check single-active constraint
                if !options.skip_uniqueness_check {
                    let uniqueness_check = self
                        .check_active_uniqueness_prerequisite(adapter_id)
                        .await?;
                    prerequisite_checks.push(uniqueness_check.clone());

                    if !uniqueness_check.passed {
                        return Ok(LifecycleEnforcementResult {
                            allowed: false,
                            denial_reason: Some(format!(
                                "Prerequisite check '{}' failed: {}",
                                uniqueness_check.check_name,
                                uniqueness_check.details.as_deref().unwrap_or("unknown")
                            )),
                            warnings: transition_result.warnings,
                            evaluated_rules: transition_result.evaluated_rules,
                            prerequisite_checks,
                        });
                    }
                }
            }
        }

        let active_references = if matches!(
            to_state_enum,
            LifecycleState::Deprecated | LifecycleState::Retired
        ) {
            self.check_active_stack_references(adapter_id).await?.len() as u64
        } else {
            0
        };

        let preflight_status = if options.skip_prerequisite_checks || has_artifact {
            PreflightStatus::Passed
        } else {
            PreflightStatus::Pending
        };

        let ctx = ValidationContext::new()
            .with_tier(tier.to_string())
            .with_preflight_status(preflight_status)
            .with_bypass_preflight(options.skip_prerequisite_checks)
            .with_artifact(has_artifact)
            .with_training_evidence(has_training_evidence)
            .with_active_references(active_references);

        if let Err(violations) =
            validate_transition_with_context(from_state_enum, to_state_enum, &ctx)
        {
            let reason = violations
                .first()
                .map(|violation| violation.message.clone())
                .unwrap_or_else(|| "Lifecycle transition denied by core rules".to_string());
            return Ok(LifecycleEnforcementResult {
                allowed: false,
                denial_reason: Some(reason),
                warnings: transition_result.warnings,
                evaluated_rules: transition_result.evaluated_rules,
                prerequisite_checks,
            });
        }

        info!(
            adapter_id = %adapter_id,
            from_state = %from_state,
            to_state = %to_state,
            tier = %tier,
            rules_evaluated = transition_result.evaluated_rules.len(),
            prerequisite_checks = prerequisite_checks.len(),
            "Lifecycle transition enforcement passed"
        );

        Ok(LifecycleEnforcementResult {
            allowed: true,
            denial_reason: None,
            warnings: transition_result.warnings,
            evaluated_rules: transition_result.evaluated_rules,
            prerequisite_checks,
        })
    }

    /// Check if an adapter has the required artifact prerequisites
    ///
    /// Verifies that aos_file_path, aos_file_hash, and content_hash_b3 are set.
    async fn check_artifact_prerequisites(
        &self,
        adapter_id: &str,
    ) -> Result<PrerequisiteCheckResult> {
        let row = sqlx::query(
            "SELECT aos_file_path, aos_file_hash, content_hash_b3 FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check artifact prerequisites: {}", e)))?;

        match row {
            None => Ok(PrerequisiteCheckResult {
                check_name: "artifact_presence".to_string(),
                passed: false,
                details: Some("Adapter not found".to_string()),
            }),
            Some(row) => {
                let aos_file_path: Option<String> = row.get(0);
                let aos_file_hash: Option<String> = row.get(1);
                let content_hash_b3: Option<String> = row.get(2);

                let missing = vec![
                    aos_file_path
                        .as_deref()
                        .map(str::is_empty)
                        .unwrap_or(true)
                        .then_some("aos_file_path"),
                    aos_file_hash
                        .as_deref()
                        .map(str::is_empty)
                        .unwrap_or(true)
                        .then_some("aos_file_hash"),
                    content_hash_b3
                        .as_deref()
                        .map(str::is_empty)
                        .unwrap_or(true)
                        .then_some("content_hash_b3"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

                if missing.is_empty() {
                    Ok(PrerequisiteCheckResult {
                        check_name: "artifact_presence".to_string(),
                        passed: true,
                        details: Some("All artifact fields present".to_string()),
                    })
                } else {
                    Ok(PrerequisiteCheckResult {
                        check_name: "artifact_presence".to_string(),
                        passed: false,
                        details: Some(format!("Missing artifact fields: {}", missing.join(", "))),
                    })
                }
            }
        }
    }

    /// Check if an adapter has a training snapshot (required for active state)
    async fn check_training_snapshot_prerequisite(
        &self,
        adapter_id: &str,
    ) -> Result<PrerequisiteCheckResult> {
        let snapshot_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM adapter_training_snapshots WHERE adapter_id = ? LIMIT 1",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check training snapshot: {}", e)))?;

        Ok(PrerequisiteCheckResult {
            check_name: "training_snapshot".to_string(),
            passed: snapshot_exists.is_some(),
            details: Some(if snapshot_exists.is_some() {
                "Training snapshot exists".to_string()
            } else {
                "No training snapshot found - required for active state".to_string()
            }),
        })
    }

    /// Check single-active uniqueness constraint for activation
    ///
    /// This prerequisite check validates all active uniqueness constraints:
    /// - repo_id + branch uniqueness
    /// - repo_path + branch uniqueness
    /// - codebase_scope + branch uniqueness (for codebase adapters)
    async fn check_active_uniqueness_prerequisite(
        &self,
        adapter_id: &str,
    ) -> Result<PrerequisiteCheckResult> {
        // Fetch adapter's repo_id, repo_path, codebase_scope, and metadata
        let row = sqlx::query(
            "SELECT repo_id, repo_path, codebase_scope, metadata_json FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch adapter for uniqueness check: {}", e)))?;

        match row {
            None => Ok(PrerequisiteCheckResult {
                check_name: "active_uniqueness".to_string(),
                passed: false,
                details: Some("Adapter not found".to_string()),
            }),
            Some(row) => {
                let mut repo_id: Option<String> = row.get(0);
                let mut repo_path: Option<String> = row.get(1);
                let mut codebase_scope: Option<String> = row.get(2);
                let metadata_json: Option<String> = row.get(3);
                let requested_branch = branch_from_metadata(&metadata_json);
                if repo_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    repo_id = repo_id_from_metadata(&metadata_json);
                }
                if repo_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    repo_path = repo_path_from_metadata(&metadata_json);
                }
                if codebase_scope
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    codebase_scope = codebase_scope_from_metadata(&metadata_json);
                }

                let result = self
                    .validate_active_uniqueness(
                        adapter_id,
                        repo_id,
                        repo_path,
                        codebase_scope,
                        requested_branch,
                    )
                    .await?;

                Ok(PrerequisiteCheckResult {
                    check_name: "active_uniqueness".to_string(),
                    passed: result.is_valid,
                    details: if result.is_valid {
                        Some("No conflicting active adapters".to_string())
                    } else {
                        result.conflict_reason
                    },
                })
            }
        }
    }

    /// Evaluate lifecycle rules for an adapter and return matching rule actions
    ///
    /// This method finds all applicable lifecycle rules for an adapter and
    /// evaluates them against the provided field values.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter to evaluate rules for
    /// * `tenant_id` - The tenant ID
    /// * `category` - The adapter category
    /// * `field_values` - JSON object with field values to match against conditions
    ///
    /// # Returns
    /// A vector of rule evaluations for rules whose conditions matched
    pub async fn evaluate_adapter_lifecycle_rules(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        category: &str,
        field_values: &serde_json::Value,
    ) -> Result<Vec<LifecycleRuleEvaluation>> {
        let rules = self
            .get_applicable_lifecycle_rules(adapter_id, tenant_id, category)
            .await?;

        let evaluations: Vec<LifecycleRuleEvaluation> = rules
            .iter()
            .map(|rule| Self::evaluate_rule(rule, field_values))
            .filter(|eval| eval.conditions_met)
            .collect();

        debug!(
            adapter_id = %adapter_id,
            tenant_id = %tenant_id,
            rules_evaluated = rules.len(),
            rules_matched = evaluations.len(),
            "Evaluated lifecycle rules for adapter"
        );

        Ok(evaluations)
    }

    /// Validate lifecycle state compatibility with tier
    ///
    /// A quick check to validate that a lifecycle state is valid for a given tier
    /// without performing a full transition validation.
    ///
    /// # Arguments
    /// * `lifecycle_state` - The lifecycle state to check
    /// * `tier` - The adapter tier
    ///
    /// # Returns
    /// `true` if the state is valid for the tier
    pub fn is_lifecycle_state_valid_for_tier(lifecycle_state: &str, tier: &str) -> bool {
        match LifecycleState::from_str(lifecycle_state) {
            Ok(state) => state.is_valid_for_tier(tier),
            Err(_) => false,
        }
    }

    /// Get the next valid lifecycle state for an adapter
    ///
    /// Returns the next state in the lifecycle progression, considering tier-specific rules.
    ///
    /// # Arguments
    /// * `current_state` - The current lifecycle state
    /// * `tier` - The adapter tier
    ///
    /// # Returns
    /// The next valid state, or None if in a terminal state
    pub fn get_next_lifecycle_state(current_state: &str, tier: &str) -> Option<String> {
        let state = LifecycleState::from_str(current_state).ok()?;

        // For ephemeral adapters, skip deprecated
        if tier == "ephemeral" && state == LifecycleState::Active {
            return Some("retired".to_string());
        }

        state.next().map(|s| s.as_str().to_string())
    }

    /// Get all valid transitions from a given state
    ///
    /// Returns a list of states that can be transitioned to from the current state,
    /// considering tier-specific rules.
    ///
    /// # Arguments
    /// * `current_state` - The current lifecycle state
    /// * `tier` - The adapter tier
    ///
    /// # Returns
    /// A vector of valid target states
    pub fn get_valid_transitions(current_state: &str, tier: &str) -> Vec<String> {
        let Ok(state) = LifecycleState::from_str(current_state) else {
            return Vec::new();
        };

        let all_states = [
            LifecycleState::Draft,
            LifecycleState::Training,
            LifecycleState::Ready,
            LifecycleState::Active,
            LifecycleState::Deprecated,
            LifecycleState::Retired,
            LifecycleState::Failed,
        ];

        all_states
            .iter()
            .filter(|target| state.can_transition_to_for_tier(**target, tier))
            .map(|s| s.as_str().to_string())
            .collect()
    }
}

/// Options for lifecycle enforcement
#[derive(Debug, Clone, Default)]
pub struct LifecycleEnforcementOptions {
    /// Skip prerequisite checks (artifact presence, training snapshot)
    pub skip_prerequisite_checks: bool,
    /// Skip uniqueness check for active state
    pub skip_uniqueness_check: bool,
    /// Treat lifecycle rule warnings as hard failures
    pub fail_on_warnings: bool,
}

/// Result of a prerequisite check
#[derive(Debug, Clone)]
pub struct PrerequisiteCheckResult {
    /// Name of the check
    pub check_name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Additional details about the check result
    pub details: Option<String>,
}

/// Result of lifecycle enforcement
#[derive(Debug, Clone)]
pub struct LifecycleEnforcementResult {
    /// Whether the transition is allowed
    pub allowed: bool,
    /// Reason for denial (if not allowed)
    pub denial_reason: Option<String>,
    /// Warnings that don't block the transition
    pub warnings: Vec<String>,
    /// Rule IDs that were evaluated
    pub evaluated_rules: Vec<String>,
    /// Results of prerequisite checks
    pub prerequisite_checks: Vec<PrerequisiteCheckResult>,
}

fn parse_metadata_json(metadata_json: &Option<String>) -> Option<Value> {
    metadata_json
        .as_ref()
        .and_then(|raw| serde_json::from_str(raw).ok())
}

fn extract_metadata_string(parsed: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = parsed.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn branch_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    let parsed = parse_metadata_json(metadata_json)?;
    extract_metadata_string(
        &parsed,
        &["scope_branch", "repo_branch", "branch", "git_branch"],
    )
}

fn repo_id_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    let parsed = parse_metadata_json(metadata_json)?;
    extract_metadata_string(&parsed, &["repo_identifier", "scope_repo_id", "repo_id"])
}

fn repo_path_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    let parsed = parse_metadata_json(metadata_json)?;
    extract_metadata_string(
        &parsed,
        &[
            "repo_path",
            "scan_root_path",
            "scope_scan_root",
            "repo_root_path",
        ],
    )
}

fn codebase_scope_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    let parsed = parse_metadata_json(metadata_json)?;
    extract_metadata_string(
        &parsed,
        &[
            "codebase_scope",
            "repo_identifier",
            "scope_repo_id",
            "repo_id",
        ],
    )
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

    #[test]
    fn test_lifecycle_state_valid_for_tier() {
        // Ephemeral cannot be deprecated
        assert!(!Db::is_lifecycle_state_valid_for_tier(
            "deprecated",
            "ephemeral"
        ));

        // Other states are valid for ephemeral
        assert!(Db::is_lifecycle_state_valid_for_tier("active", "ephemeral"));
        assert!(Db::is_lifecycle_state_valid_for_tier("ready", "ephemeral"));
        assert!(Db::is_lifecycle_state_valid_for_tier(
            "retired",
            "ephemeral"
        ));

        // All states valid for persistent
        assert!(Db::is_lifecycle_state_valid_for_tier(
            "deprecated",
            "persistent"
        ));
        assert!(Db::is_lifecycle_state_valid_for_tier(
            "active",
            "persistent"
        ));

        // Invalid state returns false
        assert!(!Db::is_lifecycle_state_valid_for_tier(
            "invalid_state",
            "persistent"
        ));
    }

    #[test]
    fn test_get_next_lifecycle_state() {
        // Normal progression
        assert_eq!(
            Db::get_next_lifecycle_state("draft", "persistent"),
            Some("training".to_string())
        );
        assert_eq!(
            Db::get_next_lifecycle_state("training", "persistent"),
            Some("ready".to_string())
        );
        assert_eq!(
            Db::get_next_lifecycle_state("ready", "persistent"),
            Some("active".to_string())
        );
        assert_eq!(
            Db::get_next_lifecycle_state("active", "persistent"),
            Some("deprecated".to_string())
        );
        assert_eq!(
            Db::get_next_lifecycle_state("deprecated", "persistent"),
            Some("retired".to_string())
        );

        // Terminal states
        assert_eq!(Db::get_next_lifecycle_state("retired", "persistent"), None);
        assert_eq!(Db::get_next_lifecycle_state("failed", "persistent"), None);

        // Ephemeral skips deprecated
        assert_eq!(
            Db::get_next_lifecycle_state("active", "ephemeral"),
            Some("retired".to_string())
        );

        // Invalid state
        assert_eq!(Db::get_next_lifecycle_state("invalid", "persistent"), None);
    }

    #[test]
    fn test_get_valid_transitions() {
        // From active, persistent adapter
        let transitions = Db::get_valid_transitions("active", "persistent");
        assert!(transitions.contains(&"deprecated".to_string()));
        assert!(transitions.contains(&"ready".to_string())); // rollback
        assert!(transitions.contains(&"failed".to_string()));
        assert!(transitions.contains(&"active".to_string())); // no-op
        assert!(!transitions.contains(&"retired".to_string())); // must go through deprecated

        // From active, ephemeral adapter
        let transitions = Db::get_valid_transitions("active", "ephemeral");
        assert!(transitions.contains(&"retired".to_string())); // can skip deprecated
        assert!(!transitions.contains(&"deprecated".to_string())); // cannot use deprecated

        // From terminal state
        let transitions = Db::get_valid_transitions("retired", "persistent");
        // Only self-transition (no-op) should be valid
        assert!(transitions.contains(&"retired".to_string()));
        assert!(!transitions.contains(&"active".to_string()));

        // Invalid state
        let transitions = Db::get_valid_transitions("invalid", "persistent");
        assert!(transitions.is_empty());
    }

    #[test]
    fn test_lifecycle_enforcement_options_default() {
        let options = LifecycleEnforcementOptions::default();
        assert!(!options.skip_prerequisite_checks);
        assert!(!options.skip_uniqueness_check);
    }

    #[test]
    fn test_prerequisite_check_result() {
        let passed = PrerequisiteCheckResult {
            check_name: "test_check".to_string(),
            passed: true,
            details: Some("Check passed".to_string()),
        };
        assert!(passed.passed);
        assert_eq!(passed.check_name, "test_check");

        let failed = PrerequisiteCheckResult {
            check_name: "test_check".to_string(),
            passed: false,
            details: Some("Missing required field".to_string()),
        };
        assert!(!failed.passed);
    }

    #[test]
    fn test_lifecycle_enforcement_result() {
        let allowed = LifecycleEnforcementResult {
            allowed: true,
            denial_reason: None,
            warnings: vec!["Consider reviewing".to_string()],
            evaluated_rules: vec!["rule-1".to_string()],
            prerequisite_checks: vec![PrerequisiteCheckResult {
                check_name: "artifact".to_string(),
                passed: true,
                details: None,
            }],
        };
        assert!(allowed.allowed);
        assert!(allowed.denial_reason.is_none());
        assert_eq!(allowed.warnings.len(), 1);
        assert_eq!(allowed.evaluated_rules.len(), 1);
        assert_eq!(allowed.prerequisite_checks.len(), 1);

        let denied = LifecycleEnforcementResult {
            allowed: false,
            denial_reason: Some("Transition not allowed".to_string()),
            warnings: Vec::new(),
            evaluated_rules: Vec::new(),
            prerequisite_checks: Vec::new(),
        };
        assert!(!denied.allowed);
        assert!(denied.denial_reason.is_some());
    }

    #[test]
    fn test_single_active_validation_result_codebase_scope_conflict_format() {
        // Test that codebase scope conflicts have proper error messages
        let result = SingleActiveValidationResult::conflict(
            vec!["codebase-adapter-1".to_string()],
            "Cannot activate adapter 'codebase-adapter-2' for codebase scope 'github.com/myorg/myrepo' on branch 'main': adapter(s) codebase-adapter-1 already active. Only one codebase adapter can be active per scope/branch.".to_string(),
        );

        assert!(!result.is_valid);
        assert_eq!(result.conflicting_adapters.len(), 1);
        assert!(result
            .conflict_reason
            .as_ref()
            .unwrap()
            .contains("codebase scope"));
        assert!(result
            .conflict_reason
            .as_ref()
            .unwrap()
            .contains("github.com/myorg/myrepo"));
    }

    #[test]
    fn test_single_active_validation_result_multiple_dimension_conflict() {
        // Test that multiple dimension conflicts (repo_id, repo_path, codebase_scope) can be combined
        let result = SingleActiveValidationResult::conflict(
            vec![
                "adapter-1".to_string(),
                "adapter-2".to_string(),
                "adapter-3".to_string(),
            ],
            "Multiple active uniqueness violations:\nrepo_id conflict\nrepo_path conflict\ncodebase_scope conflict".to_string(),
        );

        assert!(!result.is_valid);
        assert_eq!(result.conflicting_adapters.len(), 3);
        assert!(result
            .conflict_reason
            .as_ref()
            .unwrap()
            .contains("Multiple active uniqueness violations"));
    }

    #[test]
    fn test_single_active_validation_valid_different_branches() {
        // Simulates the scenario where two codebase adapters target the same scope
        // but different branches - this should be valid
        let result = SingleActiveValidationResult::valid();
        assert!(result.is_valid);
        assert!(result.conflicting_adapters.is_empty());
        assert!(result.conflict_reason.is_none());
    }

    #[test]
    fn test_codebase_scope_format_variations() {
        // Test that codebase_scope can be various formats
        let github_scope = "github.com/myorg/myrepo";
        let gitlab_scope = "gitlab.com/group/project";
        let local_scope = "/path/to/local/repo";

        // These are just strings, so any format should work
        let conflict_github = SingleActiveValidationResult::conflict(
            vec!["adapter-1".to_string()],
            format!("Conflict for codebase scope '{}'", github_scope),
        );
        assert!(conflict_github
            .conflict_reason
            .unwrap()
            .contains(github_scope));

        let conflict_gitlab = SingleActiveValidationResult::conflict(
            vec!["adapter-1".to_string()],
            format!("Conflict for codebase scope '{}'", gitlab_scope),
        );
        assert!(conflict_gitlab
            .conflict_reason
            .unwrap()
            .contains(gitlab_scope));

        let conflict_local = SingleActiveValidationResult::conflict(
            vec!["adapter-1".to_string()],
            format!("Conflict for codebase scope '{}'", local_scope),
        );
        assert!(conflict_local
            .conflict_reason
            .unwrap()
            .contains(local_scope));
    }
}
