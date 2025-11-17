use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub specific: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginStatus {
    Loaded,
    Started,
    Stopped,
    Degraded(String),
    Dead(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHealth {
    pub status: PluginStatus,
    pub details: Option<String>,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;

    async fn load(&self, config: &PluginConfig) -> Result<(), String> {
        Ok(())
    }

    async fn start(&self) -> Result<(), String> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), String> {
        Ok(())
    }

    async fn reload(&self, config: &PluginConfig) -> Result<(), String> {
        self.stop().await?;
        self.load(config).await?;
        self.start().await
    }

    async fn health_check(&self) -> Result<PluginHealth, String> {
        Ok(PluginHealth {
            status: PluginStatus::Started,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<(), String> {
        Ok(())
    }
}
