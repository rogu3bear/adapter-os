//! Prefix Templates Database Module
//!
//! CRUD operations for prefix templates used in prefix KV caching.
//! Prefix templates define static prefix text for tenant+mode combinations
//! that can be cached to skip redundant prefill computation.

use crate::Db;
use adapteros_api_types::prefix_templates::{
    CreatePrefixTemplateRequest, PrefixMode, PrefixTemplate, UpdatePrefixTemplateRequest,
};
use adapteros_core::{AosError, B3Hash, Result};
use sqlx::Row;

impl Db {
    /// Create a new prefix template.
    ///
    /// The template_hash_b3 is automatically computed from the template_text.
    pub async fn create_prefix_template(
        &self,
        req: CreatePrefixTemplateRequest,
    ) -> Result<PrefixTemplate> {
        let id = crate::new_id(adapteros_id::IdPrefix::Pol);
        let template_hash = B3Hash::hash(req.template_text.as_bytes());
        let mode_str = req.mode.as_str();

        sqlx::query(
            r#"
            INSERT INTO prefix_templates (id, tenant_id, mode, template_text, template_hash_b3, priority, enabled)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(mode_str)
        .bind(&req.template_text)
        .bind(template_hash.to_hex())
        .bind(req.priority.unwrap_or(0))
        .bind(req.enabled.unwrap_or(true))
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(PrefixTemplate {
            id,
            tenant_id: req.tenant_id,
            mode: req.mode,
            template_text: req.template_text,
            template_hash_b3: template_hash,
            priority: req.priority.unwrap_or(0),
            enabled: req.enabled.unwrap_or(true),
        })
    }

    /// Get a prefix template by ID.
    pub async fn get_prefix_template(&self, id: &str) -> Result<Option<PrefixTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, mode, template_text, template_hash_b3, priority, enabled
            FROM prefix_templates
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        match row {
            Some(row) => Ok(Some(row_to_prefix_template(&row)?)),
            None => Ok(None),
        }
    }

    /// List prefix templates for a tenant.
    ///
    /// Returns templates ordered by priority (descending), then by created_at.
    pub async fn list_prefix_templates(&self, tenant_id: &str) -> Result<Vec<PrefixTemplate>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, mode, template_text, template_hash_b3, priority, enabled
            FROM prefix_templates
            WHERE tenant_id = ?
            ORDER BY priority DESC, created_at ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        rows.iter().map(row_to_prefix_template).collect()
    }

    /// Get the best matching prefix template for a tenant and mode.
    ///
    /// Returns the highest-priority enabled template matching the tenant and mode.
    /// Falls back to system-level templates if no tenant-specific template exists.
    pub async fn get_prefix_template_for_mode(
        &self,
        tenant_id: &str,
        mode: &PrefixMode,
    ) -> Result<Option<PrefixTemplate>> {
        let mode_str = mode.as_str();

        // First try tenant-specific template
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, mode, template_text, template_hash_b3, priority, enabled
            FROM prefix_templates
            WHERE tenant_id = ? AND mode = ? AND enabled = 1
            ORDER BY priority DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(mode_str)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(row) = row {
            return Ok(Some(row_to_prefix_template(&row)?));
        }

        // Fall back to system-level template (mode = 'system' for the same tenant)
        if !matches!(mode, PrefixMode::System) {
            let system_row = sqlx::query(
                r#"
                SELECT id, tenant_id, mode, template_text, template_hash_b3, priority, enabled
                FROM prefix_templates
                WHERE tenant_id = ? AND mode = 'system' AND enabled = 1
                ORDER BY priority DESC
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .fetch_optional(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if let Some(row) = system_row {
                return Ok(Some(row_to_prefix_template(&row)?));
            }
        }

        Ok(None)
    }

    /// Update a prefix template.
    ///
    /// Recalculates template_hash_b3 if template_text is updated.
    pub async fn update_prefix_template(
        &self,
        id: &str,
        req: UpdatePrefixTemplateRequest,
    ) -> Result<Option<PrefixTemplate>> {
        // First get existing template
        let existing = self.get_prefix_template(id).await?;
        let existing = match existing {
            Some(t) => t,
            None => return Ok(None),
        };

        // Build update values
        let template_text = req.template_text.unwrap_or(existing.template_text);
        let template_hash = B3Hash::hash(template_text.as_bytes());
        let mode = req.mode.unwrap_or(existing.mode);
        let priority = req.priority.unwrap_or(existing.priority);
        let enabled = req.enabled.unwrap_or(existing.enabled);

        sqlx::query(
            r#"
            UPDATE prefix_templates
            SET mode = ?, template_text = ?, template_hash_b3 = ?, priority = ?, enabled = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(mode.as_str())
        .bind(&template_text)
        .bind(template_hash.to_hex())
        .bind(priority)
        .bind(enabled)
        .bind(id)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(Some(PrefixTemplate {
            id: id.to_string(),
            tenant_id: existing.tenant_id,
            mode,
            template_text,
            template_hash_b3: template_hash,
            priority,
            enabled,
        }))
    }

    /// Delete a prefix template.
    pub async fn delete_prefix_template(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM prefix_templates WHERE id = ?")
            .bind(id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all prefix templates for a tenant.
    pub async fn delete_prefix_templates_for_tenant(&self, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM prefix_templates WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(result.rows_affected())
    }
}

/// Convert a database row to a PrefixTemplate.
fn row_to_prefix_template(row: &sqlx::sqlite::SqliteRow) -> Result<PrefixTemplate> {
    let id: String = row.get("id");
    let tenant_id: String = row.get("tenant_id");
    let mode_str: String = row.get("mode");
    let template_text: String = row.get("template_text");
    let template_hash_hex: String = row.get("template_hash_b3");
    let priority: i32 = row.get("priority");
    let enabled: bool = row.get("enabled");

    let mode = PrefixMode::parse_mode(&mode_str);
    let template_hash = B3Hash::from_hex(&template_hash_hex)
        .map_err(|e| AosError::Database(format!("Invalid template hash: {}", e)))?;

    Ok(PrefixTemplate {
        id,
        tenant_id,
        mode,
        template_text,
        template_hash_b3: template_hash,
        priority,
        enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    async fn setup_db() -> Arc<Db> {
        let db = Arc::new(Db::new_in_memory().await.expect("Failed to create test db"));
        // Create tenant for FK constraint
        sqlx::query(
            "INSERT INTO tenants (id, name, itar_flag) VALUES ('tenant-1', 'Test Tenant', 0)",
        )
        .execute(db.pool_result().unwrap())
        .await
        .expect("Failed to create test tenant");
        db
    }

    #[tokio::test]
    async fn test_create_and_get_prefix_template() -> Result<()> {
        let db = setup_db().await;

        let req = CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "You are a helpful assistant.".to_string(),
            priority: Some(10),
            enabled: Some(true),
        };

        let template = db.create_prefix_template(req).await.unwrap();
        assert_eq!(template.tenant_id, "tenant-1");
        assert_eq!(template.mode, PrefixMode::User);
        assert_eq!(template.priority, 10);
        assert!(template.enabled);

        // Verify hash is correct
        let expected_hash = B3Hash::hash(b"You are a helpful assistant.");
        assert_eq!(template.template_hash_b3, expected_hash);

        // Get by ID
        let fetched = db.get_prefix_template(&template.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, template.id);
        assert_eq!(fetched.template_text, "You are a helpful assistant.");

        Ok(())
    }

    #[tokio::test]
    async fn test_list_prefix_templates() -> Result<()> {
        let db = setup_db().await;

        // Create multiple templates
        for (mode, priority) in [
            (PrefixMode::User, 10),
            (PrefixMode::Builder, 20),
            (PrefixMode::System, 5),
        ] {
            let template_text = format!("Template for {:?}", mode);
            db.create_prefix_template(CreatePrefixTemplateRequest {
                tenant_id: "tenant-1".to_string(),
                mode,
                template_text,
                priority: Some(priority),
                enabled: Some(true),
            })
            .await
            .unwrap();
        }

        let templates = db.list_prefix_templates("tenant-1").await.unwrap();
        assert_eq!(templates.len(), 3);

        // Should be ordered by priority descending
        assert_eq!(templates[0].priority, 20); // Builder
        assert_eq!(templates[1].priority, 10); // User
        assert_eq!(templates[2].priority, 5); // System

        Ok(())
    }

    #[tokio::test]
    async fn test_get_prefix_template_for_mode() -> Result<()> {
        let db = setup_db().await;

        // Create user-mode template
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "User mode prefix".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

        // Create system-mode template (fallback)
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::System,
            template_text: "System fallback prefix".to_string(),
            priority: Some(5),
            enabled: Some(true),
        })
        .await
        .unwrap();

        // Get user mode - should return user template
        let user_template = db
            .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user_template.template_text, "User mode prefix");

        // Get builder mode - should fall back to system
        let builder_template = db
            .get_prefix_template_for_mode("tenant-1", &PrefixMode::Builder)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(builder_template.template_text, "System fallback prefix");

        // Get audit mode for different tenant - should return None
        let none_template = db
            .get_prefix_template_for_mode("tenant-2", &PrefixMode::Audit)
            .await
            .unwrap();
        assert!(none_template.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_update_prefix_template() -> Result<()> {
        let db = setup_db().await;

        let template = db
            .create_prefix_template(CreatePrefixTemplateRequest {
                tenant_id: "tenant-1".to_string(),
                mode: PrefixMode::User,
                template_text: "Original text".to_string(),
                priority: Some(10),
                enabled: Some(true),
            })
            .await
            .unwrap();

        // Update template text and priority
        let updated = db
            .update_prefix_template(
                &template.id,
                UpdatePrefixTemplateRequest {
                    mode: None,
                    template_text: Some("Updated text".to_string()),
                    priority: Some(20),
                    enabled: None,
                },
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.template_text, "Updated text");
        assert_eq!(updated.priority, 20);
        assert!(updated.enabled); // Unchanged

        // Verify hash was recomputed
        let expected_hash = B3Hash::hash(b"Updated text");
        assert_eq!(updated.template_hash_b3, expected_hash);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_prefix_template() -> Result<()> {
        let db = setup_db().await;

        let template = db
            .create_prefix_template(CreatePrefixTemplateRequest {
                tenant_id: "tenant-1".to_string(),
                mode: PrefixMode::User,
                template_text: "To be deleted".to_string(),
                priority: None,
                enabled: None,
            })
            .await
            .unwrap();

        // Delete
        let deleted = db.delete_prefix_template(&template.id).await.unwrap();
        assert!(deleted);

        // Verify it's gone
        let fetched = db.get_prefix_template(&template.id).await.unwrap();
        assert!(fetched.is_none());

        // Delete again - should return false
        let deleted_again = db.delete_prefix_template(&template.id).await.unwrap();
        assert!(!deleted_again);

        Ok(())
    }
}
