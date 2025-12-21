//! Advanced Metrics Plugin
//!
//! This plugin provides enhanced metrics collection and reporting capabilities
//! for AdapterOS. It tracks detailed performance metrics including:
//!
//! - Inference latency percentiles (p50, p95, p99) per adapter
//! - Training job duration histograms
//! - Adapter activation patterns
//! - Token throughput per tenant
//!
//! The plugin subscribes to OnMetricsTick, OnInferenceComplete, and
//! OnTrainingJobEvent events and exposes metrics via a custom Prometheus endpoint.
//!
//! ## Usage
//!
//! ```no_run
//! use adapteros_plugin_advanced_metrics::AdvancedMetricsPlugin;
//! use adapteros_core::plugins::{Plugin, PluginConfig};
//!
//! # async fn example() -> adapteros_core::Result<()> {
//! let plugin = AdvancedMetricsPlugin::new();
//! let config = PluginConfig {
//!     name: "advanced-metrics".to_string(),
//!     enabled: true,
//!     specific: Default::default(),
//! };
//!
//! plugin.load(&config).await?;
//! plugin.start().await?;
//! # Ok(())
//! # }
//! ```

use adapteros_core::{
    plugin_events::PluginEvent,
    plugins::{EventHookType, Plugin, PluginConfig, PluginHealth, PluginStatus},
    Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

mod endpoints;
mod metrics;

pub use endpoints::{metrics_endpoint, metrics_json};
pub use metrics::MetricsCollector;

/// Advanced Metrics Plugin
///
/// Provides detailed performance metrics collection and Prometheus endpoint.
pub struct AdvancedMetricsPlugin {
    /// Metrics collector instance
    collector: Arc<RwLock<MetricsCollector>>,
    /// Plugin status
    status: Arc<RwLock<PluginStatus>>,
}

impl AdvancedMetricsPlugin {
    /// Create a new Advanced Metrics plugin instance
    pub fn new() -> Self {
        Self {
            collector: Arc::new(RwLock::new(MetricsCollector::new())),
            status: Arc::new(RwLock::new(PluginStatus::Unloaded)),
        }
    }

    /// Get a reference to the metrics collector
    pub fn collector(&self) -> Arc<RwLock<MetricsCollector>> {
        Arc::clone(&self.collector)
    }
}

impl Default for AdvancedMetricsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for AdvancedMetricsPlugin {
    fn name(&self) -> &'static str {
        "advanced-metrics"
    }

    async fn load(&self, config: &PluginConfig) -> Result<()> {
        info!(plugin = self.name(), "Loading advanced metrics plugin");
        *self.status.write().await = PluginStatus::Loading;

        // Validate configuration
        if !config.enabled {
            warn!(plugin = self.name(), "Plugin is disabled in configuration");
        }

        // Initialize metrics collector
        let mut collector = self.collector.write().await;
        collector.initialize()?;

        *self.status.write().await = PluginStatus::Loaded;
        info!(plugin = self.name(), "Advanced metrics plugin loaded");
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        info!(plugin = self.name(), "Starting advanced metrics plugin");
        *self.status.write().await = PluginStatus::Starting;

        // Metrics collector is passive - no background tasks needed
        *self.status.write().await = PluginStatus::Started;
        info!(plugin = self.name(), "Advanced metrics plugin started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!(plugin = self.name(), "Stopping advanced metrics plugin");
        *self.status.write().await = PluginStatus::Stopping;

        // No cleanup needed - metrics are in-memory
        *self.status.write().await = PluginStatus::Stopped;
        info!(plugin = self.name(), "Advanced metrics plugin stopped");
        Ok(())
    }

    async fn reload(&self, config: &PluginConfig) -> Result<()> {
        info!(plugin = self.name(), "Reloading advanced metrics plugin");

        // Stop and restart with new config
        self.stop().await?;
        self.load(config).await?;
        self.start().await?;

        info!(plugin = self.name(), "Advanced metrics plugin reloaded");
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        let status = self.status.read().await.clone();
        let collector = self.collector.read().await;

        let details = match status {
            PluginStatus::Started => {
                let stats = collector.get_stats();
                Some(format!(
                    "Tracking {} adapters, {} inference events, {} training events",
                    stats.tracked_adapters, stats.inference_events, stats.training_events
                ))
            }
            PluginStatus::Failed(ref msg) => Some(msg.clone()),
            PluginStatus::Degraded(ref msg) => Some(msg.clone()),
            _ => None,
        };

        Ok(PluginHealth { status, details })
    }

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<()> {
        debug!(
            plugin = self.name(),
            tenant_id, enabled, "Tenant metrics collection toggle"
        );

        // Metrics collection is global - tenant filtering handled at query time
        // This is a no-op for this plugin
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventHookType> {
        vec![
            EventHookType::OnMetricsTick,
            EventHookType::OnInferenceComplete,
            EventHookType::OnTrainingJobEvent,
        ]
    }

    async fn on_event(&self, event: &PluginEvent) -> Result<()> {
        let mut collector = self.collector.write().await;

        match event {
            PluginEvent::MetricsTick(e) => {
                debug!(plugin = self.name(), "Processing metrics tick event");
                collector.record_metrics_tick(e)?;
            }
            PluginEvent::InferenceComplete(e) => {
                debug!(
                    plugin = self.name(),
                    request_id = %e.request_id,
                    latency_ms = e.latency_ms,
                    "Processing inference complete event"
                );
                collector.record_inference_complete(e)?;
            }
            PluginEvent::TrainingJob(e) => {
                debug!(
                    plugin = self.name(),
                    job_id = %e.job_id,
                    status = %e.status,
                    "Processing training job event"
                );
                collector.record_training_job_event(e)?;
            }
            _ => {
                warn!(
                    plugin = self.name(),
                    event_type = event.event_type(),
                    "Received unexpected event type"
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::plugin_events::{InferenceEvent, MetricsTickEvent, TrainingJobEvent};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_plugin_lifecycle() {
        let plugin = AdvancedMetricsPlugin::new();
        let config = PluginConfig {
            name: "advanced-metrics".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        // Load
        assert!(plugin.load(&config).await.is_ok());
        let status = plugin.status.read().await;
        assert!(matches!(*status, PluginStatus::Loaded));
        drop(status);

        // Start
        assert!(plugin.start().await.is_ok());
        let status = plugin.status.read().await;
        assert!(matches!(*status, PluginStatus::Started));
        drop(status);

        // Health check
        let health = plugin.health_check().await.unwrap();
        assert!(matches!(health.status, PluginStatus::Started));

        // Stop
        assert!(plugin.stop().await.is_ok());
        let status = plugin.status.read().await;
        assert!(matches!(*status, PluginStatus::Stopped));
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let plugin = AdvancedMetricsPlugin::new();
        let subscriptions = plugin.subscribed_events();

        assert_eq!(subscriptions.len(), 3);
        assert!(subscriptions.contains(&EventHookType::OnMetricsTick));
        assert!(subscriptions.contains(&EventHookType::OnInferenceComplete));
        assert!(subscriptions.contains(&EventHookType::OnTrainingJobEvent));
    }

    #[tokio::test]
    async fn test_inference_event_handling() {
        let plugin = AdvancedMetricsPlugin::new();
        let config = PluginConfig {
            name: "advanced-metrics".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        plugin.load(&config).await.unwrap();
        plugin.start().await.unwrap();

        let event = PluginEvent::InferenceComplete(InferenceEvent {
            request_id: "req-123".to_string(),
            adapter_ids: vec!["adapter-1".to_string()],
            stack_id: None,
            prompt: Some("Test prompt".to_string()),
            output: Some("Test output".to_string()),
            latency_ms: 123.45,
            tokens_generated: Some(50),
            tokens_per_sec: Some(100.0),
            tenant_id: Some("tenant-1".to_string()),
            model: Some("qwen2.5-7b".to_string()),
            streaming: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        });

        assert!(plugin.on_event(&event).await.is_ok());
    }

    #[tokio::test]
    async fn test_training_event_handling() {
        let plugin = AdvancedMetricsPlugin::new();
        let config = PluginConfig {
            name: "advanced-metrics".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        plugin.load(&config).await.unwrap();
        plugin.start().await.unwrap();

        let event = PluginEvent::TrainingJob(TrainingJobEvent {
            job_id: "job-456".to_string(),
            status: "running".to_string(),
            progress_pct: Some(50.0),
            loss: Some(0.5),
            tokens_per_sec: Some(1000.0),
            dataset_id: Some("dataset-1".to_string()),
            adapter_id: Some("adapter-2".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        });

        assert!(plugin.on_event(&event).await.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_tick_handling() {
        let plugin = AdvancedMetricsPlugin::new();
        let config = PluginConfig {
            name: "advanced-metrics".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        plugin.load(&config).await.unwrap();
        plugin.start().await.unwrap();

        let event = PluginEvent::MetricsTick(MetricsTickEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_percent: Some(25.5),
            memory_bytes: Some(1024 * 1024 * 1024),
            memory_percent: Some(50.0),
            active_adapters: Some(5),
            loaded_adapters: Some(10),
            inference_requests: Some(1000),
            avg_latency_ms: Some(150.0),
            gpu_memory_bytes: Some(2048 * 1024 * 1024),
            gpu_percent: Some(75.0),
            metrics: HashMap::new(),
        });

        assert!(plugin.on_event(&event).await.is_ok());
    }
}
