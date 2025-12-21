//! Lineage validation and revision monotonicity
//!
//! This module implements adapter lineage validation including:
//! - Revision monotonicity (new revisions must be greater than existing)
//! - Gap constraints (cannot skip more than 5 revisions)
//! - Circular dependency detection
//!
//! Uses SQLite recursive CTEs for efficient ancestry traversal.

use crate::Db;
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use tracing::{error, warn};

/// Trait for lineage validation operations
#[async_trait]
pub trait LineageValidator {
    /// Check if child is a descendant of potential ancestor (recursive CTE)
    async fn is_descendant_of(&self, child_id: &str, ancestor_id: &str) -> Result<bool>;

    /// Validate revision monotonicity for a lineage
    async fn validate_revision_monotonicity(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
        new_revision: u32,
    ) -> Result<()>;

    /// Check for circular dependencies before registration
    async fn check_circular_dependency(&self, adapter_id: &str, parent_id: &str) -> Result<()>;
}

#[async_trait]
impl LineageValidator for Db {
    async fn is_descendant_of(&self, child_id: &str, ancestor_id: &str) -> Result<bool> {
        is_descendant_of(self, child_id, ancestor_id).await
    }

    async fn validate_revision_monotonicity(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
        new_revision: u32,
    ) -> Result<()> {
        validate_revision_for_registration(self, tenant, domain, purpose, new_revision).await
    }

    async fn check_circular_dependency(&self, adapter_id: &str, parent_id: &str) -> Result<()> {
        if is_descendant_of(self, parent_id, adapter_id).await? {
            error!(
                event_type = "circular_dependency.detected",
                adapter_id = %adapter_id,
                parent_id = %parent_id,
                "Circular dependency detected - registration blocked"
            );
            return Err(AosError::Registry(format!(
                "Circular dependency detected: '{}' cannot be parent of '{}' (creates cycle)",
                parent_id, adapter_id
            )));
        }
        Ok(())
    }
}

/// Check if child_id is a descendant of potential_ancestor_id (recursively)
///
/// Uses SQLite recursive CTE to traverse the parent chain and detect:
/// - Direct parent-child relationships
/// - Multi-level ancestry (grandparent, great-grandparent, etc.)
/// - Circular dependencies (A→B→C→A)
pub async fn is_descendant_of(db: &Db, child_id: &str, potential_ancestor_id: &str) -> Result<bool> {
    let result: Option<i32> = sqlx::query_scalar(
        r#"
        WITH RECURSIVE ancestry AS (
            SELECT adapter_id, parent_id FROM adapters WHERE adapter_id = ?1
            UNION ALL
            SELECT a.adapter_id, a.parent_id
            FROM adapters a
            JOIN ancestry ON a.adapter_id = ancestry.parent_id
            WHERE ancestry.parent_id IS NOT NULL
        )
        SELECT 1 FROM ancestry WHERE adapter_id = ?2
        "#,
    )
    .bind(child_id)
    .bind(potential_ancestor_id)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Registry(format!("Ancestry check failed: {}", e)))?;

    Ok(result.is_some())
}

/// Validate revision monotonicity for a lineage before registration
///
/// Enforces:
/// 1. New revision must be greater than the latest revision
/// 2. Cannot skip more than 5 revisions (prevents accidental gaps)
pub async fn validate_revision_for_registration(
    db: &Db,
    tenant: &str,
    domain: &str,
    purpose: &str,
    new_revision: u32,
) -> Result<()> {
    // Get the latest revision in the lineage
    let latest: Option<LatestRevisionRow> = sqlx::query_as(
        r#"
        SELECT revision FROM adapters
        WHERE tenant_namespace = ?1 AND domain = ?2 AND purpose = ?3
        ORDER BY
            CAST(REPLACE(REPLACE(revision, 'r', ''), 'R', '') AS INTEGER) DESC
        LIMIT 1
        "#,
    )
    .bind(tenant)
    .bind(domain)
    .bind(purpose)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Registry(format!("Failed to get latest revision: {}", e)))?;

    if let Some(row) = latest {
        if let Some(rev_str) = row.revision {
            // Parse revision number (format: r001, r042, etc.)
            let latest_rev = parse_revision_number(&rev_str)?;

            // Revision must be greater than latest
            if new_revision <= latest_rev {
                warn!(
                    event_type = "revision.monotonicity_violation",
                    tenant = %tenant,
                    domain = %domain,
                    purpose = %purpose,
                    latest_rev = latest_rev,
                    attempted_rev = new_revision,
                    "Revision monotonicity check failed"
                );
                return Err(AosError::Registry(format!(
                    "Revision r{:03} must be greater than latest r{:03} in lineage {}/{}/{}",
                    new_revision, latest_rev, tenant, domain, purpose
                )));
            }

            // Cannot skip more than 5 revisions
            if new_revision > latest_rev + 5 {
                warn!(
                    event_type = "revision.gap_too_large",
                    tenant = %tenant,
                    domain = %domain,
                    purpose = %purpose,
                    latest_rev = latest_rev,
                    attempted_rev = new_revision,
                    gap = new_revision - latest_rev,
                    "Revision gap exceeds limit"
                );
                return Err(AosError::Registry(format!(
                    "Cannot skip more than 5 revisions: r{:03} → r{:03} (gap of {})",
                    latest_rev,
                    new_revision,
                    new_revision - latest_rev
                )));
            }
        }
    }

    Ok(())
}

/// Get next revision number for a lineage
pub async fn next_revision_number(db: &Db, tenant: &str, domain: &str, purpose: &str) -> Result<u32> {
    let latest: Option<LatestRevisionRow> = sqlx::query_as(
        r#"
        SELECT revision FROM adapters
        WHERE tenant_namespace = ?1 AND domain = ?2 AND purpose = ?3
        ORDER BY
            CAST(REPLACE(REPLACE(revision, 'r', ''), 'R', '') AS INTEGER) DESC
        LIMIT 1
        "#,
    )
    .bind(tenant)
    .bind(domain)
    .bind(purpose)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Registry(format!("Failed to get latest revision: {}", e)))?;

    if let Some(row) = latest {
        if let Some(rev_str) = row.revision {
            let current = parse_revision_number(&rev_str)?;
            return Ok(current + 1);
        }
    }

    Ok(1)
}

/// Parse revision number from string (e.g., "r042" -> 42)
fn parse_revision_number(rev_str: &str) -> Result<u32> {
    let cleaned = rev_str
        .trim()
        .trim_start_matches('r')
        .trim_start_matches('R');

    cleaned.parse::<u32>().map_err(|e| {
        AosError::Registry(format!(
            "Failed to parse revision number '{}': {}",
            rev_str, e
        ))
    })
}

#[derive(sqlx::FromRow)]
struct LatestRevisionRow {
    revision: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_descendant_of_empty() {
        let db = Db::new_in_memory().await.unwrap();

        // Insert test adapters
        sqlx::query(
            r#"
            INSERT INTO adapters (id, adapter_id, tenant_id, name, hash_b3, tier, rank, alpha, targets_json)
            VALUES ('a1', 'adapter-a', 'default', 'a', 'abc', 'warm', 8, 32, '[]')
            "#,
        )
        .execute(db.pool())
        .await
        .unwrap();

        // No parent relationship
        let result = is_descendant_of(&db, "adapter-a", "adapter-b").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_parse_revision_number() {
        assert_eq!(parse_revision_number("r001").unwrap(), 1);
        assert_eq!(parse_revision_number("r042").unwrap(), 42);
        assert_eq!(parse_revision_number("R100").unwrap(), 100);
        assert_eq!(parse_revision_number("5").unwrap(), 5);
    }
}
