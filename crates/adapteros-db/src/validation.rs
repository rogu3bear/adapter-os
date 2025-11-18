/// Metadata validation functions
///
/// This module provides validation for adapter and stack metadata changes,
/// enforcing the rules defined in PRD-02 and documented in VERSION_GUARANTEES.md
///
/// Citation: PRD-02 (2025-11-17)

use crate::metadata::{LifecycleState, validate_state_transition, validate_version};
use crate::Db;
use anyhow::Result;
use std::str::FromStr;

impl Db {
    /// Validate and update adapter lifecycle state
    ///
    /// This method enforces lifecycle state transition rules and tier-specific constraints.
    ///
    /// **Rules**:
    /// - States must transition forward: draft → active → deprecated → retired
    /// - Ephemeral adapters cannot be deprecated (must go directly to retired)
    /// - Retired is a terminal state (no transitions out)
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

        let current_state = LifecycleState::from_str(&adapter.lifecycle_state)
            .map_err(|_| {
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

        sqlx::query(
            "UPDATE adapters SET lifecycle_state = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(new_state.as_str())
        .bind(adapter_id)
        .execute(self.pool())
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
    pub async fn update_adapter_version(
        &self,
        adapter_id: &str,
        new_version: &str,
    ) -> Result<()> {
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

        let current_state = LifecycleState::from_str(&stack.lifecycle_state)
            .map_err(|_| {
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

        sqlx::query(
            "UPDATE adapter_stacks SET lifecycle_state = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?",
        )
        .bind(new_state.as_str())
        .bind(stack_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update lifecycle state: {}", e)))?;

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

        sqlx::query(
            "UPDATE adapter_stacks SET version = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?",
        )
        .bind(new_version)
        .bind(stack_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update version: {}", e)))?;

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
        assert!(result.unwrap_err().to_string().contains("not valid for tier"));
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
        assert!(result.unwrap_err().to_string().contains("Invalid lifecycle state"));
    }
}
