use crate::Result;
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
    /// Plugin not loaded
    Unloaded,
    /// Plugin is being loaded
    Loading,
    /// Plugin successfully loaded
    Loaded,
    /// Plugin is starting
    Starting,
    /// Plugin successfully started and running
    Started,
    /// Plugin is stopping
    Stopping,
    /// Plugin successfully stopped
    Stopped,
    /// Plugin degraded but still functional (e.g., partial tenant failures)
    Degraded(String),
    /// Plugin completely failed and needs restart
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHealth {
    pub status: PluginStatus,
    pub details: Option<String>,
}

/// Event hook types for plugin subscriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventHookType {
    /// Fired when a training job status changes
    OnTrainingJobEvent,
    /// Fired when an adapter is registered
    OnAdapterRegistered,
    /// Fired when an adapter is loaded into memory
    OnAdapterLoaded,
    /// Fired when an adapter is unloaded from memory
    OnAdapterUnloaded,
    /// Fired when an audit event occurs
    OnAuditEvent,
    /// Fired on periodic metrics collection
    OnMetricsTick,
    /// Fired when inference completes
    OnInferenceComplete,
    /// Fired when a policy violation is detected
    OnPolicyViolation,
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

    /// Returns the list of event types this plugin subscribes to
    fn subscribed_events(&self) -> Vec<EventHookType> {
        vec![]
    }

    /// Called when a subscribed event occurs
    async fn on_event(&self, _event: &crate::plugin_events::PluginEvent) -> Result<()> {
        Ok(())
    }
}
