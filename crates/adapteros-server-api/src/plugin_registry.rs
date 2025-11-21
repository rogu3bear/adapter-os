//! Plugin registry for managing server plugins
//!
//! This module was moved from adapteros-server to break circular dependencies.

use adapteros_core::{Plugin, PluginConfig, PluginHealth, PluginStatus, Result};
use adapteros_db::tenants::Tenant;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, warn};

#[derive(Clone)]
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn Plugin + Send + Sync>>>>,
    tasks: Arc<RwLock<HashMap<String, JoinHandle<Result<()>>>>>,
    db: adapteros_db::Db,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry").finish()
    }
}

impl PluginRegistry {
    pub fn new(db: adapteros_db::Db) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    pub async fn register<P>(&self, name: String, plugin: P, config: PluginConfig) -> Result<()>
    where
        P: Plugin + 'static + Send + Sync,
    {
        let arc_plugin: Arc<dyn Plugin + Send + Sync> = Arc::new(plugin);
        let mut plugins = self.plugins.write().await;
        plugins.insert(name.clone(), arc_plugin.clone());

        // Load the plugin (Unloaded -> Loading -> Loaded)
        arc_plugin.load(&config).await?;

        // Start and spawn supervisor task
        self.start_plugin(&name, arc_plugin, config).await?;

        Ok(())
    }

    async fn start_plugin(
        &self,
        name: &str,
        plugin: Arc<dyn Plugin + Send + Sync>,
        config: PluginConfig,
    ) -> Result<()> {
        // Start the plugin (Loaded -> Starting -> Started)
        plugin.start().await?;

        // Spawn supervisor task
        let plugin_clone = plugin.clone();
        let name_clone = name.to_string();
        let config_clone = config.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                match plugin_clone.health_check().await {
                    Ok(health) => match health.status {
                        PluginStatus::Failed(_) => {
                            warn!("Plugin {} has failed, attempting restart", name_clone);
                            if let Err(e) = plugin_clone.reload(&config_clone).await {
                                error!("Failed to restart plugin {}: {}", name_clone, e);
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        error!("Health check failed for plugin {}: {}", name_clone, e);
                    }
                }
            }
        });

        let mut tasks = self.tasks.write().await;
        tasks.insert(name.to_string(), handle);

        Ok(())
    }

    pub async fn stop_plugin(&self, name: &str) -> Result<()> {
        let plugins = self.plugins.read().await;
        if let Some(plugin) = plugins.get(name) {
            let _ = plugin.stop().await;
        }

        let mut tasks = self.tasks.write().await;
        if let Some(handle) = tasks.remove(name) {
            handle.abort();
        }

        Ok(())
    }

    pub async fn enable_for_tenant(
        &self,
        plugin_name: &str,
        tenant_id: &str,
        enabled: bool,
    ) -> Result<()> {
        self.db
            .set_plugin_enable(tenant_id, plugin_name, enabled)
            .await?;
        if let Some(plugin) = self.plugins.read().await.get(plugin_name) {
            plugin.set_tenant_enabled(tenant_id, enabled).await?;
        }
        Ok(())
    }

    pub async fn is_enabled_for_tenant(&self, plugin_name: &str, tenant_id: &str) -> Result<bool> {
        match self.db.get_plugin_enable(tenant_id, plugin_name).await? {
            Some(e) => Ok(e),
            None => Ok(true), // default enabled
        }
    }

    pub async fn health_all(&self) -> HashMap<String, HashMap<String, PluginHealth>> {
        let tenants_result = self.db.list_tenants().await;
        let tenants = if let Ok(ts) = tenants_result {
            ts.into_iter().map(|t: Tenant| t.id).collect::<Vec<_>>()
        } else {
            vec!["default".to_string()] // fallback
        };

        let mut overall = HashMap::new();
        let plugins = self.plugins.read().await;
        for (name, _plugin) in plugins.iter() {
            let mut tenant_healths = HashMap::new();
            for tenant in &tenants {
                let enabled = self
                    .is_enabled_for_tenant(name, tenant)
                    .await
                    .unwrap_or(false);
                let health = PluginHealth {
                    status: if enabled {
                        PluginStatus::Started
                    } else {
                        PluginStatus::Stopped
                    },
                    details: None,
                };
                tenant_healths.insert(tenant.clone(), health);
            }
            overall.insert(name.clone(), tenant_healths);
        }
        overall
    }
}
