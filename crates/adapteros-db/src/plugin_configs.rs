use crate::plugin_configs_kv::{PluginConfigKv, PluginConfigKvRepository, PluginTenantEnableKv};
use crate::{Db, StorageMode};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;
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

impl From<PluginConfigKv> for PluginConfig {
    fn from(kv: PluginConfigKv) -> Self {
        Self {
            id: kv.id,
            plugin_name: kv.plugin_name,
            enabled: kv.enabled,
            config_json: kv.config_json,
            created_at: kv.created_at,
            updated_at: kv.updated_at,
        }
    }
}

impl From<PluginConfig> for PluginConfigKv {
    fn from(cfg: PluginConfig) -> Self {
        Self {
            id: cfg.id,
            plugin_name: cfg.plugin_name,
            enabled: cfg.enabled,
            config_json: cfg.config_json,
            created_at: cfg.created_at,
            updated_at: cfg.updated_at,
        }
    }
}

impl From<PluginTenantEnableKv> for PluginTenantEnable {
    fn from(kv: PluginTenantEnableKv) -> Self {
        Self {
            id: kv.id,
            plugin_name: kv.plugin_name,
            tenant_id: kv.tenant_id,
            enabled: kv.enabled,
            config_override_json: kv.config_override_json,
            created_at: kv.created_at,
        }
    }
}

impl From<PluginTenantEnable> for PluginTenantEnableKv {
    fn from(enable: PluginTenantEnable) -> Self {
        let created_at = enable.created_at.clone();

        Self {
            id: enable.id,
            plugin_name: enable.plugin_name,
            tenant_id: enable.tenant_id,
            enabled: enable.enabled,
            config_override_json: enable.config_override_json,
            created_at,
            updated_at: enable.created_at,
        }
    }
}

impl Db {
    fn get_plugin_config_kv_repo(&self) -> Option<PluginConfigKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend()
                .map(|kv| PluginConfigKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    async fn sql_get_plugin_config(&self, plugin_name: &str) -> Result<Option<PluginConfig>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(None);
        };

        sqlx::query_as::<_, PluginConfig>("SELECT * FROM plugin_configs WHERE plugin_name = ?")
            .bind(plugin_name)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get plugin config: {}", e)))
    }

    async fn sql_get_tenant_plugin(
        &self,
        plugin_name: &str,
        tenant_id: &str,
    ) -> Result<Option<PluginTenantEnable>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(None);
        };

        sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE plugin_name = ? AND tenant_id = ?",
        )
        .bind(plugin_name)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check tenant plugin override: {}", e)))
    }

    async fn sql_list_plugin_configs(&self) -> Result<Vec<PluginConfig>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

        sqlx::query_as::<_, PluginConfig>("SELECT * FROM plugin_configs ORDER BY plugin_name")
            .fetch_all(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list plugin configs: {}", e)))
    }

    async fn sql_list_tenant_enables(&self, tenant_id: &str) -> Result<Vec<PluginTenantEnable>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

        sqlx::query_as::<_, PluginTenantEnable>(
            "SELECT * FROM plugin_tenant_enables WHERE tenant_id = ? ORDER BY plugin_name",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list tenant plugin enables: {}", e)))
    }

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
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plugin_config_kv_repo() {
                if let Some(kv) = repo.get_plugin_config(plugin_name).await? {
                    return Ok(Some(kv.into()));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("plugin_configs.get");
        }

        self.sql_get_plugin_config(plugin_name).await
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
        let mut canonical: Option<PluginConfig> = None;

        if self.storage_mode().write_to_sql() {
            let existing = self.sql_get_plugin_config(plugin_name).await?;
            if let Some(pool) = self.pool_opt() {
                if let Some(existing_config) = existing {
                    sqlx::query(
                        "UPDATE plugin_configs SET enabled = ?, config_json = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now') WHERE id = ?"
                    )
                    .bind(enabled)
                    .bind(config_json)
                    .bind(&existing_config.id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to update plugin config: {}", e)))?;
                } else {
                    let id = Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO plugin_configs (id, plugin_name, enabled, config_json) VALUES (?, ?, ?, ?)"
                    )
                    .bind(&id)
                    .bind(plugin_name)
                    .bind(enabled)
                    .bind(config_json)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to insert plugin config: {}", e)))?;
                }
                canonical = self.sql_get_plugin_config(plugin_name).await?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for plugin config upsert".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_plugin_config_kv_repo() {
                let desired = if let Some(cfg) = canonical.clone() {
                    PluginConfigKv::from(cfg)
                } else {
                    let kv_existing = repo.get_plugin_config(plugin_name).await?;
                    repo.new_config_record(
                        plugin_name,
                        enabled,
                        config_json.map(|s| s.to_string()),
                        kv_existing,
                    )
                };

                if let Err(e) = repo.upsert_plugin_config(desired).await {
                    self.record_kv_write_fallback("plugin_configs.upsert");
                    return Err(e);
                }
            } else {
                return Err(AosError::Database(
                    "KV backend unavailable for plugin config upsert".to_string(),
                ));
            }
        }

        debug!(
            plugin_name = %plugin_name,
            enabled = enabled,
            mode = ?self.storage_mode(),
            "Upserted plugin config"
        );

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
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plugin_config_kv_repo() {
                let configs = repo
                    .list_plugin_configs()
                    .await?
                    .into_iter()
                    .map(PluginConfig::from)
                    .collect();
                return Ok(configs);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
            self.record_kv_read_fallback("plugin_configs.list");
        }

        self.sql_list_plugin_configs().await
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
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_plugin_config_kv_repo() {
                if let Some(override_record) =
                    repo.get_tenant_enable(tenant_id, plugin_name).await?
                {
                    return Ok(override_record.enabled);
                }

                if let Some(global) = repo.get_plugin_config(plugin_name).await? {
                    return Ok(global.enabled);
                }
            }

            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(false);
            }
            self.record_kv_read_fallback("plugin_configs.is_enabled");
        }

        // First check for tenant-specific override
        let tenant_override = self.sql_get_tenant_plugin(plugin_name, tenant_id).await?;

        // If tenant override exists, use that
        if let Some(override_record) = tenant_override {
            return Ok(override_record.enabled);
        }

        // Otherwise, check global plugin config
        let global_config = self.sql_get_plugin_config(plugin_name).await?;
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
        let mut canonical: Option<PluginTenantEnable> = None;

        if self.storage_mode().write_to_sql() {
            let existing = self.sql_get_tenant_plugin(plugin_name, tenant_id).await?;

            if let Some(pool) = self.pool_opt() {
                if let Some(existing_record) = existing {
                    sqlx::query(
                        "UPDATE plugin_tenant_enables SET enabled = ?, config_override_json = ? WHERE id = ?"
                    )
                    .bind(true)
                    .bind(config_override)
                    .bind(&existing_record.id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to update tenant plugin override: {}", e)))?;
                } else {
                    let id = Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO plugin_tenant_enables (id, plugin_name, tenant_id, enabled, config_override_json) VALUES (?, ?, ?, ?, ?)"
                    )
                    .bind(&id)
                    .bind(plugin_name)
                    .bind(tenant_id)
                    .bind(true)
                    .bind(config_override)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to insert tenant plugin override: {}", e)))?;
                }
                canonical = self.sql_get_tenant_plugin(plugin_name, tenant_id).await?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for tenant plugin enable".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_plugin_config_kv_repo() {
                let desired = if let Some(record) = canonical.clone() {
                    PluginTenantEnableKv::from(record)
                } else {
                    let existing = repo.get_tenant_enable(tenant_id, plugin_name).await?;
                    repo.new_tenant_record(
                        tenant_id,
                        plugin_name,
                        true,
                        config_override.map(|s| s.to_string()),
                        existing,
                    )
                };

                if let Err(e) = repo.upsert_tenant_enable(desired).await {
                    self.record_kv_write_fallback("plugin_configs.enable_tenant");
                    return Err(e);
                }
            } else {
                return Err(AosError::Database(
                    "KV backend unavailable for tenant plugin enable".to_string(),
                ));
            }
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
