//! Plugin Isolation Integration Tests
//!
//! Tests per PRD-PLUG-01 requirements:
//! 1. Plugin panic does not crash server
//! 2. Plugin timeout does not block event bus
//! 3. Failed plugin error is logged but not propagated
//! 4. Multiple plugins receive events
//! 5. Plugin subscription filtering

use adapteros_core::{
    AdapterEvent, AosError, EventHookType, Plugin, PluginConfig, PluginEvent, PluginHealth,
    PluginStatus, Result,
};
use adapteros_server_api::event_bus::EventBus;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// ============================================================================
// Test Plugins
// ============================================================================

/// PanicPlugin - panics on every event
struct PanicPlugin {
    call_count: Arc<AtomicUsize>,
}

impl PanicPlugin {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Plugin for PanicPlugin {
    fn name(&self) -> &'static str {
        "panic-plugin"
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        Ok(PluginHealth {
            status: PluginStatus::Started,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventHookType> {
        vec![EventHookType::OnAdapterRegistered]
    }

    async fn on_event(&self, _event: &PluginEvent) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        panic!("PanicPlugin intentional panic");
    }
}

/// SlowPlugin - sleeps for 10 seconds (should timeout at 5 seconds)
struct SlowPlugin {
    call_count: Arc<AtomicUsize>,
}

impl SlowPlugin {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Plugin for SlowPlugin {
    fn name(&self) -> &'static str {
        "slow-plugin"
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        Ok(PluginHealth {
            status: PluginStatus::Started,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventHookType> {
        vec![EventHookType::OnAdapterRegistered]
    }

    async fn on_event(&self, _event: &PluginEvent) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        // Sleep for 10 seconds - should timeout at 5 seconds
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(())
    }
}

/// ErrorPlugin - returns Err on every event
struct ErrorPlugin {
    call_count: Arc<AtomicUsize>,
}

impl ErrorPlugin {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Plugin for ErrorPlugin {
    fn name(&self) -> &'static str {
        "error-plugin"
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        Ok(PluginHealth {
            status: PluginStatus::Started,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventHookType> {
        vec![EventHookType::OnAdapterRegistered]
    }

    async fn on_event(&self, _event: &PluginEvent) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Err(AosError::Internal("ErrorPlugin intentional error".to_string()))
    }
}

/// GoodPlugin - records received events for verification
struct GoodPlugin {
    call_count: Arc<AtomicUsize>,
    received_events: Arc<Mutex<Vec<PluginEvent>>>,
    subscribed_to: Vec<EventHookType>,
}

impl GoodPlugin {
    fn new(subscribed_to: Vec<EventHookType>) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            received_events: Arc::new(Mutex::new(Vec::new())),
            subscribed_to,
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    async fn received_events(&self) -> Vec<PluginEvent> {
        self.received_events.lock().await.clone()
    }
}

#[async_trait]
impl Plugin for GoodPlugin {
    fn name(&self) -> &'static str {
        "good-plugin"
    }

    async fn load(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        Ok(PluginHealth {
            status: PluginStatus::Started,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, _tenant_id: &str, _enabled: bool) -> Result<()> {
        Ok(())
    }

    fn subscribed_events(&self) -> Vec<EventHookType> {
        self.subscribed_to.clone()
    }

    async fn on_event(&self, event: &PluginEvent) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.received_events.lock().await.push(event.clone());
        Ok(())
    }
}

// ============================================================================
// Test Helper Functions
// ============================================================================

fn create_test_adapter_event() -> PluginEvent {
    PluginEvent::AdapterRegistered(AdapterEvent {
        adapter_id: "test-adapter".to_string(),
        action: "registered".to_string(),
        hash: Some("test-hash".to_string()),
        tier: Some("tier_1".to_string()),
        rank: Some(16),
        tenant_id: Some("test-tenant".to_string()),
        lifecycle_state: Some("cold".to_string()),
        timestamp: "2025-11-25T12:00:00Z".to_string(),
        metadata: HashMap::new(),
    })
}

fn create_test_adapter_loaded_event() -> PluginEvent {
    PluginEvent::AdapterLoaded(AdapterEvent {
        adapter_id: "test-adapter".to_string(),
        action: "loaded".to_string(),
        hash: Some("test-hash".to_string()),
        tier: Some("tier_1".to_string()),
        rank: Some(16),
        tenant_id: Some("test-tenant".to_string()),
        lifecycle_state: Some("warm".to_string()),
        timestamp: "2025-11-25T12:00:00Z".to_string(),
        metadata: HashMap::new(),
    })
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_plugin_panic_does_not_crash_server() {
    // Setup: Create event bus and register panic plugin
    let event_bus = EventBus::new(100);
    let panic_plugin = Arc::new(PanicPlugin::new());
    event_bus
        .register_plugin("panic-plugin".to_string(), panic_plugin.clone())
        .await;

    // Also register a good plugin to verify server continues working
    let good_plugin = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    event_bus
        .register_plugin("good-plugin".to_string(), good_plugin.clone())
        .await;

    // Act: Emit event that will cause panic
    let event = create_test_adapter_event();
    let result = event_bus.emit(event).await;

    // Assert: Server did not crash, but panic was caught and logged
    // emit() should return Err with failed plugin name
    assert!(result.is_err(), "Should return error for failed plugin");
    let failures = result.unwrap_err();
    assert_eq!(failures.len(), 1, "Should have exactly one failed plugin");
    assert_eq!(failures[0], "panic-plugin", "Panic plugin should be listed");

    // Verify panic plugin was called
    assert_eq!(
        panic_plugin.call_count(),
        1,
        "Panic plugin should be called"
    );

    // Verify good plugin was still called (server continues working)
    assert_eq!(good_plugin.call_count(), 1, "Good plugin should be called");
}

#[tokio::test]
async fn test_plugin_timeout_does_not_block_event_bus() {
    // Setup: Create event bus and register slow plugin
    let event_bus = EventBus::new(100);
    let slow_plugin = Arc::new(SlowPlugin::new());
    event_bus
        .register_plugin("slow-plugin".to_string(), slow_plugin.clone())
        .await;

    // Also register a good plugin to verify bus is not blocked
    let good_plugin = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    event_bus
        .register_plugin("good-plugin".to_string(), good_plugin.clone())
        .await;

    // Act: Emit event and measure timeout enforcement
    let event = create_test_adapter_event();
    let start = std::time::Instant::now();
    let result = event_bus.emit(event).await;
    let elapsed = start.elapsed();

    // Assert: Should timeout at 5 seconds, not wait for full 10 seconds
    assert!(
        elapsed < Duration::from_secs(7),
        "Should timeout within 7 seconds (5s timeout + overhead), got {:?}",
        elapsed
    );
    assert!(
        elapsed >= Duration::from_secs(5),
        "Should wait at least 5 seconds for timeout, got {:?}",
        elapsed
    );

    // emit() should return Err with failed plugin name
    assert!(result.is_err(), "Should return error for timed out plugin");
    let failures = result.unwrap_err();
    assert_eq!(failures.len(), 1, "Should have exactly one failed plugin");
    assert_eq!(failures[0], "slow-plugin", "Slow plugin should be listed");

    // Verify slow plugin was called (but timed out)
    assert_eq!(slow_plugin.call_count(), 1, "Slow plugin should be called");

    // Verify good plugin was called (bus not blocked)
    assert_eq!(good_plugin.call_count(), 1, "Good plugin should be called");
}

#[tokio::test]
async fn test_failed_plugin_error_logged_but_not_propagated() {
    // Setup: Create event bus and register error plugin
    let event_bus = EventBus::new(100);
    let error_plugin = Arc::new(ErrorPlugin::new());
    event_bus
        .register_plugin("error-plugin".to_string(), error_plugin.clone())
        .await;

    // Also register a good plugin to verify bus continues
    let good_plugin = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    event_bus
        .register_plugin("good-plugin".to_string(), good_plugin.clone())
        .await;

    // Act: Emit event that will cause error
    let event = create_test_adapter_event();
    let result = event_bus.emit(event).await;

    // Assert: Error was logged and returned, but server continues
    assert!(result.is_err(), "Should return error for failed plugin");
    let failures = result.unwrap_err();
    assert_eq!(failures.len(), 1, "Should have exactly one failed plugin");
    assert_eq!(failures[0], "error-plugin", "Error plugin should be listed");

    // Verify error plugin was called
    assert_eq!(
        error_plugin.call_count(),
        1,
        "Error plugin should be called"
    );

    // Verify good plugin was still called (error not propagated)
    assert_eq!(good_plugin.call_count(), 1, "Good plugin should be called");
}

#[tokio::test]
async fn test_multiple_plugins_receive_events() {
    // Setup: Create event bus and register multiple good plugins
    let event_bus = EventBus::new(100);

    let plugin1 = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    let plugin2 = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    let plugin3 = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));

    event_bus
        .register_plugin("plugin1".to_string(), plugin1.clone())
        .await;
    event_bus
        .register_plugin("plugin2".to_string(), plugin2.clone())
        .await;
    event_bus
        .register_plugin("plugin3".to_string(), plugin3.clone())
        .await;

    // Act: Emit event
    let event = create_test_adapter_event();
    let result = event_bus.emit(event.clone()).await;

    // Assert: All plugins received the event
    assert!(result.is_ok(), "Should succeed with all good plugins");

    assert_eq!(plugin1.call_count(), 1, "Plugin1 should be called once");
    assert_eq!(plugin2.call_count(), 1, "Plugin2 should be called once");
    assert_eq!(plugin3.call_count(), 1, "Plugin3 should be called once");

    // Verify all plugins received the same event
    let events1 = plugin1.received_events().await;
    let events2 = plugin2.received_events().await;
    let events3 = plugin3.received_events().await;

    assert_eq!(events1.len(), 1, "Plugin1 should have one event");
    assert_eq!(events2.len(), 1, "Plugin2 should have one event");
    assert_eq!(events3.len(), 1, "Plugin3 should have one event");

    // Verify event data matches
    if let PluginEvent::AdapterRegistered(ref e1) = events1[0] {
        if let PluginEvent::AdapterRegistered(ref e2) = events2[0] {
            if let PluginEvent::AdapterRegistered(ref e3) = events3[0] {
                assert_eq!(e1.adapter_id, "test-adapter");
                assert_eq!(e2.adapter_id, "test-adapter");
                assert_eq!(e3.adapter_id, "test-adapter");
            } else {
                panic!("Plugin3 received wrong event type");
            }
        } else {
            panic!("Plugin2 received wrong event type");
        }
    } else {
        panic!("Plugin1 received wrong event type");
    }
}

#[tokio::test]
async fn test_plugin_subscription_filtering() {
    // Setup: Create event bus with plugins subscribed to different events
    let event_bus = EventBus::new(100);

    // Plugin subscribed only to OnAdapterRegistered
    let plugin_registered = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));

    // Plugin subscribed only to OnAdapterLoaded
    let plugin_loaded = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterLoaded]));

    // Plugin subscribed to both
    let plugin_both = Arc::new(GoodPlugin::new(vec![
        EventHookType::OnAdapterRegistered,
        EventHookType::OnAdapterLoaded,
    ]));

    event_bus
        .register_plugin("plugin-registered".to_string(), plugin_registered.clone())
        .await;
    event_bus
        .register_plugin("plugin-loaded".to_string(), plugin_loaded.clone())
        .await;
    event_bus
        .register_plugin("plugin-both".to_string(), plugin_both.clone())
        .await;

    // Act 1: Emit OnAdapterRegistered event
    let event1 = create_test_adapter_event();
    let result1 = event_bus.emit(event1).await;
    assert!(result1.is_ok(), "First event should succeed");

    // Assert 1: Only plugins subscribed to OnAdapterRegistered receive it
    assert_eq!(
        plugin_registered.call_count(),
        1,
        "plugin-registered should receive event"
    );
    assert_eq!(
        plugin_loaded.call_count(),
        0,
        "plugin-loaded should NOT receive event"
    );
    assert_eq!(
        plugin_both.call_count(),
        1,
        "plugin-both should receive event"
    );

    // Act 2: Emit OnAdapterLoaded event
    let event2 = create_test_adapter_loaded_event();
    let result2 = event_bus.emit(event2).await;
    assert!(result2.is_ok(), "Second event should succeed");

    // Assert 2: Only plugins subscribed to OnAdapterLoaded receive it
    assert_eq!(
        plugin_registered.call_count(),
        1,
        "plugin-registered should still have 1 event (no change)"
    );
    assert_eq!(
        plugin_loaded.call_count(),
        1,
        "plugin-loaded should now receive event"
    );
    assert_eq!(
        plugin_both.call_count(),
        2,
        "plugin-both should receive both events"
    );

    // Verify received event types
    let events_registered = plugin_registered.received_events().await;
    let events_loaded = plugin_loaded.received_events().await;
    let events_both = plugin_both.received_events().await;

    assert_eq!(events_registered.len(), 1);
    assert_eq!(events_loaded.len(), 1);
    assert_eq!(events_both.len(), 2);

    // Verify correct event types
    assert!(
        matches!(events_registered[0], PluginEvent::AdapterRegistered(_)),
        "plugin-registered should receive AdapterRegistered"
    );
    assert!(
        matches!(events_loaded[0], PluginEvent::AdapterLoaded(_)),
        "plugin-loaded should receive AdapterLoaded"
    );
    assert!(
        matches!(events_both[0], PluginEvent::AdapterRegistered(_)),
        "plugin-both first event should be AdapterRegistered"
    );
    assert!(
        matches!(events_both[1], PluginEvent::AdapterLoaded(_)),
        "plugin-both second event should be AdapterLoaded"
    );
}

#[tokio::test]
async fn test_mixed_plugin_failures_do_not_stop_good_plugins() {
    // Setup: Mix of panic, timeout, error, and good plugins
    let event_bus = EventBus::new(100);

    let panic_plugin = Arc::new(PanicPlugin::new());
    let slow_plugin = Arc::new(SlowPlugin::new());
    let error_plugin = Arc::new(ErrorPlugin::new());
    let good_plugin1 = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));
    let good_plugin2 = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterRegistered]));

    event_bus
        .register_plugin("panic-plugin".to_string(), panic_plugin.clone())
        .await;
    event_bus
        .register_plugin("slow-plugin".to_string(), slow_plugin.clone())
        .await;
    event_bus
        .register_plugin("error-plugin".to_string(), error_plugin.clone())
        .await;
    event_bus
        .register_plugin("good-plugin1".to_string(), good_plugin1.clone())
        .await;
    event_bus
        .register_plugin("good-plugin2".to_string(), good_plugin2.clone())
        .await;

    // Act: Emit event
    let event = create_test_adapter_event();
    let result = event_bus.emit(event).await;

    // Assert: Should return errors for failed plugins
    assert!(result.is_err(), "Should return errors for failed plugins");
    let failures = result.unwrap_err();
    assert_eq!(failures.len(), 3, "Should have 3 failed plugins");

    // Verify all failed plugin names are present
    assert!(
        failures.contains(&"panic-plugin".to_string()),
        "panic-plugin should be in failures"
    );
    assert!(
        failures.contains(&"slow-plugin".to_string()),
        "slow-plugin should be in failures"
    );
    assert!(
        failures.contains(&"error-plugin".to_string()),
        "error-plugin should be in failures"
    );

    // Verify good plugins still received the event
    assert_eq!(
        good_plugin1.call_count(),
        1,
        "good-plugin1 should be called"
    );
    assert_eq!(
        good_plugin2.call_count(),
        1,
        "good-plugin2 should be called"
    );

    // Verify failed plugins were called (failures happened during execution)
    assert_eq!(
        panic_plugin.call_count(),
        1,
        "panic-plugin should be called"
    );
    assert_eq!(slow_plugin.call_count(), 1, "slow-plugin should be called");
    assert_eq!(
        error_plugin.call_count(),
        1,
        "error-plugin should be called"
    );
}

#[tokio::test]
async fn test_event_bus_with_no_subscribers() {
    // Setup: Event bus with no registered plugins
    let event_bus = EventBus::new(100);

    // Act: Emit event with no subscribers
    let event = create_test_adapter_event();
    let result = event_bus.emit(event).await;

    // Assert: Should succeed with no subscribers
    assert!(
        result.is_ok(),
        "Should succeed when no plugins are subscribed"
    );
}

#[tokio::test]
async fn test_event_bus_with_unsubscribed_event_type() {
    // Setup: Register plugin subscribed to different event type
    let event_bus = EventBus::new(100);
    let plugin = Arc::new(GoodPlugin::new(vec![EventHookType::OnAdapterLoaded]));
    event_bus
        .register_plugin("plugin".to_string(), plugin.clone())
        .await;

    // Act: Emit event of different type
    let event = create_test_adapter_event(); // OnAdapterRegistered
    let result = event_bus.emit(event).await;

    // Assert: Should succeed but plugin not called
    assert!(result.is_ok(), "Should succeed");
    assert_eq!(plugin.call_count(), 0, "Plugin should not be called");
}
