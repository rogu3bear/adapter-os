use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Plugin configuration record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PluginConfig {
    pub id: String,
    pub plugin_name: String,
    pub enabled: bool,
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Per-tenant plugin enablement record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PluginTenantEnable {
    pub id: String,
    pub plugin_name: String,
    pub tenant_id: String,
    pub enabled: bool,
    pub config_override_json: Option<String>,
    pub created_at: String,
}

impl Db {
    /// Get plugin configuration by plugin name
    ///
    /// Returns `None` if plugin configuration does not exist.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// let config = db.get_plugin_config("code-intelligence").await.expect("query succeeds");
    /// if let Some(cfg) = config {
    ///     println!("Plugin {} is enabled: {}", cfg.plugin_name, cfg.enabled);
    /// }
    /// # }
    /// ```
    pub async fn get_plugin_config(&self, plugin_name: &str) -> Result<Option<PluginConfig>> {
        sqlx::query_as::<_, PluginConfig>("SELECT * FROM plugin_configs WHERE plugin_name = ?")
            .bind(plugin_name)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get plugin config: {}", e)))
    }

    /// Upsert plugin configuration (insert or update if exists)
    ///
    /// # Arguments
    /// - `plugin_name` - Unique plugin identifier
    /// - `enabled` - Whether the plugin is enabled globally
    /// - `config_json` - Optional JSON configuration string
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// db.upsert_plugin_config(
    ///     "code-intelligence",
    ///     true,
    ///     Some(r#"{"scan_interval": 300}"#)
    /// ).await.expect("upsert succeeds");
    /// # }
    /// ```
    pub async fn upsert_plugin_config(
        &self,
        plugin_name: &str,
        enabled: bool,
        config_json: Option<&str>,
    ) -> Result<()> {
        // Try to get existing config
        let existing = self.get_plugin_config(plugin_name).await?;

        if let Some(existing_config) = existing {
            // Update existing config
            sqlx::query(
                "UPDATE plugin_configs SET enabled = ?, config_json = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now') WHERE id = ?"
            )
            .bind(enabled)
            .bind(config_json)
            .bind(&existing_config.id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update plugin config: {}", e)))?;
        } else {
            // Insert new config
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO plugin_configs (id, plugin_name, enabled, config_json) VALUES (?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(plugin_name)
            .bind(enabled)
            .bind(config_json)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert plugin config: {}", e)))?;
        }

        Ok(())
    }

    /// List all plugin configurations
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// let configs = db.list_plugin_configs().await.expect("query succeeds");
    /// for cfg in configs {
    ///     println!("Plugin {}: enabled={}", cfg.plugin_name, cfg.enabled);
    /// }
    /// # }
    /// ```
    pub async fn list_plugin_configs(&self) -> Result<Vec<PluginConfig>> {
        sqlx::query_as::<_, PluginConfig>("SELECT * FROM plugin_configs ORDER BY plugin_name")
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list plugin configs: {}", e)))
    }

    /// Check if a plugin is enabled for a specific tenant
    ///
    /// This checks both:
    /// 1. Whether the plugin is globally enabled
    /// 2. Whether there's a tenant-specific override
    ///
    /// Tenant-specific overrides take precedence over global settings.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// let is_enabled = db.is_plugin_enabled_for_tenant("code-intelligence", "tenant-123")
    ///     .await
    ///     .expect("query succeeds");
    /// if is_enabled {
    ///     println!("Plugin is enabled for this tenant");
    /// }
    /// # }
    /// ```
    pub async fn is_plugin_enabled_for_tenant(
        &self,
        plugin_name: &str,
        tenant_id: &str,
    ) -> Result<bool> {
        // First check for tenant-specific override
        let tenant_override = sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE plugin_name = ? AND tenant_id = ?",
        )
        .bind(plugin_name)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to check tenant plugin override: {}", e))
        })?;

        // If tenant override exists, use that
        if let Some(override_record) = tenant_override {
            return Ok(override_record.enabled);
        }

        // Otherwise, check global plugin config
        let global_config = self.get_plugin_config(plugin_name).await?;
        Ok(global_config.map(|c| c.enabled).unwrap_or(false))
    }

    /// Enable a plugin for a specific tenant with optional config override
    ///
    /// # Arguments
    /// - `plugin_name` - Plugin identifier
    /// - `tenant_id` - Tenant identifier
    /// - `config_override` - Optional JSON config to override global settings
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// db.enable_plugin_for_tenant(
    ///     "code-intelligence",
    ///     "tenant-123",
    ///     Some(r#"{"scan_interval": 600}"#)
    /// ).await.expect("enable succeeds");
    /// # }
    /// ```
    pub async fn enable_plugin_for_tenant(
        &self,
        plugin_name: &str,
        tenant_id: &str,
        config_override: Option<&str>,
    ) -> Result<()> {
        // Check if tenant override already exists
        let existing = sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE plugin_name = ? AND tenant_id = ?",
        )
        .bind(plugin_name)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to check existing tenant override: {}", e))
        })?;

        if let Some(existing_record) = existing {
            // Update existing override
            sqlx::query(
                "UPDATE plugin_tenant_enables SET enabled = ?, config_override_json = ? WHERE id = ?"
            )
            .bind(true)
            .bind(config_override)
            .bind(&existing_record.id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update tenant plugin override: {}", e)))?;
        } else {
            // Insert new override
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO plugin_tenant_enables (id, plugin_name, tenant_id, enabled, config_override_json) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(plugin_name)
            .bind(tenant_id)
            .bind(true)
            .bind(config_override)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert tenant plugin override: {}", e)))?;
        }

        Ok(())
    }

    /// Disable a plugin for a specific tenant
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// db.disable_plugin_for_tenant("code-intelligence", "tenant-123")
    ///     .await
    ///     .expect("disable succeeds");
    /// # }
    /// ```
    pub async fn disable_plugin_for_tenant(
        &self,
        plugin_name: &str,
        tenant_id: &str,
    ) -> Result<()> {
        // Check if tenant override exists
        let existing = sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE plugin_name = ? AND tenant_id = ?",
        )
        .bind(plugin_name)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to check existing tenant override: {}", e))
        })?;

        if let Some(existing_record) = existing {
            // Update to disabled
            sqlx::query("UPDATE plugin_tenant_enables SET enabled = ? WHERE id = ?")
                .bind(false)
                .bind(&existing_record.id)
                .execute(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to disable tenant plugin: {}", e))
                })?;
        } else {
            // Insert new override with disabled state
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO plugin_tenant_enables (id, plugin_name, tenant_id, enabled) VALUES (?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(plugin_name)
            .bind(tenant_id)
            .bind(false)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert tenant plugin override: {}", e)))?;
        }

        Ok(())
    }

    /// List all plugin enablements for a specific tenant
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) {
    /// let tenant_plugins = db.list_tenant_plugin_enables("tenant-123")
    ///     .await
    ///     .expect("query succeeds");
    /// for plugin in tenant_plugins {
    ///     println!("Plugin {}: enabled={}", plugin.plugin_name, plugin.enabled);
    /// }
    /// # }
    /// ```
    pub async fn list_tenant_plugin_enables(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<PluginTenantEnable>> {
        sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE tenant_id = ? ORDER BY plugin_name",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list tenant plugin enables: {}", e)))
    }
}
