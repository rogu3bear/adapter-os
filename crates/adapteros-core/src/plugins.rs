use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use crate::Result;

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

    async fn load(&self, _config: &PluginConfig) -> Result<()>;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn reload(&self, _config: &PluginConfig) -> Result<()>;
    async fn health_check(&self) -> Result<PluginHealth>;
    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()>;
}
