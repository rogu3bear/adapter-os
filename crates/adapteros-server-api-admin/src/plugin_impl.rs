//! PluginRegistry trait implementation for adapteros-server-api's PluginRegistry
//!
//! This module bridges the concrete PluginRegistry from adapteros-server-api
//! to the PluginRegistry trait required by admin handlers.

use crate::state::PluginRegistry as PluginRegistryTrait;
use adapteros_core::{PluginHealth, Result};
use adapteros_server_api::plugin_registry::PluginRegistry;
use std::collections::HashMap;

impl PluginRegistryTrait for PluginRegistry {
    async fn enable_for_tenant(&self, name: &str, tenant_id: &str, enabled: bool) -> Result<()> {
        self.enable_for_tenant(name, tenant_id, enabled).await
    }

    async fn is_enabled_for_tenant(&self, name: &str, tenant_id: &str) -> Result<bool> {
        self.is_enabled_for_tenant(name, tenant_id).await
    }

    async fn health_all(&self) -> HashMap<String, HashMap<String, PluginHealth>> {
        self.health_all().await
    }
}
