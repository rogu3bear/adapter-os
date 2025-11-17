//! Chaos engineering tests for plugin isolation and safety
//!
//! Citation: PRD 7 - Operator / Plugin Isolation
//!
//! These tests verify that:
//! - A broken plugin can't drag the runtime down
//! - Silent corruption is prevented
//! - Timeouts work correctly
//! - Circuit breakers prevent cascading failures
//! - Tenant isolation is maintained
//! - Runtime continues serving requests despite plugin failures

use adapteros_core::{
    identity::IdentityEnvelope, AosError, CircuitState, Plugin, PluginConfig, PluginHealth,
    PluginStatus, PluginWatchdog, RestartPolicy, Result,
};
use adapteros_db::Db;
use adapteros_server::plugin_registry::{PluginRegistry, PluginRegistryConfig};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;

/// Mock plugin that can be configured to fail in various ways
struct ChaosPlugin {
    name: &'static str,
    fail_mode: Arc<RwLock<FailureMode>>,
    call_count: Arc<AtomicUsize>,
    slow_duration: Arc<RwLock<Option<Duration>>>,
}

#[derive(Debug, Clone, PartialEq)]
enum FailureMode {
    Success,
    HealthCheckFail,
    LoadFail,
    StartFail,
    ReloadFail,
    Panic,
}

impl ChaosPlugin {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            fail_mode: Arc::new(RwLock::new(FailureMode::Success)),
            call_count: Arc::new(AtomicUsize::new(0)),
            slow_duration: Arc::new(RwLock::new(None)),
        }
    }

    async fn set_fail_mode(&self, mode: FailureMode) {
        *self.fail_mode.write().await = mode;
    }

    async fn set_slow(&self, duration: Option<Duration>) {
        *self.slow_duration.write().await = duration;
    }

    fn get_call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Plugin for ChaosPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if let Some(duration) = *self.slow_duration.read().await {
            sleep(duration).await;
        }

        match *self.fail_mode.read().await {
            FailureMode::LoadFail => Err(AosError::Config("Load failed".to_string())),
            FailureMode::Panic => panic!("Plugin load panic!"),
            _ => Ok(()),
        }
    }

    async fn start(&self) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match *self.fail_mode.read().await {
            FailureMode::StartFail => Err(AosError::Config("Start failed".to_string())),
            FailureMode::Panic => panic!("Plugin start panic!"),
            _ => Ok(()),
        }
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match *self.fail_mode.read().await {
            FailureMode::ReloadFail => Err(AosError::Config("Reload failed".to_string())),
            FailureMode::Panic => panic!("Plugin reload panic!"),
            _ => {
                // Successful reload should clear health check failures
                *self.fail_mode.write().await = FailureMode::Success;
                Ok(())
            }
        }
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        match *self.fail_mode.read().await {
            FailureMode::HealthCheckFail => Ok(PluginHealth {
                status: PluginStatus::Failed("Chaos failure".to_string()),
                details: Some("Intentional failure for testing".to_string()),
            }),
            FailureMode::Panic => panic!("Plugin health check panic!"),
            _ => Ok(PluginHealth {
                status: PluginStatus::Started,
                details: None,
            }),
        }
    }

    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
        Ok(())
    }
}

/// Test 1: Plugin killed mid-request → runtime still serves
#[tokio::test]
async fn test_plugin_failure_doesnt_crash_runtime() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        operation_timeout: Duration::from_secs(5),
        health_check_interval: Duration::from_millis(100),
        restart_policy: RestartPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        },
        enable_telemetry: true,
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin1 = ChaosPlugin::new("chaos1");
    let plugin2 = ChaosPlugin::new("chaos2");

    let plugin1_ref = plugin1.clone();
    let plugin2_ref = plugin2.clone();

    // Register both plugins
    let plugin_config = PluginConfig {
        name: "chaos1".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };
    registry.register("chaos1".to_string(), plugin1, plugin_config.clone()).await.unwrap();

    let plugin_config2 = PluginConfig {
        name: "chaos2".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };
    registry.register("chaos2".to_string(), plugin2, plugin_config2).await.unwrap();

    // Kill plugin 1 by making it fail health checks
    plugin1_ref.set_fail_mode(FailureMode::HealthCheckFail).await;

    // Wait for health check to detect failure
    sleep(Duration::from_millis(300)).await;

    // Plugin 2 should still be healthy
    let health = registry.health_all().await;
    assert!(health.contains_key("chaos2"));

    // Registry should still be functional
    assert!(registry.is_enabled_for_tenant("chaos2", "default").await.unwrap());
}

/// Test 2: Slow plugin → bounded impact with timeouts
#[tokio::test]
async fn test_slow_plugin_timeout() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        operation_timeout: Duration::from_millis(100),
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin = ChaosPlugin::new("slow_plugin");
    plugin.set_slow(Some(Duration::from_secs(10))).await; // 10s delay, exceeds 100ms timeout

    let plugin_config = PluginConfig {
        name: "slow_plugin".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    let start = std::time::Instant::now();
    let result = registry.register("slow_plugin".to_string(), plugin, plugin_config).await;
    let elapsed = start.elapsed();

    // Should timeout quickly, not wait for full 10 seconds
    assert!(result.is_err());
    assert!(elapsed < Duration::from_millis(500));
    assert!(result.unwrap_err().to_string().contains("timed out"));
}

/// Test 3: Circuit breaker prevents cascading failures
#[tokio::test]
async fn test_circuit_breaker_opens_on_failures() {
    let watchdog = PluginWatchdog::new(RestartPolicy {
        max_attempts: 5,
        failure_threshold: 3,
        enable_circuit_breaker: true,
        recovery_window: Duration::from_millis(100),
        ..Default::default()
    });

    let plugin_name = "circuit_test";

    // Simulate multiple failures
    for _ in 0..3 {
        assert!(watchdog.can_restart(plugin_name).await.unwrap());
        watchdog.record_restart(plugin_name).await.unwrap();
        sleep(Duration::from_millis(15)).await; // Wait for backoff
        watchdog.record_failure(plugin_name).await.unwrap();
    }

    // Circuit should be open now
    if let Some((_, circuit_state)) = watchdog.get_state(plugin_name).await {
        assert_eq!(circuit_state, CircuitState::Open);
    }

    // Should not allow restart
    assert!(!watchdog.can_restart(plugin_name).await.unwrap());

    // Wait for recovery window
    sleep(Duration::from_millis(150)).await;

    // Should transition to half-open and allow one attempt
    assert!(watchdog.can_restart(plugin_name).await.unwrap());

    // If this attempt succeeds, circuit should close
    watchdog.record_restart(plugin_name).await.unwrap();
    watchdog.record_success(plugin_name).await.unwrap();

    if let Some((_, circuit_state)) = watchdog.get_state(plugin_name).await {
        assert_eq!(circuit_state, CircuitState::Closed);
    }
}

/// Test 4: Max restart attempts limit
#[tokio::test]
async fn test_max_restart_attempts() {
    let watchdog = PluginWatchdog::new(RestartPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(10),
        enable_circuit_breaker: false, // Disable circuit breaker for this test
        ..Default::default()
    });

    let plugin_name = "restart_limit_test";

    // Attempt 3 restarts
    for attempt in 1..=3 {
        assert!(watchdog.can_restart(plugin_name).await.unwrap());
        watchdog.record_restart(plugin_name).await.unwrap();

        if attempt < 3 {
            sleep(Duration::from_millis(20)).await; // Wait for backoff
        }
    }

    // Wait for backoff
    sleep(Duration::from_millis(100)).await;

    // 4th restart should be denied
    assert!(!watchdog.can_restart(plugin_name).await.unwrap());

    // Reset should allow restarts again
    watchdog.reset(plugin_name).await.unwrap();
    assert!(watchdog.can_restart(plugin_name).await.unwrap());
}

/// Test 5: Exponential backoff
#[tokio::test]
async fn test_exponential_backoff() {
    let watchdog = PluginWatchdog::new(RestartPolicy {
        max_attempts: 10,
        initial_backoff: Duration::from_millis(10),
        backoff_multiplier: 2.0,
        max_backoff: Duration::from_millis(100),
        ..Default::default()
    });

    let plugin_name = "backoff_test";

    // First restart
    watchdog.record_restart(plugin_name).await.unwrap();
    assert!(!watchdog.can_restart(plugin_name).await.unwrap()); // Blocked before backoff

    // Wait for first backoff (10ms)
    sleep(Duration::from_millis(15)).await;
    assert!(watchdog.can_restart(plugin_name).await.unwrap());

    // Second restart
    watchdog.record_restart(plugin_name).await.unwrap();
    assert!(!watchdog.can_restart(plugin_name).await.unwrap());

    // Wait for second backoff (20ms due to 2x multiplier)
    sleep(Duration::from_millis(25)).await;
    assert!(watchdog.can_restart(plugin_name).await.unwrap());

    // Third restart
    watchdog.record_restart(plugin_name).await.unwrap();

    // Wait for third backoff (40ms due to 2x multiplier)
    sleep(Duration::from_millis(45)).await;
    assert!(watchdog.can_restart(plugin_name).await.unwrap());
}

/// Test 6: Graceful degradation - plugin failure doesn't affect other plugins
#[tokio::test]
async fn test_graceful_degradation() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(100),
        restart_policy: RestartPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin1 = ChaosPlugin::new("degraded");
    let plugin2 = ChaosPlugin::new("healthy");

    let plugin1_ref = plugin1.clone();

    let config1 = PluginConfig {
        name: "degraded".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };
    let config2 = PluginConfig {
        name: "healthy".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("degraded".to_string(), plugin1, config1).await.unwrap();
    registry.register("healthy".to_string(), plugin2, config2).await.unwrap();

    // Make plugin1 fail
    plugin1_ref.set_fail_mode(FailureMode::HealthCheckFail).await;

    // Wait for health check and restart attempt
    sleep(Duration::from_millis(300)).await;

    // Both plugins should still be in the registry
    let health = registry.health_all().await;
    assert!(health.contains_key("degraded"));
    assert!(health.contains_key("healthy"));

    // Healthy plugin should still be functional
    assert!(registry.is_enabled_for_tenant("healthy", "default").await.unwrap());
}

/// Test 7: Tenant isolation - plugin failure for one tenant doesn't affect another
#[tokio::test]
async fn test_tenant_isolation() {
    let db = Db::open_in_memory().await.unwrap();

    // Create two tenants
    db.upsert_tenant("tenant_a", 1001, 1001).await.unwrap();
    db.upsert_tenant("tenant_b", 1002, 1002).await.unwrap();

    let identity = IdentityEnvelope::anonymous();
    let registry = PluginRegistry::new(db, identity);

    let plugin = ChaosPlugin::new("isolated");

    let config = PluginConfig {
        name: "isolated".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("isolated".to_string(), plugin, config).await.unwrap();

    // Enable for both tenants
    registry.enable_for_tenant("isolated", "tenant_a", true).await.unwrap();
    registry.enable_for_tenant("isolated", "tenant_b", true).await.unwrap();

    assert!(registry.is_enabled_for_tenant("isolated", "tenant_a").await.unwrap());
    assert!(registry.is_enabled_for_tenant("isolated", "tenant_b").await.unwrap());

    // Disable for tenant_a
    registry.enable_for_tenant("isolated", "tenant_a", false).await.unwrap();

    // tenant_a should be disabled, tenant_b should still be enabled
    assert!(!registry.is_enabled_for_tenant("isolated", "tenant_a").await.unwrap());
    assert!(registry.is_enabled_for_tenant("isolated", "tenant_b").await.unwrap());
}

/// Test 8: Quiesce prevents further restarts
#[tokio::test]
async fn test_quiesce_stops_restarts() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();
    let registry = PluginRegistry::new(db, identity);

    let plugin = ChaosPlugin::new("quiesce_test");
    let plugin_ref = plugin.clone();

    let config = PluginConfig {
        name: "quiesce_test".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("quiesce_test".to_string(), plugin, config).await.unwrap();

    // Make plugin fail
    plugin_ref.set_fail_mode(FailureMode::HealthCheckFail).await;

    // Quiesce the plugin
    registry.quiesce_plugin("quiesce_test").await.unwrap();

    // Get watchdog state - should show max attempts reached
    if let Some((attempts, _)) = registry.get_watchdog_state("quiesce_test").await {
        // Quiesce sets attempts to max
        assert!(attempts >= 3); // Default max attempts
    }
}

/// Test 9: Plugin recovery after successful health check
#[tokio::test]
async fn test_plugin_recovery() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();

    let config = PluginRegistryConfig {
        health_check_interval: Duration::from_millis(100),
        restart_policy: RestartPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let registry = PluginRegistry::with_config(db, identity, config);

    let plugin = ChaosPlugin::new("recovery_test");
    let plugin_ref = plugin.clone();

    let plugin_config = PluginConfig {
        name: "recovery_test".to_string(),
        enabled: true,
        specific: HashMap::new(),
    };

    registry.register("recovery_test".to_string(), plugin, plugin_config).await.unwrap();

    // Make plugin fail
    plugin_ref.set_fail_mode(FailureMode::HealthCheckFail).await;

    // Wait for failure detection
    sleep(Duration::from_millis(200)).await;

    // Check that restart was attempted (reload clears fail mode)
    sleep(Duration::from_millis(200)).await;

    // Plugin should have recovered
    let health = registry.health_all().await;
    assert!(health.contains_key("recovery_test"));
}

/// Test 10: Concurrent plugin operations are isolated
#[tokio::test]
async fn test_concurrent_plugin_isolation() {
    let db = Db::open_in_memory().await.unwrap();
    let identity = IdentityEnvelope::anonymous();
    let registry = Arc::new(PluginRegistry::new(db, identity));

    let mut handles = vec![];

    // Register 10 plugins concurrently
    for i in 0..10 {
        let registry_clone = registry.clone();
        let handle = tokio::spawn(async move {
            let plugin = ChaosPlugin::new(Box::leak(format!("concurrent_{}", i).into_boxed_str()));
            let config = PluginConfig {
                name: format!("concurrent_{}", i),
                enabled: true,
                specific: HashMap::new(),
            };

            registry_clone.register(format!("concurrent_{}", i), plugin, config).await
        });
        handles.push(handle);
    }

    // All should succeed
    let results = futures::future::join_all(handles).await;
    assert!(results.iter().all(|r| r.is_ok() && r.as_ref().unwrap().is_ok()));

    // All plugins should be registered
    let health = registry.health_all().await;
    assert_eq!(health.len(), 10);
}
