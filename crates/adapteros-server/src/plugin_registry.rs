//! Plugin registry with health monitoring, watchdog, and isolation
//!
//! Citation: PRD 7 - Operator / Plugin Isolation
//! Implements:
//! - Watchdog with max restart attempts and exponential backoff
//! - Circuit breaker for preventing cascading failures
//! - Timeout protection for plugin operations
//! - Tenant-level degradation telemetry
//! - Graceful degradation (failed plugin doesn't bring down runtime)

use adapteros_core::{
    identity::IdentityEnvelope, AosError, CircuitState, Plugin, PluginConfig, PluginHealth,
    PluginStatus, PluginWatchdog, Result, RestartPolicy,
};
use adapteros_db::tenants::Tenant;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{error, info, warn};

/// Default timeout for plugin operations (30 seconds)
const DEFAULT_PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Default health check interval (30 seconds)
const DEFAULT_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Plugin registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistryConfig {
    /// Timeout for plugin operations
    pub operation_timeout: Duration,

    /// Health check interval
    pub health_check_interval: Duration,

    /// Restart policy
    pub restart_policy: RestartPolicy,

    /// Enable telemetry
    pub enable_telemetry: bool,
}

impl Default for PluginRegistryConfig {
    fn default() -> Self {
        Self {
            operation_timeout: DEFAULT_PLUGIN_TIMEOUT,
            health_check_interval: DEFAULT_HEALTH_CHECK_INTERVAL,
            restart_policy: RestartPolicy::default(),
            enable_telemetry: true,
        }
    }
}

/// Plugin registry with health monitoring and isolation
#[derive(Debug, Clone)]
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn Plugin + Send + Sync>>>>,
    tasks: Arc<RwLock<HashMap<String, JoinHandle<Result<()>>>>>,
    db: adapteros_db::Db,
    watchdog: Arc<PluginWatchdog>,
    config: Arc<PluginRegistryConfig>,
    identity: Arc<IdentityEnvelope>,
}

impl PluginRegistry {
    /// Create new plugin registry with default config
    pub fn new(db: adapteros_db::Db, identity: IdentityEnvelope) -> Self {
        Self::with_config(db, identity, PluginRegistryConfig::default())
    }

    /// Create new plugin registry with custom config
    pub fn with_config(
        db: adapteros_db::Db,
        identity: IdentityEnvelope,
        config: PluginRegistryConfig,
    ) -> Self {
        let watchdog = PluginWatchdog::new(config.restart_policy.clone());

        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            db,
            watchdog: Arc::new(watchdog),
            config: Arc::new(config),
            identity: Arc::new(identity),
        }
    }

    /// Emit telemetry event
    async fn emit_event(
        &self,
        event_type: EventType,
        level: LogLevel,
        message: String,
        metadata: serde_json::Value,
    ) {
        if !self.config.enable_telemetry {
            return;
        }

        let event = TelemetryEventBuilder::new(
            event_type,
            level,
            message,
            (*self.identity).clone(),
        )
        .component("adapteros-plugin-registry".to_string())
        .metadata(metadata)
        .build();

        // Fire and forget - don't block on telemetry
        tokio::spawn(async move {
            // In production, this would emit to telemetry writer
            // For now, just log
            info!(
                event_type = event.event_type,
                message = event.message,
                "Plugin telemetry event"
            );
        });
    }

    /// Register plugin with timeout protection
    pub async fn register<P>(&self, name: String, plugin: P, config: PluginConfig) -> Result<()>
    where
        P: Plugin + 'static + Send + Sync,
    {
        let arc_plugin: Arc<dyn Plugin + Send + Sync> = Arc::new(plugin);
        let mut plugins = self.plugins.write().await;
        plugins.insert(name.clone(), arc_plugin.clone());

        self.emit_event(
            EventType::PluginLoaded,
            LogLevel::Info,
            format!("Plugin {} loaded", name),
            json!({"plugin_name": name}),
        )
        .await;

        // Load the plugin with timeout protection
        match timeout(
            self.config.operation_timeout,
            arc_plugin.load(&config),
        )
        .await
        {
            Ok(Ok(_)) => {
                info!(plugin_name = %name, "Plugin load completed successfully");
            }
            Ok(Err(e)) => {
                error!(plugin_name = %name, error = %e, "Plugin load failed");
                self.emit_event(
                    EventType::PluginHealthFailed,
                    LogLevel::Error,
                    format!("Plugin {} load failed: {}", name, e),
                    json!({"plugin_name": name, "error": e.to_string()}),
                )
                .await;
                return Err(e);
            }
            Err(_) => {
                error!(plugin_name = %name, timeout = ?self.config.operation_timeout, "Plugin load timed out");
                self.emit_event(
                    EventType::PluginTimeout,
                    LogLevel::Error,
                    format!("Plugin {} load timed out", name),
                    json!({"plugin_name": name, "timeout_secs": self.config.operation_timeout.as_secs()}),
                )
                .await;
                return Err(AosError::Timeout {
                    duration: self.config.operation_timeout,
                });
            }
        }

        // Start and spawn supervisor task
        self.start_plugin(&name, arc_plugin, config).await?;

        Ok(())
    }

    /// Start plugin with supervisor task
    async fn start_plugin(
        &self,
        name: &str,
        plugin: Arc<dyn Plugin + Send + Sync>,
        config: PluginConfig,
    ) -> Result<()> {
        // Start the plugin with timeout protection
        match timeout(
            self.config.operation_timeout,
            plugin.start(),
        )
        .await
        {
            Ok(Ok(_)) => {
                info!(plugin_name = %name, "Plugin started successfully");
                self.emit_event(
                    EventType::PluginStarted,
                    LogLevel::Info,
                    format!("Plugin {} started", name),
                    json!({"plugin_name": name}),
                )
                .await;
            }
            Ok(Err(e)) => {
                error!(plugin_name = %name, error = %e, "Plugin start failed");
                self.emit_event(
                    EventType::PluginHealthFailed,
                    LogLevel::Error,
                    format!("Plugin {} start failed: {}", name, e),
                    json!({"plugin_name": name, "error": e.to_string()}),
                )
                .await;
                return Err(e);
            }
            Err(_) => {
                error!(plugin_name = %name, timeout = ?self.config.operation_timeout, "Plugin start timed out");
                self.emit_event(
                    EventType::PluginTimeout,
                    LogLevel::Error,
                    format!("Plugin {} start timed out", name),
                    json!({"plugin_name": name, "timeout_secs": self.config.operation_timeout.as_secs()}),
                )
                .await;
                return Err(AosError::Timeout {
                    duration: self.config.operation_timeout,
                });
            }
        }

        // Spawn supervisor task with health monitoring
        let plugin_clone = plugin.clone();
        let name_clone = name.to_string();
        let config_clone = config.clone();
        let watchdog = self.watchdog.clone();
        let interval = self.config.health_check_interval;
        let operation_timeout = self.config.operation_timeout;
        let identity = self.identity.clone();
        let enable_telemetry = self.config.enable_telemetry;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;

                // Emit health check start event
                if enable_telemetry {
                    let _ = Self::emit_static_event(
                        EventType::PluginHealthCheckStart,
                        LogLevel::Debug,
                        format!("Starting health check for plugin {}", name_clone),
                        json!({"plugin_name": name_clone}),
                        &identity,
                    )
                    .await;
                }

                // Run health check with timeout
                let health_result = timeout(
                    operation_timeout,
                    plugin_clone.health_check(),
                )
                .await;

                match health_result {
                    Ok(Ok(health)) => {
                        // Emit health check complete event
                        if enable_telemetry {
                            let _ = Self::emit_static_event(
                                EventType::PluginHealthCheckComplete,
                                LogLevel::Debug,
                                format!("Health check completed for plugin {}", name_clone),
                                json!({
                                    "plugin_name": name_clone,
                                    "status": format!("{:?}", health.status)
                                }),
                                &identity,
                            )
                            .await;
                        }

                        match health.status {
                            PluginStatus::Failed(ref reason) => {
                                warn!(
                                    plugin_name = %name_clone,
                                    reason = %reason,
                                    "Plugin health check failed"
                                );

                                // Emit health failed event
                                if enable_telemetry {
                                    let _ = Self::emit_static_event(
                                        EventType::PluginHealthFailed,
                                        LogLevel::Warn,
                                        format!("Plugin {} health check failed: {}", name_clone, reason),
                                        json!({
                                            "plugin_name": name_clone,
                                            "reason": reason
                                        }),
                                        &identity,
                                    )
                                    .await;
                                }

                                // Attempt restart via watchdog
                                if let Err(e) = Self::attempt_restart(
                                    &watchdog,
                                    &plugin_clone,
                                    &config_clone,
                                    &name_clone,
                                    &identity,
                                    enable_telemetry,
                                )
                                .await
                                {
                                    error!(
                                        plugin_name = %name_clone,
                                        error = %e,
                                        "Failed to restart plugin"
                                    );
                                }
                            }
                            PluginStatus::Degraded(ref reason) => {
                                warn!(
                                    plugin_name = %name_clone,
                                    reason = %reason,
                                    "Plugin degraded but functional"
                                );

                                // Emit degraded event
                                if enable_telemetry {
                                    let _ = Self::emit_static_event(
                                        EventType::PluginDegraded,
                                        LogLevel::Warn,
                                        format!("Plugin {} degraded: {}", name_clone, reason),
                                        json!({
                                            "plugin_name": name_clone,
                                            "reason": reason
                                        }),
                                        &identity,
                                    )
                                    .await;
                                }
                            }
                            PluginStatus::Started => {
                                // Check if plugin was previously degraded/failed and recovered
                                if let Some((attempts, circuit_state)) = watchdog.get_state(&name_clone).await {
                                    if attempts > 0 && circuit_state == CircuitState::Closed {
                                        info!(plugin_name = %name_clone, "Plugin recovered");

                                        // Emit recovered event
                                        if enable_telemetry {
                                            let _ = Self::emit_static_event(
                                                EventType::PluginRecovered,
                                                LogLevel::Info,
                                                format!("Plugin {} recovered after {} restart attempts", name_clone, attempts),
                                                json!({
                                                    "plugin_name": name_clone,
                                                    "restart_attempts": attempts
                                                }),
                                                &identity,
                                            )
                                            .await;
                                        }

                                        // Reset watchdog on successful recovery
                                        let _ = watchdog.record_success(&name_clone).await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(Err(e)) => {
                        error!(
                            plugin_name = %name_clone,
                            error = %e,
                            "Health check failed with error"
                        );

                        if enable_telemetry {
                            let _ = Self::emit_static_event(
                                EventType::PluginHealthFailed,
                                LogLevel::Error,
                                format!("Plugin {} health check error: {}", name_clone, e),
                                json!({
                                    "plugin_name": name_clone,
                                    "error": e.to_string()
                                }),
                                &identity,
                            )
                            .await;
                        }
                    }
                    Err(_) => {
                        warn!(
                            plugin_name = %name_clone,
                            timeout_secs = operation_timeout.as_secs(),
                            "Health check timed out"
                        );

                        if enable_telemetry {
                            let _ = Self::emit_static_event(
                                EventType::PluginTimeout,
                                LogLevel::Warn,
                                format!("Plugin {} health check timed out", name_clone),
                                json!({
                                    "plugin_name": name_clone,
                                    "timeout_secs": operation_timeout.as_secs()
                                }),
                                &identity,
                            )
                            .await;
                        }
                    }
                }
            }
        });

        let mut tasks = self.tasks.write().await;
        tasks.insert(name.to_string(), handle);

        Ok(())
    }

    /// Static helper for emitting events from spawned tasks
    async fn emit_static_event(
        event_type: EventType,
        level: LogLevel,
        message: String,
        metadata: serde_json::Value,
        identity: &IdentityEnvelope,
    ) -> Result<()> {
        let event = TelemetryEventBuilder::new(
            event_type,
            level,
            message,
            identity.clone(),
        )
        .component("adapteros-plugin-registry".to_string())
        .metadata(metadata)
        .build();

        // Log event
        info!(
            event_type = event.event_type,
            message = event.message,
            "Plugin telemetry event"
        );

        Ok(())
    }

    /// Attempt to restart plugin via watchdog
    async fn attempt_restart(
        watchdog: &PluginWatchdog,
        plugin: &Arc<dyn Plugin + Send + Sync>,
        config: &PluginConfig,
        name: &str,
        identity: &IdentityEnvelope,
        enable_telemetry: bool,
    ) -> Result<()> {
        // Check if restart is allowed
        if !watchdog.can_restart(name).await? {
            warn!(plugin_name = %name, "Plugin restart not allowed by watchdog");

            // Check if circuit breaker is open or max attempts reached
            if let Some((attempts, circuit_state)) = watchdog.get_state(name).await {
                if circuit_state == CircuitState::Open {
                    if enable_telemetry {
                        let _ = Self::emit_static_event(
                            EventType::PluginCircuitBreakerOpened,
                            LogLevel::Error,
                            format!("Plugin {} circuit breaker opened after {} failures", name, attempts),
                            json!({
                                "plugin_name": name,
                                "failure_count": attempts
                            }),
                            identity,
                        )
                        .await;
                    }
                } else {
                    if enable_telemetry {
                        let _ = Self::emit_static_event(
                            EventType::PluginRestartLimitReached,
                            LogLevel::Error,
                            format!("Plugin {} reached max restart attempts ({})", name, attempts),
                            json!({
                                "plugin_name": name,
                                "max_attempts": attempts
                            }),
                            identity,
                        )
                        .await;
                    }
                }
            }

            return Err(AosError::PolicyViolation(format!(
                "Plugin {} restart blocked by watchdog policy",
                name
            )));
        }

        info!(plugin_name = %name, "Attempting plugin restart");

        // Emit restart attempt event
        if enable_telemetry {
            if let Some((attempts, _)) = watchdog.get_state(name).await {
                let _ = Self::emit_static_event(
                    EventType::PluginRestartAttempt,
                    LogLevel::Info,
                    format!("Attempting restart of plugin {} (attempt {})", name, attempts + 1),
                    json!({
                        "plugin_name": name,
                        "attempt": attempts + 1
                    }),
                    identity,
                )
                .await;
            }
        }

        // Record restart attempt
        watchdog.record_restart(name).await?;

        // Attempt reload
        match plugin.reload(config).await {
            Ok(_) => {
                info!(plugin_name = %name, "Plugin restart successful");

                if enable_telemetry {
                    let _ = Self::emit_static_event(
                        EventType::PluginRestartSuccess,
                        LogLevel::Info,
                        format!("Plugin {} restart successful", name),
                        json!({"plugin_name": name}),
                        identity,
                    )
                    .await;
                }

                // Note: We don't call record_success here - wait for health check to confirm
                Ok(())
            }
            Err(e) => {
                error!(plugin_name = %name, error = %e, "Plugin restart failed");

                if enable_telemetry {
                    let _ = Self::emit_static_event(
                        EventType::PluginRestartFailed,
                        LogLevel::Error,
                        format!("Plugin {} restart failed: {}", name, e),
                        json!({
                            "plugin_name": name,
                            "error": e.to_string()
                        }),
                        identity,
                    )
                    .await;
                }

                watchdog.record_failure(name).await?;
                Err(e)
            }
        }
    }

    /// Stop plugin with timeout protection
    pub async fn stop_plugin(&self, name: &str) -> Result<()> {
        let plugins = self.plugins.read().await;
        if let Some(plugin) = plugins.get(name) {
            match timeout(
                self.config.operation_timeout,
                plugin.stop(),
            )
            .await
            {
                Ok(Ok(_)) => {
                    info!(plugin_name = %name, "Plugin stopped successfully");
                    self.emit_event(
                        EventType::PluginStopped,
                        LogLevel::Info,
                        format!("Plugin {} stopped", name),
                        json!({"plugin_name": name}),
                    )
                    .await;
                }
                Ok(Err(e)) => {
                    warn!(plugin_name = %name, error = %e, "Plugin stop failed");
                }
                Err(_) => {
                    warn!(plugin_name = %name, "Plugin stop timed out");
                    self.emit_event(
                        EventType::PluginTimeout,
                        LogLevel::Warn,
                        format!("Plugin {} stop timed out", name),
                        json!({
                            "plugin_name": name,
                            "timeout_secs": self.config.operation_timeout.as_secs()
                        }),
                    )
                    .await;
                }
            }
        }

        let mut tasks = self.tasks.write().await;
        if let Some(handle) = tasks.remove(name) {
            handle.abort();
        }

        Ok(())
    }

    /// Enable/disable plugin for specific tenant
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
                self.config.operation_timeout,
                plugin.set_tenant_enabled(tenant_id, enabled),
            )
            .await
            {
                Ok(Ok(_)) => {
                    info!(
                        plugin_name = %plugin_name,
                        tenant_id = %tenant_id,
                        enabled = enabled,
                        "Plugin tenant setting updated"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        plugin_name = %plugin_name,
                        tenant_id = %tenant_id,
                        error = %e,
                        "Failed to update plugin tenant setting"
                    );
                }
                Err(_) => {
                    warn!(
                        plugin_name = %plugin_name,
                        tenant_id = %tenant_id,
                        "Plugin tenant setting update timed out"
                    );
                }
            }
        }
        Ok(())
    }

    /// Check if plugin is enabled for tenant
    pub async fn is_enabled_for_tenant(&self, plugin_name: &str, tenant_id: &str) -> Result<bool> {
        match self.db.get_plugin_enable(tenant_id, plugin_name).await? {
            Some(e) => Ok(e),
            None => Ok(true), // default enabled
        }
    }

    /// Get health status for all plugins across all tenants
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

    /// Quiesce plugin (prevent further restarts) - admin operation
    pub async fn quiesce_plugin(&self, name: &str) -> Result<()> {
        info!(plugin_name = %name, "Quiescing plugin");

        self.watchdog.quiesce(name).await?;

        self.emit_event(
            EventType::PluginQuiesced,
            LogLevel::Warn,
            format!("Plugin {} quiesced, no further restarts allowed", name),
            json!({"plugin_name": name}),
        )
        .await;

        Ok(())
    }

    /// Reset plugin restart state - admin operation
    pub async fn reset_plugin(&self, name: &str) -> Result<()> {
        info!(plugin_name = %name, "Resetting plugin restart state");

        self.watchdog.reset(name).await?;

        self.emit_event(
            EventType::PluginRecovered,
            LogLevel::Info,
            format!("Plugin {} restart state manually reset", name),
            json!({"plugin_name": name}),
        )
        .await;

        Ok(())
    }

    /// Get watchdog state for plugin (for monitoring/debugging)
    pub async fn get_watchdog_state(&self, name: &str) -> Option<(u32, CircuitState)> {
        self.watchdog.get_state(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::identity::IdentityEnvelope;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    // Mock plugin for testing
    struct MockPlugin {
        name: &'static str,
        fail_health_check: Arc<AtomicBool>,
        health_check_count: Arc<AtomicUsize>,
        reload_count: Arc<AtomicUsize>,
        load_delay: Option<Duration>,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn load(&self, _config: &PluginConfig) -> Result<()> {
            if let Some(delay) = self.load_delay {
                tokio::time::sleep(delay).await;
            }
            Ok(())
        }

        async fn start(&self) -> Result<()> {
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn reload(&self, _config: &PluginConfig) -> Result<()> {
            self.reload_count.fetch_add(1, Ordering::SeqCst);
            self.fail_health_check.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn health_check(&self) -> Result<PluginHealth> {
            self.health_check_count.fetch_add(1, Ordering::SeqCst);

            if self.fail_health_check.load(Ordering::SeqCst) {
                Ok(PluginHealth {
                    status: PluginStatus::Failed("Mock failure".to_string()),
                    details: Some("Test failure".to_string()),
                })
            } else {
                Ok(PluginHealth {
                    status: PluginStatus::Started,
                    details: None,
                })
            }
        }

        async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_plugin_timeout_protection() {
        let db = adapteros_db::Db::open_in_memory().await.unwrap();
        let identity = IdentityEnvelope::anonymous();

        let config = PluginRegistryConfig {
            operation_timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let registry = PluginRegistry::with_config(db, identity, config);

        let plugin = MockPlugin {
            name: "timeout_test",
            fail_health_check: Arc::new(AtomicBool::new(false)),
            health_check_count: Arc::new(AtomicUsize::new(0)),
            reload_count: Arc::new(AtomicUsize::new(0)),
            load_delay: Some(Duration::from_secs(1)), // Exceeds timeout
        };

        let plugin_config = PluginConfig {
            name: "timeout_test".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        let result = registry.register("timeout_test".to_string(), plugin, plugin_config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_plugin_restart_with_watchdog() {
        let db = adapteros_db::Db::open_in_memory().await.unwrap();
        let identity = IdentityEnvelope::anonymous();

        let config = PluginRegistryConfig {
            health_check_interval: Duration::from_millis(100),
            restart_policy: RestartPolicy {
                max_attempts: 2,
                initial_backoff: Duration::from_millis(10),
                ..Default::default()
            },
            ..Default::default()
        };

        let registry = PluginRegistry::with_config(db, identity, config);

        let fail_health = Arc::new(AtomicBool::new(false));
        let reload_count = Arc::new(AtomicUsize::new(0));

        let plugin = MockPlugin {
            name: "restart_test",
            fail_health_check: fail_health.clone(),
            health_check_count: Arc::new(AtomicUsize::new(0)),
            reload_count: reload_count.clone(),
            load_delay: None,
        };

        let plugin_config = PluginConfig {
            name: "restart_test".to_string(),
            enabled: true,
            specific: HashMap::new(),
        };

        registry.register("restart_test".to_string(), plugin, plugin_config).await.unwrap();

        // Trigger failure
        fail_health.store(true, Ordering::SeqCst);

        // Wait for health check and restart
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should have attempted restart at least once
        assert!(reload_count.load(Ordering::SeqCst) > 0);
    }
}
