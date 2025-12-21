//! Tenant Policy Customizations - Database operations
//!
//! Manages tenant-specific policy parameter overrides with approval workflow.
//! Citation: AGENTS.md - Policy Studio feature for tenant-safe policy authoring

use adapteros_core::{AosError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Status of a tenant policy customization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomizationStatus {
    Draft,
    PendingReview,
    Approved,
    Rejected,
    Active,
}

impl CustomizationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Active => "active",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "draft" => Ok(Self::Draft),
            "pending_review" => Ok(Self::PendingReview),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "active" => Ok(Self::Active),
            _ => Err(AosError::Validation(format!(
                "Invalid customization status: {}",
                s
            ))),
        }
    }
}

/// Tenant policy customization record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantPolicyCustomization {
    pub id: String,
    pub tenant_id: String,
    pub base_policy_type: String,
    pub customizations_json: String,
    pub status: CustomizationStatus,
    pub submitted_at: Option<String>,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub review_notes: Option<String>,
    pub activated_at: Option<String>,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub metadata_json: Option<String>,
}

/// History entry for policy customization changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomizationHistoryEntry {
    pub id: String,
    pub customization_id: String,
    pub action: String,
    pub performed_by: String,
    pub performed_at: String,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub notes: Option<String>,
    pub changes_json: Option<String>,
}

/// Create customization request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCustomizationRequest {
    pub tenant_id: String,
    pub base_policy_type: String,
    pub customizations_json: String,
    pub created_by: String,
    pub metadata_json: Option<String>,
}

/// Database operations for tenant policy customizations
pub struct TenantPolicyCustomizationOps {
    pool: SqlitePool,
}

impl TenantPolicyCustomizationOps {
    #[allow(dead_code)]
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new policy customization (draft status)
    pub async fn create_customization(
        &self,
        req: CreateCustomizationRequest,
    ) -> Result<TenantPolicyCustomization> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let status = CustomizationStatus::Draft;

        sqlx::query(
            r#"
            INSERT INTO tenant_policy_customizations 
            (id, tenant_id, base_policy_type, customizations_json, status, created_at, created_by, updated_at, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(&req.base_policy_type)
        .bind(&req.customizations_json)
        .bind(status.as_str())
        .bind(&now)
        .bind(&req.created_by)
        .bind(&now)
        .bind(&req.metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create customization: {}", e)))?;

        // Record history
        self.add_history_entry(
            &id,
            "created",
            &req.created_by,
            None,
            Some(status.as_str()),
            Some("Created policy customization draft"),
            None,
        )
        .await?;

        info!(
            customization_id = %id,
            tenant_id = %req.tenant_id,
            policy_type = %req.base_policy_type,
            "Created policy customization"
        );

        self.get_customization(&id).await?.ok_or_else(|| {
            AosError::NotFound(format!("Customization {} not found after creation", id))
        })
    }

    /// Get customization by ID
    pub async fn get_customization(&self, id: &str) -> Result<Option<TenantPolicyCustomization>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, base_policy_type, customizations_json, status, 
                   submitted_at, reviewed_at, reviewed_by, review_notes, activated_at,
                   created_at, created_by, updated_at, metadata_json
            FROM tenant_policy_customizations
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get customization: {}", e)))?;

        Ok(row.map(|r| TenantPolicyCustomization {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            base_policy_type: r.get("base_policy_type"),
            customizations_json: r.get("customizations_json"),
            status: CustomizationStatus::from_str(r.get("status"))
                .unwrap_or(CustomizationStatus::Draft),
            submitted_at: r.get("submitted_at"),
            reviewed_at: r.get("reviewed_at"),
            reviewed_by: r.get("reviewed_by"),
            review_notes: r.get("review_notes"),
            activated_at: r.get("activated_at"),
            created_at: r.get("created_at"),
            created_by: r.get("created_by"),
            updated_at: r.get("updated_at"),
            metadata_json: r.get("metadata_json"),
        }))
    }

    /// List customizations for a tenant
    pub async fn list_tenant_customizations(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TenantPolicyCustomization>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, base_policy_type, customizations_json, status, 
                   submitted_at, reviewed_at, reviewed_by, review_notes, activated_at,
                   created_at, created_by, updated_at, metadata_json
            FROM tenant_policy_customizations
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list customizations: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| TenantPolicyCustomization {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                base_policy_type: r.get("base_policy_type"),
                customizations_json: r.get("customizations_json"),
                status: CustomizationStatus::from_str(r.get("status"))
                    .unwrap_or(CustomizationStatus::Draft),
                submitted_at: r.get("submitted_at"),
                reviewed_at: r.get("reviewed_at"),
                reviewed_by: r.get("reviewed_by"),
                review_notes: r.get("review_notes"),
                activated_at: r.get("activated_at"),
                created_at: r.get("created_at"),
                created_by: r.get("created_by"),
                updated_at: r.get("updated_at"),
                metadata_json: r.get("metadata_json"),
            })
            .collect())
    }

    /// List pending review customizations
    pub async fn list_pending_reviews(&self) -> Result<Vec<TenantPolicyCustomization>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, base_policy_type, customizations_json, status, 
                   submitted_at, reviewed_at, reviewed_by, review_notes, activated_at,
                   created_at, created_by, updated_at, metadata_json
            FROM tenant_policy_customizations
            WHERE status = 'pending_review'
            ORDER BY submitted_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list pending reviews: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| TenantPolicyCustomization {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                base_policy_type: r.get("base_policy_type"),
                customizations_json: r.get("customizations_json"),
                status: CustomizationStatus::from_str(r.get("status"))
                    .unwrap_or(CustomizationStatus::Draft),
                submitted_at: r.get("submitted_at"),
                reviewed_at: r.get("reviewed_at"),
                reviewed_by: r.get("reviewed_by"),
                review_notes: r.get("review_notes"),
                activated_at: r.get("activated_at"),
                created_at: r.get("created_at"),
                created_by: r.get("created_by"),
                updated_at: r.get("updated_at"),
                metadata_json: r.get("metadata_json"),
            })
            .collect())
    }

    /// Submit customization for review
    pub async fn submit_for_review(&self, id: &str, submitted_by: &str) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::Draft {
            return Err(AosError::Validation(format!(
                "Cannot submit customization in {} status",
                customization.status.as_str()
            )));
        }

        let now = Utc::now().to_rfc3339();
        let new_status = CustomizationStatus::PendingReview;

        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET status = ?, submitted_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_status.as_str())
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to submit for review: {}", e)))?;

        self.add_history_entry(
            id,
            "submitted",
            submitted_by,
            Some(customization.status.as_str()),
            Some(new_status.as_str()),
            Some("Submitted for review"),
            None,
        )
        .await?;

        info!(customization_id = %id, "Submitted customization for review");
        Ok(())
    }

    /// Approve customization
    pub async fn approve_customization(
        &self,
        id: &str,
        reviewed_by: &str,
        notes: Option<&str>,
    ) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::PendingReview {
            return Err(AosError::Validation(format!(
                "Cannot approve customization in {} status",
                customization.status.as_str()
            )));
        }

        let now = Utc::now().to_rfc3339();
        let new_status = CustomizationStatus::Approved;

        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET status = ?, reviewed_at = ?, reviewed_by = ?, review_notes = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_status.as_str())
        .bind(&now)
        .bind(reviewed_by)
        .bind(notes)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to approve customization: {}", e)))?;

        self.add_history_entry(
            id,
            "approved",
            reviewed_by,
            Some(customization.status.as_str()),
            Some(new_status.as_str()),
            notes,
            None,
        )
        .await?;

        info!(customization_id = %id, reviewed_by = %reviewed_by, "Approved customization");
        Ok(())
    }

    /// Reject customization
    pub async fn reject_customization(
        &self,
        id: &str,
        reviewed_by: &str,
        notes: Option<&str>,
    ) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::PendingReview {
            return Err(AosError::Validation(format!(
                "Cannot reject customization in {} status",
                customization.status.as_str()
            )));
        }

        let now = Utc::now().to_rfc3339();
        let new_status = CustomizationStatus::Rejected;

        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET status = ?, reviewed_at = ?, reviewed_by = ?, review_notes = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_status.as_str())
        .bind(&now)
        .bind(reviewed_by)
        .bind(notes)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to reject customization: {}", e)))?;

        self.add_history_entry(
            id,
            "rejected",
            reviewed_by,
            Some(customization.status.as_str()),
            Some(new_status.as_str()),
            notes,
            None,
        )
        .await?;

        info!(customization_id = %id, reviewed_by = %reviewed_by, "Rejected customization");
        Ok(())
    }

    /// Activate approved customization
    pub async fn activate_customization(&self, id: &str, activated_by: &str) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::Approved {
            return Err(AosError::Validation(format!(
                "Cannot activate customization in {} status (must be approved)",
                customization.status.as_str()
            )));
        }

        // Deactivate any existing active customization for the same tenant/policy type
        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET status = 'draft', updated_at = ?
            WHERE tenant_id = ? AND base_policy_type = ? AND status = 'active' AND id != ?
            "#,
        )
        .bind(&Utc::now().to_rfc3339())
        .bind(&customization.tenant_id)
        .bind(&customization.base_policy_type)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to deactivate existing customizations: {}",
                e
            ))
        })?;

        let now = Utc::now().to_rfc3339();
        let new_status = CustomizationStatus::Active;

        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET status = ?, activated_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_status.as_str())
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to activate customization: {}", e)))?;

        self.add_history_entry(
            id,
            "activated",
            activated_by,
            Some(customization.status.as_str()),
            Some(new_status.as_str()),
            Some("Activated policy customization"),
            None,
        )
        .await?;

        info!(customization_id = %id, activated_by = %activated_by, "Activated customization");
        Ok(())
    }

    /// Update customization (draft only)
    pub async fn update_customization(
        &self,
        id: &str,
        customizations_json: &str,
        updated_by: &str,
    ) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::Draft {
            return Err(AosError::Validation(format!(
                "Cannot update customization in {} status (must be draft)",
                customization.status.as_str()
            )));
        }

        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE tenant_policy_customizations
            SET customizations_json = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(customizations_json)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update customization: {}", e)))?;

        self.add_history_entry(
            id,
            "updated",
            updated_by,
            Some(customization.status.as_str()),
            Some(customization.status.as_str()),
            Some("Updated customization values"),
            None,
        )
        .await?;

        info!(customization_id = %id, updated_by = %updated_by, "Updated customization");
        Ok(())
    }

    /// Delete customization (draft only)
    pub async fn delete_customization(&self, id: &str) -> Result<()> {
        let customization = self
            .get_customization(id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Customization {} not found", id)))?;

        if customization.status != CustomizationStatus::Draft {
            return Err(AosError::Validation(format!(
                "Cannot delete customization in {} status (must be draft)",
                customization.status.as_str()
            )));
        }

        sqlx::query("DELETE FROM tenant_policy_customizations WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete customization: {}", e)))?;

        info!(customization_id = %id, "Deleted customization");
        Ok(())
    }

    /// Get active customization for tenant/policy type
    pub async fn get_active_customization(
        &self,
        tenant_id: &str,
        policy_type: &str,
    ) -> Result<Option<TenantPolicyCustomization>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, base_policy_type, customizations_json, status, 
                   submitted_at, reviewed_at, reviewed_by, review_notes, activated_at,
                   created_at, created_by, updated_at, metadata_json
            FROM tenant_policy_customizations
            WHERE tenant_id = ? AND base_policy_type = ? AND status = 'active'
            ORDER BY activated_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(policy_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active customization: {}", e)))?;

        Ok(row.map(|r| TenantPolicyCustomization {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            base_policy_type: r.get("base_policy_type"),
            customizations_json: r.get("customizations_json"),
            status: CustomizationStatus::from_str(r.get("status"))
                .unwrap_or(CustomizationStatus::Draft),
            submitted_at: r.get("submitted_at"),
            reviewed_at: r.get("reviewed_at"),
            reviewed_by: r.get("reviewed_by"),
            review_notes: r.get("review_notes"),
            activated_at: r.get("activated_at"),
            created_at: r.get("created_at"),
            created_by: r.get("created_by"),
            updated_at: r.get("updated_at"),
            metadata_json: r.get("metadata_json"),
        }))
    }

    /// Get history for a customization
    pub async fn get_customization_history(
        &self,
        customization_id: &str,
    ) -> Result<Vec<CustomizationHistoryEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, customization_id, action, performed_by, performed_at,
                   old_status, new_status, notes, changes_json
            FROM tenant_policy_customization_history
            WHERE customization_id = ?
            ORDER BY performed_at DESC
            "#,
        )
        .bind(customization_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get history: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| CustomizationHistoryEntry {
                id: r.get("id"),
                customization_id: r.get("customization_id"),
                action: r.get("action"),
                performed_by: r.get("performed_by"),
                performed_at: r.get("performed_at"),
                old_status: r.get("old_status"),
                new_status: r.get("new_status"),
                notes: r.get("notes"),
                changes_json: r.get("changes_json"),
            })
            .collect())
    }

    /// Add history entry (internal)
    async fn add_history_entry(
        &self,
        customization_id: &str,
        action: &str,
        performed_by: &str,
        old_status: Option<&str>,
        new_status: Option<&str>,
        notes: Option<&str>,
        changes_json: Option<&str>,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO tenant_policy_customization_history
            (id, customization_id, action, performed_by, performed_at, old_status, new_status, notes, changes_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(customization_id)
        .bind(action)
        .bind(performed_by)
        .bind(&now)
        .bind(old_status)
        .bind(new_status)
        .bind(notes)
        .bind(changes_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to add history entry: {}", e)))?;

        Ok(())
    }
}
