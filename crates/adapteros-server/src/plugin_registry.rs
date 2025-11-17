use adapteros_core::{
    identity::IdentityEnvelope, AosError, Plugin, PluginConfig, PluginHealth, PluginStatus, Result,
};
use adapteros_db::tenants::Tenant;
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{error, info, warn};

/// Maximum consecutive failures before auto-disabling plugin
/// TODO(production): Implement circuit breaker with exponential backoff instead of simple auto-disable
/// See: docs/PLUGIN_ISOLATION_PRODUCTION_CHECKLIST.md#5-circuit-breaker-pattern
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Timeout for plugin operations
/// TODO(production): Make configurable via config file
/// See: docs/PLUGIN_ISOLATION_PRODUCTION_CHECKLIST.md#9-configurable-timeouts
const PLUGIN_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Plugin metadata for tracking health and failures
/// TODO(production): Add Prometheus metrics for plugin_failures_total, plugin_health_check_duration, etc.
/// See: docs/PLUGIN_ISOLATION_PRODUCTION_CHECKLIST.md#4-prometheus-metrics
#[derive(Debug, Clone)]
struct PluginMetadata {
    consecutive_failures: u32,
    last_error: Option<String>,
    last_healthy_at: Option<i64>,
    // TODO(production): Add circuit breaker fields
    // circuit_state: CircuitState,
    // last_failure_at: Option<i64>,
    // backoff_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn Plugin + Send + Sync>>>>,
    tasks: Arc<RwLock<HashMap<String, JoinHandle<Result<()>>>>>,
    metadata: Arc<RwLock<HashMap<String, PluginMetadata>>>,
    telemetry: Option<Arc<adapteros_telemetry::TelemetryWriter>>,
    db: adapteros_db::Db,
}

impl PluginRegistry {
    pub fn new(db: adapteros_db::Db) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            telemetry: None,
            db,
        }
    }

    /// Create a new plugin registry with telemetry support
    pub fn with_telemetry(db: adapteros_db::Db, telemetry: Arc<adapteros_telemetry::TelemetryWriter>) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            telemetry: Some(telemetry),
            db,
        }
    }

    /// Emit plugin telemetry event
    async fn emit_telemetry(
        &self,
        event_type: EventType,
        plugin_name: &str,
        level: LogLevel,
        message: String,
        metadata: Option<serde_json::Value>,
    ) {
        let identity = IdentityEnvelope::new(
            "system".to_string(),
            "plugin".to_string(),
            plugin_name.to_string(),
            IdentityEnvelope::default_revision(),
        );

        let event = TelemetryEventBuilder::new(event_type, level, message)
            .identity(identity)
            .component("plugin-registry".to_string())
            .metadata(metadata.unwrap_or(serde_json::json!({})))
            .build();

        // Log to tracing system
        match level {
            LogLevel::Debug => tracing::debug!(event = ?event, "Plugin telemetry"),
            LogLevel::Info => tracing::info!(event = ?event, "Plugin telemetry"),
            LogLevel::Warn => tracing::warn!(event = ?event, "Plugin telemetry"),
            LogLevel::Error => tracing::error!(event = ?event, "Plugin telemetry"),
            LogLevel::Critical => tracing::error!(event = ?event, "Plugin telemetry (CRITICAL)"),
        }

        // Write to telemetry writer for durable storage
        if let Some(ref telemetry) = self.telemetry {
            if let Err(e) = telemetry.log_event(event) {
                tracing::warn!(error = %e, "Failed to persist plugin telemetry event");
            }
        }
    }

    /// Record plugin failure
    async fn record_failure(&self, plugin_name: &str, error: &str) {
        let mut metadata = self.metadata.write().await;
        let entry = metadata
            .entry(plugin_name.to_string())
            .or_insert(PluginMetadata {
                consecutive_failures: 0,
                last_error: None,
                last_healthy_at: None,
            });

        entry.consecutive_failures += 1;
        entry.last_error = Some(error.to_string());

        info!(
            plugin = plugin_name,
            failures = entry.consecutive_failures,
            error = error,
            "Plugin failure recorded"
        );
    }

    /// Record successful plugin operation
    async fn record_success(&self, plugin_name: &str) {
        let mut metadata = self.metadata.write().await;
        let entry = metadata
            .entry(plugin_name.to_string())
            .or_insert(PluginMetadata {
                consecutive_failures: 0,
                last_error: None,
                last_healthy_at: None,
            });

        entry.consecutive_failures = 0;
        entry.last_error = None;
        entry.last_healthy_at = Some(chrono::Utc::now().timestamp());
    }

    /// Check if plugin should be auto-disabled due to repeated failures
    async fn should_auto_disable(&self, plugin_name: &str) -> bool {
        let metadata = self.metadata.read().await;
        if let Some(entry) = metadata.get(plugin_name) {
            entry.consecutive_failures >= MAX_CONSECUTIVE_FAILURES
        } else {
            false
        }
    }

    pub async fn register<P>(&self, name: String, plugin: P, config: PluginConfig) -> Result<()>
    where
        P: Plugin + 'static + Send + Sync,
    {
        let arc_plugin: Arc<dyn Plugin + Send + Sync> = Arc::new(plugin);
        let mut plugins = self.plugins.write().await;
        plugins.insert(name.clone(), arc_plugin.clone());

        // Initialize metadata
        let mut metadata = self.metadata.write().await;
        metadata.insert(
            name.clone(),
            PluginMetadata {
                consecutive_failures: 0,
                last_error: None,
                last_healthy_at: Some(chrono::Utc::now().timestamp()),
            },
        );
        drop(metadata);

        // Load the plugin with timeout
        match timeout(PLUGIN_OPERATION_TIMEOUT, arc_plugin.load(&config)).await {
            Ok(Ok(_)) => {
                self.record_success(&name).await;
                self.emit_telemetry(
                    EventType::PluginStarted,
                    &name,
                    LogLevel::Info,
                    format!("Plugin '{}' loaded successfully", name),
                    Some(serde_json::json!({"plugin": name})),
                )
                .await;
            }
            Ok(Err(e)) => {
                let error_msg = format!("Plugin load failed: {}", e);
                self.record_failure(&name, &error_msg).await;
                self.emit_telemetry(
                    EventType::PluginDegraded,
                    &name,
                    LogLevel::Error,
                    error_msg.clone(),
                    Some(serde_json::json!({"plugin": name, "error": e.to_string()})),
                )
                .await;
                return Err(e);
            }
            Err(_) => {
                let error_msg = "Plugin load timeout";
                self.record_failure(&name, error_msg).await;
                self.emit_telemetry(
                    EventType::PluginTimeout,
                    &name,
                    LogLevel::Error,
                    format!("Plugin '{}' load timeout after {:?}", name, PLUGIN_OPERATION_TIMEOUT),
                    Some(serde_json::json!({"plugin": name, "timeout_secs": PLUGIN_OPERATION_TIMEOUT.as_secs()})),
                )
                .await;
                return Err(AosError::Timeout(format!(
                    "Plugin '{}' load timeout",
                    name
                )));
            }
        }

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
        // Start plugin with timeout
        match timeout(PLUGIN_OPERATION_TIMEOUT, plugin.start()).await {
            Ok(Ok(_)) => {
                self.emit_telemetry(
                    EventType::PluginStarted,
                    name,
                    LogLevel::Info,
                    format!("Plugin '{}' started successfully", name),
                    Some(serde_json::json!({"plugin": name})),
                )
                .await;
            }
            Ok(Err(e)) => {
                let error_msg = format!("Plugin start failed: {}", e);
                self.record_failure(name, &error_msg).await;
                self.emit_telemetry(
                    EventType::PluginDegraded,
                    name,
                    LogLevel::Error,
                    error_msg.clone(),
                    Some(serde_json::json!({"plugin": name, "error": e.to_string()})),
                )
                .await;
                return Err(e);
            }
            Err(_) => {
                let error_msg = "Plugin start timeout";
                self.record_failure(name, error_msg).await;
                self.emit_telemetry(
                    EventType::PluginTimeout,
                    name,
                    LogLevel::Error,
                    format!("Plugin '{}' start timeout", name),
                    Some(serde_json::json!({"plugin": name})),
                )
                .await;
                return Err(AosError::Timeout(format!("Plugin '{}' start timeout", name)));
            }
        }

        // Spawn supervisor task with panic recovery
        let plugin_clone = plugin.clone();
        let name_clone = name.to_string();
        let config_clone = config.clone();
        let metadata_clone = self.metadata.clone();
        let db_clone = self.db.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;

                // Catch panics in health check
                let health_result = tokio::task::spawn({
                    let plugin = plugin_clone.clone();
                    async move { timeout(PLUGIN_OPERATION_TIMEOUT, plugin.health_check()).await }
                })
                .await;

                match health_result {
                    Ok(Ok(Ok(health))) => {
                        // Health check succeeded
                        let mut metadata = metadata_clone.write().await;
                        if let Some(entry) = metadata.get_mut(&name_clone) {
                            entry.consecutive_failures = 0;
                            entry.last_healthy_at = Some(chrono::Utc::now().timestamp());
                        }
                        drop(metadata);

                        match health.status {
                            PluginStatus::Dead(ref reason) => {
                                warn!("Plugin {} is dead: {}, attempting restart", name_clone, reason);

                                // Attempt reload with timeout
                                match timeout(
                                    PLUGIN_OPERATION_TIMEOUT,
                                    plugin_clone.reload(&config_clone),
                                )
                                .await
                                {
                                    Ok(Ok(_)) => {
                                        info!("Plugin {} restarted successfully", name_clone);
                                        let mut metadata = metadata_clone.write().await;
                                        if let Some(entry) = metadata.get_mut(&name_clone) {
                                            entry.consecutive_failures = 0;
                                            entry.last_error = None;
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        error!("Failed to restart plugin {}: {}", name_clone, e);
                                        let mut metadata = metadata_clone.write().await;
                                        if let Some(entry) = metadata.get_mut(&name_clone) {
                                            entry.consecutive_failures += 1;
                                            entry.last_error = Some(e.to_string());
                                        }
                                    }
                                    Err(_) => {
                                        error!("Plugin {} restart timeout", name_clone);
                                        let mut metadata = metadata_clone.write().await;
                                        if let Some(entry) = metadata.get_mut(&name_clone) {
                                            entry.consecutive_failures += 1;
                                            entry.last_error = Some("Restart timeout".to_string());
                                        }
                                    }
                                }

                                // Check if should auto-disable
                                let should_disable = {
                                    let metadata = metadata_clone.read().await;
                                    metadata
                                        .get(&name_clone)
                                        .map(|e| e.consecutive_failures >= MAX_CONSECUTIVE_FAILURES)
                                        .unwrap_or(false)
                                };

                                if should_disable {
                                    warn!(
                                        "Plugin {} exceeded max failures, auto-disabling",
                                        name_clone
                                    );
                                    // Disable for all tenants
                                    if let Ok(tenants) = db_clone.list_tenants().await {
                                        for tenant in tenants {
                                            let _ = db_clone
                                                .set_plugin_enable(&tenant.id, &name_clone, false)
                                                .await;
                                        }
                                    }
                                }
                            }
                            PluginStatus::Degraded(ref reason) => {
                                warn!("Plugin {} degraded: {}", name_clone, reason);
                            }
                            _ => {}
                        }
                    }
                    Ok(Ok(Err(_))) => {
                        // Health check timeout
                        error!("Health check timeout for plugin {}", name_clone);
                        let mut metadata = metadata_clone.write().await;
                        if let Some(entry) = metadata.get_mut(&name_clone) {
                            entry.consecutive_failures += 1;
                            entry.last_error = Some("Health check timeout".to_string());
                        }
                    }
                    Ok(Err(e)) => {
                        // Health check failed
                        error!("Health check failed for plugin {}: {}", name_clone, e);
                        let mut metadata = metadata_clone.write().await;
                        if let Some(entry) = metadata.get_mut(&name_clone) {
                            entry.consecutive_failures += 1;
                            entry.last_error = Some(e.to_string());
                        }
                    }
                    Err(e) => {
                        // Panic in health check task
                        error!("Plugin {} health check panicked: {}", name_clone, e);
                        let mut metadata = metadata_clone.write().await;
                        if let Some(entry) = metadata.get_mut(&name_clone) {
                            entry.consecutive_failures += 1;
                            entry.last_error = Some(format!("Panic: {}", e));
                        }
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
            match timeout(PLUGIN_OPERATION_TIMEOUT, plugin.stop()).await {
                Ok(Ok(_)) => {
                    self.emit_telemetry(
                        EventType::PluginStopped,
                        name,
                        LogLevel::Info,
                        format!("Plugin '{}' stopped successfully", name),
                        Some(serde_json::json!({"plugin": name})),
                    )
                    .await;
                }
                Ok(Err(e)) => {
                    warn!("Plugin {} stop failed: {}", name, e);
                }
                Err(_) => {
                    warn!("Plugin {} stop timeout", name);
                }
            }
        }

        // TODO(production): Implement graceful shutdown instead of abort()
        // Should wait 5s for task to exit naturally before aborting
        // See: docs/PLUGIN_ISOLATION_PRODUCTION_CHECKLIST.md#6-graceful-shutdown
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
            match timeout(
                PLUGIN_OPERATION_TIMEOUT,
                plugin.set_tenant_enabled(tenant_id, enabled),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let event_type = if enabled {
                        EventType::PluginEnabled
                    } else {
                        EventType::PluginDisabled
                    };
                    self.emit_telemetry(
                        event_type,
                        plugin_name,
                        LogLevel::Info,
                        format!(
                            "Plugin '{}' {} for tenant '{}'",
                            plugin_name,
                            if enabled { "enabled" } else { "disabled" },
                            tenant_id
                        ),
                        Some(serde_json::json!({
                            "plugin": plugin_name,
                            "tenant": tenant_id,
                            "enabled": enabled
                        })),
                    )
                    .await;
                }
                Ok(Err(e)) => {
                    warn!(
                        "Failed to set tenant enabled for plugin {}: {}",
                        plugin_name, e
                    );
                }
                Err(_) => {
                    warn!("Timeout setting tenant enabled for plugin {}", plugin_name);
                }
            }
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
        let metadata = self.metadata.read().await;

        for (name, _plugin) in plugins.iter() {
            let mut tenant_healths = HashMap::new();
            let plugin_metadata = metadata.get(name);

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
                    last_error: plugin_metadata.and_then(|m| m.last_error.clone()),
                    last_healthy_at: plugin_metadata.and_then(|m| m.last_healthy_at),
                };
                tenant_healths.insert(tenant.clone(), health);
            }
            overall.insert(name.clone(), tenant_healths);
        }
        overall
    }
}
