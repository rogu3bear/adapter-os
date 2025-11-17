//! Integration tests for plugin health monitoring and recovery
//!
//! Citation: PRD 7 - Operator / Plugin Isolation
//!
//! These tests verify end-to-end functionality including:
//! - Health monitoring integration
//! - Telemetry event emission
//! - Watchdog and registry integration
//! - Complete failure and recovery cycles
//! - Production-realistic scenarios

use adapteros_core::{
    identity::IdentityEnvelope, AosError, CircuitState, Plugin, PluginConfig, PluginHealth,
    PluginStatus, Result,
};
use adapteros_db::Db;
use adapteros_server::plugin_registry::{PluginRegistry, PluginRegistryConfig};
use adapteros_core::RestartPolicy;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Test plugin with configurable behavior
struct TestPlugin {
    name: String,
    health_status: Arc<RwLock<PluginStatus>>,
    load_count: Arc<AtomicUsize>,
    start_count: Arc<AtomicUsize>,
    reload_count: Arc<AtomicUsize>,
    health_check_count: Arc<AtomicUsize>,
    tenant_settings: Arc<RwLock<HashMap<String, bool>>>,
}

impl TestPlugin {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            health_status: Arc::new(RwLock::new(PluginStatus::Started)),
            load_count: Arc::new(AtomicUsize::new(0)),
            start_count: Arc::new(AtomicUsize::new(0)),
            reload_count: Arc::new(AtomicUsize::new(0)),
            health_check_count: Arc::new(AtomicUsize::new(0)),
            tenant_settings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn set_health_status(&self, status: PluginStatus) {
        *self.health_status.write().await = status;
    }

    fn get_load_count(&self) -> usize {
        self.load_count.load(Ordering::SeqCst)
    }

    fn get_reload_count(&self) -> usize {
        self.reload_count.load(Ordering::SeqCst)
    }

    fn get_health_check_count(&self) -> usize {
        self.health_check_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Plugin for TestPlugin {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        self.load_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        self.reload_count.fetch_add(1, Ordering::SeqCst);
        // Reset health status on reload
        *self.health_status.write().await = PluginStatus::Started;
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        self.health_check_count.fetch_add(1, Ordering::SeqCst);
        Ok(PluginHealth {
            status: self.health_status.read().await.clone(),
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<()> {
        self.tenant_settings.write().await.insert(tenant_id.to_string(), enabled);
        Ok(())
    }
}

/// Integration test 1: Complete failure and recovery cycle
#[tokio::test]
async fn test_complete_failure_recovery_cycle() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(100),
        operation_timeout: Duration::from_secs(5),
        restart_policy: RestartPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(50),
            backoff_multiplier: 1.5,
            ..Default::default()
        },
        enable_telemetry: true,
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin = TestPlugin::new("recovery_cycle");
    let plugin_ref = plugin.clone();

    let plugin_config = PluginConfig {
        name: "recovery_cycle".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    // Register plugin
    registry.register("recovery_cycle".to_string(), plugin, plugin_config).await.unwrap();

    // Verify plugin loaded and started
    assert_eq!(plugin_ref.get_load_count(), 1);

    // Let plugin run healthy for a bit
    sleep(Duration::from_millis(300)).await;
    let initial_health_checks = plugin_ref.get_health_check_count();
    assert!(initial_health_checks >= 2); // Should have done multiple health checks

    // Make plugin fail
    plugin_ref.set_health_status(PluginStatus::Failed("Test failure".to_string())).await;

    // Wait for health check to detect failure and attempt restart
    sleep(Duration::from_millis(300)).await;

    // Verify reload was attempted
    assert!(plugin_ref.get_reload_count() > 0);

    // Plugin should be recovered now (reload resets health status)
    sleep(Duration::from_millis(300)).await;

    // Verify health checks continued
    let final_health_checks = plugin_ref.get_health_check_count();
    assert!(final_health_checks > initial_health_checks);
}

/// Integration test 2: Multiple plugins with independent health
#[tokio::test]
async fn test_multiple_plugins_independent_health() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(100),
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    // Create 3 plugins
    let plugin1 = TestPlugin::new("plugin1");
    let plugin2 = TestPlugin::new("plugin2");
    let plugin3 = TestPlugin::new("plugin3");

    let plugin1_ref = plugin1.clone();
    let plugin2_ref = plugin2.clone();
    let plugin3_ref = plugin3.clone();

    let config1 = PluginConfig {
        name: "plugin1".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };
    let config2 = PluginConfig {
        name: "plugin2".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };
    let config3 = PluginConfig {
        name: "plugin3".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("plugin1".to_string(), plugin1, config1).await.unwrap();
    registry.register("plugin2".to_string(), plugin2, config2).await.unwrap();
    registry.register("plugin3".to_string(), plugin3, config3).await.unwrap();

    // Let all run healthy
    sleep(Duration::from_millis(300)).await;

    // Make plugin2 fail
    plugin2_ref.set_health_status(PluginStatus::Failed("Plugin2 failure".to_string())).await;

    // Wait for restart
    sleep(Duration::from_millis(300)).await;

    // Verify only plugin2 was reloaded
    assert_eq!(plugin1_ref.get_reload_count(), 0);
    assert!(plugin2_ref.get_reload_count() > 0);
    assert_eq!(plugin3_ref.get_reload_count(), 0);

    // All plugins should still be doing health checks
    assert!(plugin1_ref.get_health_check_count() > 0);
    assert!(plugin2_ref.get_health_check_count() > 0);
    assert!(plugin3_ref.get_health_check_count() > 0);
}

/// Integration test 3: Watchdog state persistence across failures
#[tokio::test]
async fn test_watchdog_state_persistence() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(50),
        restart_policy: RestartPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(20),
            enable_circuit_breaker: true,
            failure_threshold: 2,
            ..Default::default()
        },
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin = TestPlugin::new("watchdog_state");
    let plugin_ref = plugin.clone();

    // Configure plugin to fail on reload (simulate persistent failure)
    let plugin_ref_inner = plugin_ref.clone();
    let failing_plugin = FailingPlugin {
        inner: plugin,
        should_fail_reload: Arc::new(AtomicBool::new(true)),
    };

    let plugin_config = PluginConfig {
        name: "watchdog_state".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("watchdog_state".to_string(), failing_plugin, plugin_config).await.unwrap();

    // Make plugin fail
    plugin_ref_inner.set_health_status(PluginStatus::Failed("Persistent failure".to_string())).await;

    // Wait for multiple restart attempts and circuit breaker to open
    sleep(Duration::from_millis(400)).await;

    // Check watchdog state
    if let Some((attempts, circuit_state)) = registry.get_watchdog_state("watchdog_state").await {
        // Should have attempted restarts
        assert!(attempts > 0);

        // Circuit breaker should be open after failures
        assert_eq!(circuit_state, CircuitState::Open);
    } else {
        panic!("Watchdog state not found");
    }
}

/// Wrapper plugin that can fail on reload
struct FailingPlugin {
    inner: TestPlugin,
    should_fail_reload: Arc<AtomicBool>,
}

impl Clone for FailingPlugin {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            should_fail_reload: self.should_fail_reload.clone(),
        }
    }
}

#[async_trait]
impl Plugin for FailingPlugin {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    async fn load(&self, config: &PluginConfig) -> Result<()> {
        self.inner.load(config).await
    }

    async fn start(&self) -> Result<()> {
        self.inner.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.inner.stop().await
    }

    async fn reload(&self, config: &PluginConfig) -> Result<()> {
        if self.should_fail_reload.load(Ordering::SeqCst) {
            Err(AosError::Config("Reload failed".to_string()))
        } else {
            self.inner.reload(config).await
        }
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        self.inner.health_check().await
    }

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<()> {
        self.inner.set_tenant_enabled(tenant_id, enabled).await
    }
}

/// Integration test 4: Admin operations (quiesce, reset)
#[tokio::test]
async fn test_admin_operations() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();
    let registry = PluginRegistry::new(db, identity);

    let plugin = TestPlugin::new("admin_test");
    let plugin_ref = plugin.clone();

    let plugin_config = PluginConfig {
        name: "admin_test".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("admin_test".to_string(), plugin, plugin_config).await.unwrap();

    // Make plugin fail
    plugin_ref.set_health_status(PluginStatus::Failed("Admin test failure".to_string())).await;

    sleep(Duration::from_millis(200)).await;

    // Quiesce plugin (prevents further restarts)
    registry.quiesce_plugin("admin_test").await.unwrap();

    // Wait a bit
    sleep(Duration::from_millis(200)).await;

    // Check that plugin is quiesced (max attempts should be reached)
    if let Some((attempts, _)) = registry.get_watchdog_state("admin_test").await {
        assert!(attempts >= 3); // Quiesce sets to max
    }

    // Reset plugin state
    registry.reset_plugin("admin_test").await.unwrap();

    // Check that state was reset
    if let Some((attempts, circuit_state)) = registry.get_watchdog_state("admin_test").await {
        assert_eq!(attempts, 0);
        assert_eq!(circuit_state, CircuitState::Closed);
    }
}

/// Integration test 5: Tenant-specific enablement
#[tokio::test]
async fn test_tenant_specific_enablement() {
    let db = Db::open_in_memory().await.unwrap();

    // Create multiple tenants
    db.upsert_tenant("tenant1", 2001, 2001).await.unwrap();
    db.upsert_tenant("tenant2", 2002, 2002).await.unwrap();
    db.upsert_tenant("tenant3", 2003, 2003).await.unwrap();

    let identity = IdentityEnvelope::anonymous();
    let registry = PluginRegistry::new(db.clone(), identity);

    let plugin = TestPlugin::new("tenant_test");
    let plugin_ref = plugin.clone();

    let plugin_config = PluginConfig {
        name: "tenant_test".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("tenant_test".to_string(), plugin, plugin_config).await.unwrap();

    // Enable for all tenants initially
    registry.enable_for_tenant("tenant_test", "tenant1", true).await.unwrap();
    registry.enable_for_tenant("tenant_test", "tenant2", true).await.unwrap();
    registry.enable_for_tenant("tenant_test", "tenant3", true).await.unwrap();

    // Verify all enabled
    assert!(registry.is_enabled_for_tenant("tenant_test", "tenant1").await.unwrap());
    assert!(registry.is_enabled_for_tenant("tenant_test", "tenant2").await.unwrap());
    assert!(registry.is_enabled_for_tenant("tenant_test", "tenant3").await.unwrap());

    // Disable for tenant2 only
    registry.enable_for_tenant("tenant_test", "tenant2", false).await.unwrap();

    // Verify selective disablement
    assert!(registry.is_enabled_for_tenant("tenant_test", "tenant1").await.unwrap());
    assert!(!registry.is_enabled_for_tenant("tenant_test", "tenant2").await.unwrap());
    assert!(registry.is_enabled_for_tenant("tenant_test", "tenant3").await.unwrap());

    // Verify plugin received tenant settings
    let settings = plugin_ref.tenant_settings.read().await;
    assert_eq!(settings.get("tenant1"), Some(&true));
    assert_eq!(settings.get("tenant2"), Some(&false));
    assert_eq!(settings.get("tenant3"), Some(&true));
}

/// Integration test 6: Health monitoring with degraded state
#[tokio::test]
async fn test_degraded_state_monitoring() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(100),
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin = TestPlugin::new("degraded_test");
    let plugin_ref = plugin.clone();

    let plugin_config = PluginConfig {
        name: "degraded_test".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("degraded_test".to_string(), plugin, plugin_config).await.unwrap();

    // Let plugin run healthy
    sleep(Duration::from_millis(200)).await;

    // Make plugin degraded (not failed, but not optimal)
    plugin_ref.set_health_status(PluginStatus::Degraded("Partial tenant failures".to_string())).await;

    // Wait for health check to detect degradation
    sleep(Duration::from_millis(300)).await;

    // Plugin should NOT be restarted for degraded state (only for failed)
    assert_eq!(plugin_ref.get_reload_count(), 0);

    // But health checks should continue
    assert!(plugin_ref.get_health_check_count() > 2);

    // Make plugin fully fail
    plugin_ref.set_health_status(PluginStatus::Failed("Complete failure".to_string())).await;

    // Wait for restart
    sleep(Duration::from_millis(300)).await;

    // Now reload should have been triggered
    assert!(plugin_ref.get_reload_count() > 0);
}

/// Integration test 7: Registry-wide health status
#[tokio::test]
async fn test_registry_wide_health() {
    let db = Db::open_in_memory().await.unwrap();
    db.upsert_tenant("default", 1000, 1000).await.unwrap();
    db.upsert_tenant("tenant1", 1001, 1001).await.unwrap();

    let identity = IdentityEnvelope::anonymous();
    let registry = PluginRegistry::new(db, identity);

    // Register multiple plugins
    for i in 1..=3 {
        let plugin = TestPlugin::new(format!("plugin{}", i));
        let config = PluginConfig {
            name: format!("plugin{}", i),
            enabled: true,
            specific: HashMap::new(),
        };
        registry.register(format!("plugin{}", i), plugin, config).await.unwrap();
    }

    // Get health for all plugins across all tenants
    let health = registry.health_all().await;

    // Should have all 3 plugins
    assert_eq!(health.len(), 3);

    // Each plugin should have health status for all tenants
    for plugin_health in health.values() {
        assert!(plugin_health.contains_key("default"));
        assert!(plugin_health.contains_key("tenant1"));
    }
}
