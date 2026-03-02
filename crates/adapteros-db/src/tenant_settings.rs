//! Tenant settings for stack and pin defaults
//!
//! Provides configurable behavior for:
//! - Stack inheritance on chat session creation
//! - Stack fallback on inference with session
//!
//! All settings default to FALSE (disabled) for backwards compatibility.

use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Tenant settings for controlling default stack/adapter behavior
///
/// These settings are opt-in (all default to FALSE) to maintain backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TenantSettings {
    pub tenant_id: String,
    /// When TRUE: new chat sessions inherit stack_id from tenants.default_stack_id
    pub use_default_stack_on_chat_create: bool,
    /// When TRUE: inference with session_id falls back to tenant default stack
    /// Only applies when no adapters/stack specified in request
    pub use_default_stack_on_infer_session: bool,
    /// Extensible JSON for experimental flags
    #[sqlx(default)]
    pub settings_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Parameters for updating tenant settings
///
/// All fields are optional to support partial updates.
#[derive(Debug, Clone, Default)]
pub struct UpdateTenantSettingsParams {
    pub use_default_stack_on_chat_create: Option<bool>,
    pub use_default_stack_on_infer_session: Option<bool>,
    pub settings_json: Option<String>,
}

/// Default settings when no row exists in the database
impl Default for TenantSettings {
    fn default() -> Self {
        Self {
            tenant_id: String::new(),
            use_default_stack_on_chat_create: false,
            use_default_stack_on_infer_session: false,
            settings_json: Some("{}".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

impl TenantSettings {
    /// Create default settings for a tenant
    pub fn defaults_for(tenant_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            ..Default::default()
        }
    }
}

impl Db {
    /// Get tenant settings, returning defaults if not configured
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// TenantSettings with defaults if no row exists
    pub async fn get_tenant_settings(&self, tenant_id: &str) -> Result<TenantSettings> {
        debug!(tenant_id = %tenant_id, "Getting tenant settings");

        let result = sqlx::query_as::<_, TenantSettings>(
            r#"
            SELECT tenant_id, use_default_stack_on_chat_create, use_default_stack_on_infer_session,
                   settings_json, created_at, updated_at
            FROM tenant_settings
            WHERE tenant_id = ?
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get tenant settings"))?;

        match result {
            Some(settings) => Ok(settings),
            None => {
                debug!(tenant_id = %tenant_id, "No tenant settings found, using defaults");
                Ok(TenantSettings::defaults_for(tenant_id))
            }
        }
    }

    /// Upsert tenant settings (insert or update)
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `params` - The settings to update (None values preserve existing)
    ///
    /// # Returns
    /// Updated TenantSettings
    pub async fn upsert_tenant_settings(
        &self,
        tenant_id: &str,
        params: UpdateTenantSettingsParams,
    ) -> Result<TenantSettings> {
        debug!(tenant_id = %tenant_id, ?params, "Upserting tenant settings");

        // First, get existing settings (or defaults)
        let existing = self.get_tenant_settings(tenant_id).await?;

        // Merge with provided params
        let use_default_stack_on_chat_create = params
            .use_default_stack_on_chat_create
            .unwrap_or(existing.use_default_stack_on_chat_create);
        let use_default_stack_on_infer_session = params
            .use_default_stack_on_infer_session
            .unwrap_or(existing.use_default_stack_on_infer_session);
        let settings_json = params.settings_json.or(existing.settings_json);

        // Upsert (INSERT OR REPLACE)
        sqlx::query(
            r#"
            INSERT INTO tenant_settings (
                tenant_id,
                use_default_stack_on_chat_create,
                use_default_stack_on_infer_session,
                settings_json
            ) VALUES (?, ?, ?, ?)
            ON CONFLICT(tenant_id) DO UPDATE SET
                use_default_stack_on_chat_create = excluded.use_default_stack_on_chat_create,
                use_default_stack_on_infer_session = excluded.use_default_stack_on_infer_session,
                settings_json = excluded.settings_json,
                updated_at = datetime('now')
            "#,
        )
        .bind(tenant_id)
        .bind(use_default_stack_on_chat_create as i32)
        .bind(use_default_stack_on_infer_session as i32)
        .bind(&settings_json)
        .execute(self.pool())
        .await
        .map_err(db_err("upsert tenant settings"))?;

        info!(tenant_id = %tenant_id, "Tenant settings upserted");

        // Return the updated settings
        self.get_tenant_settings(tenant_id).await
    }

    /// Check if stack should be inherited on chat session creation
    ///
    /// Optimized single-column query for the hot path.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// true if stack should be inherited, false otherwise (default)
    pub async fn should_inherit_stack_on_chat_create(&self, tenant_id: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i32>(
            "SELECT use_default_stack_on_chat_create FROM tenant_settings WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("check inherit stack on chat create"))?;

        Ok(result.map(|v| v != 0).unwrap_or(false))
    }

    /// Check if inference should fall back to tenant default stack
    ///
    /// Optimized single-column query for the inference hot path.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// true if fallback is enabled, false otherwise (default)
    pub async fn should_fallback_stack_on_infer(&self, tenant_id: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i32>(
            "SELECT use_default_stack_on_infer_session FROM tenant_settings WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("check fallback stack on infer"))?;

        Ok(result.map(|v| v != 0).unwrap_or(false))
    }

    /// Delete tenant settings
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// true if a row was deleted, false if no row existed
    pub async fn delete_tenant_settings(&self, tenant_id: &str) -> Result<bool> {
        debug!(tenant_id = %tenant_id, "Deleting tenant settings");

        let result = sqlx::query("DELETE FROM tenant_settings WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete tenant settings"))?;

        let deleted = result.rows_affected() > 0;
        if deleted {
            info!(tenant_id = %tenant_id, "Tenant settings deleted");
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = TenantSettings::defaults_for("test-tenant");
        assert_eq!(settings.tenant_id, "test-tenant");
        assert!(!settings.use_default_stack_on_chat_create);
        assert!(!settings.use_default_stack_on_infer_session);
        assert_eq!(settings.settings_json, Some("{}".to_string()));
    }

    #[test]
    fn test_update_params_default() {
        let params = UpdateTenantSettingsParams::default();
        assert!(params.use_default_stack_on_chat_create.is_none());
        assert!(params.use_default_stack_on_infer_session.is_none());
        assert!(params.settings_json.is_none());
    }
}
