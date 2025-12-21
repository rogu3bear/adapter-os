//! Plugin Event Bus
//!
//! Provides event dispatch mechanism for plugin event hooks. Features:
//! - Panic isolation for plugin handlers
//! - 5-second timeout per plugin event handler
//! - Automatic subscription management based on plugin's subscribed_events()
//! - Non-blocking broadcast channel distribution
//! - Error logging without server crashes
//!
//! # Example
//!
//! ```no_run
//! use adapteros_server_api::event_bus::EventBus;
//! use adapteros_core::{PluginEvent, Plugin};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let event_bus = EventBus::new(1000);
//!
//! // Register plugins
//! let plugin: Arc<dyn Plugin> = todo!();
//! event_bus.register_plugin("my-plugin".to_string(), plugin).await;
//!
//! // Emit events
//! let event = PluginEvent::AdapterRegistered(todo!());
//! event_bus.emit(event).await?;
//! # Ok(())
//! # }
//! ```

use adapteros_core::{EventHookType, Plugin, PluginEvent};
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// Plugin event bus for dispatching events to registered plugins
///
/// Provides fault-tolerant event dispatch with:
/// - Panic isolation (plugins can't crash the server)
/// - Timeout enforcement (5 seconds per handler)
/// - Automatic subscription management
/// - Non-blocking event distribution
#[derive(Clone)]
pub struct EventBus {
    /// Registry of all registered plugins
    registry: Arc<RwLock<HashMap<String, Arc<dyn Plugin>>>>,
    /// Subscription mapping: EventHookType -> list of plugin names
    subscriptions: Arc<RwLock<HashMap<EventHookType, Vec<String>>>>,
    /// Broadcast channel for event distribution
    event_tx: broadcast::Sender<PluginEvent>,
}

impl EventBus {
    /// Create a new event bus with the specified broadcast channel capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events that can be buffered in the broadcast channel
    ///
    /// # Example
    ///
    /// ```
    /// use adapteros_server_api::event_bus::EventBus;
    ///
    /// let event_bus = EventBus::new(1000);
    /// ```
    pub fn new(capacity: usize) -> Self {
        let (event_tx, _) = broadcast::channel(capacity);

        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Register a plugin with the event bus
    ///
    /// Automatically subscribes the plugin to events based on its `subscribed_events()` method.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique plugin identifier
    /// * `plugin` - Plugin implementation
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_server_api::event_bus::EventBus;
    /// # use adapteros_core::Plugin;
    /// # use std::sync::Arc;
    /// # async fn example(event_bus: &EventBus, plugin: Arc<dyn Plugin>) {
    /// event_bus.register_plugin("my-plugin".to_string(), plugin).await;
    /// # }
    /// ```
    pub async fn register_plugin(&self, name: String, plugin: Arc<dyn Plugin>) {
        let event_types = plugin.subscribed_events();

        info!(
            plugin = %name,
            event_count = event_types.len(),
            "Registering plugin with event bus"
        );

        // Add to registry
        {
            let mut registry = self.registry.write().await;
            registry.insert(name.clone(), plugin);
        }

        // Update subscriptions
        {
            let mut subscriptions = self.subscriptions.write().await;
            for event_type in event_types {
                subscriptions
                    .entry(event_type)
                    .or_insert_with(Vec::new)
                    .push(name.clone());

                debug!(
                    plugin = %name,
                    event_type = ?event_type,
                    "Plugin subscribed to event type"
                );
            }
        }
    }

    /// Emit an event to all subscribed plugins
    ///
    /// Features:
    /// - Timeout enforcement (5 seconds per plugin handler)
    /// - Panic isolation (plugin crashes don't affect the server)
    /// - Error logging (failures are logged but don't stop event dispatch)
    /// - Non-blocking (uses broadcast channel)
    ///
    /// Returns a list of plugin names that failed to handle the event.
    ///
    /// # Arguments
    ///
    /// * `event` - Event to dispatch to plugins
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Event dispatched successfully (failures are logged but not returned as errors)
    /// * `Err(failures)` - List of plugin names that failed to handle the event
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use adapteros_server_api::event_bus::EventBus;
    /// # use adapteros_core::{PluginEvent, AdapterEvent};
    /// # use std::collections::HashMap;
    /// # async fn example(event_bus: &EventBus) -> Result<(), Vec<String>> {
    /// let event = PluginEvent::AdapterRegistered(AdapterEvent {
    ///     adapter_id: "my-adapter".to_string(),
    ///     action: "registered".to_string(),
    ///     hash: None,
    ///     tier: None,
    ///     rank: None,
    ///     tenant_id: None,
    ///     lifecycle_state: None,
    ///     timestamp: "2025-11-25T12:00:00Z".to_string(),
    ///     metadata: HashMap::new(),
    /// });
    ///
    /// event_bus.emit(event).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn emit(&self, event: PluginEvent) -> Result<(), Vec<String>> {
        let event_type = self.event_to_hook_type(&event);

        debug!(
            event_type = ?event_type,
            event_timestamp = %event.timestamp(),
            tenant_id = ?event.tenant_id(),
            "Emitting plugin event"
        );

        // Get subscribed plugins for this event type
        let plugin_names = {
            let subscriptions = self.subscriptions.read().await;
            subscriptions.get(&event_type).cloned().unwrap_or_default()
        };

        if plugin_names.is_empty() {
            debug!(event_type = ?event_type, "No plugins subscribed to event type");
            return Ok(());
        }

        debug!(
            event_type = ?event_type,
            subscriber_count = plugin_names.len(),
            "Dispatching event to subscribers"
        );

        let mut failures = Vec::new();

        // Dispatch to each subscribed plugin
        for plugin_name in plugin_names {
            let plugin = {
                let registry = self.registry.read().await;
                registry.get(&plugin_name).cloned()
            };

            let Some(plugin) = plugin else {
                warn!(
                    plugin = %plugin_name,
                    event_type = ?event_type,
                    "Plugin subscribed but not found in registry"
                );
                failures.push(plugin_name);
                continue;
            };

            // Clone event for this plugin handler
            let event_clone = event.clone();

            // Dispatch with timeout and panic isolation
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                Self::isolated_dispatch(plugin, event_clone, plugin_name.clone()),
            )
            .await;

            match result {
                Ok(Ok(())) => {
                    debug!(
                        plugin = %plugin_name,
                        event_type = ?event_type,
                        "Plugin handled event successfully"
                    );
                }
                Ok(Err(e)) => {
                    error!(
                        plugin = %plugin_name,
                        event_type = ?event_type,
                        error = %e,
                        "Plugin handler returned error"
                    );
                    failures.push(plugin_name);
                }
                Err(_) => {
                    error!(
                        plugin = %plugin_name,
                        event_type = ?event_type,
                        timeout_secs = 5,
                        "Plugin handler timed out"
                    );
                    failures.push(plugin_name);
                }
            }
        }

        // Broadcast event (non-blocking)
        if let Err(e) = self.event_tx.send(event) {
            warn!(
                error = %e,
                "Failed to broadcast event (no receivers)"
            );
        }

        if !failures.is_empty() {
            Err(failures)
        } else {
            Ok(())
        }
    }

    /// Spawn a background dispatcher task
    ///
    /// Creates a long-lived task that listens for events on the broadcast channel
    /// and dispatches them to plugins. This is useful for decoupling event
    /// emission from event handling.
    ///
    /// Returns a join handle that can be used to await the task's completion.
    ///
    /// # Example
    ///
    /// ```
    /// # use adapteros_server_api::event_bus::EventBus;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let event_bus = EventBus::new(1000);
    /// let handle = event_bus.spawn_dispatcher();
    ///
    /// // Event bus is now running in the background
    /// // To shut down:
    /// // handle.abort();
    /// # }
    /// ```
    pub fn spawn_dispatcher(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.event_tx.subscribe();
        let bus = self.clone();

        tokio::spawn(async move {
            info!("Event bus dispatcher started");

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        debug!(
                            event_type = %event.event_type(),
                            timestamp = %event.timestamp(),
                            "Dispatcher received event"
                        );

                        // Emit to plugins (ignore failures - already logged)
                        let _ = bus.emit(event).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            skipped_events = skipped,
                            "Event bus dispatcher lagged behind, some events skipped"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Event bus channel closed, stopping dispatcher");
                        break;
                    }
                }
            }

            info!("Event bus dispatcher stopped");
        })
    }

    /// Dispatch event to plugin with panic isolation
    ///
    /// Wraps the plugin's on_event handler in panic catching logic to prevent
    /// plugin crashes from taking down the server.
    async fn isolated_dispatch(
        plugin: Arc<dyn Plugin>,
        event: PluginEvent,
        plugin_name: String,
    ) -> Result<(), String> {
        use futures_util::FutureExt;

        // Wrap plugin handler in panic catcher
        let result = AssertUnwindSafe(plugin.on_event(&event))
            .catch_unwind()
            .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(format!("Plugin handler error: {}", e)),
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                error!(
                    plugin = %plugin_name,
                    panic = %panic_msg,
                    "Plugin handler panicked (isolated)"
                );

                Err(format!("Plugin panicked: {}", panic_msg))
            }
        }
    }

    /// Map PluginEvent to EventHookType
    fn event_to_hook_type(&self, event: &PluginEvent) -> EventHookType {
        match event {
            PluginEvent::TrainingJob(_) => EventHookType::OnTrainingJobEvent,
            PluginEvent::AdapterRegistered(_) => EventHookType::OnAdapterRegistered,
            PluginEvent::AdapterLoaded(_) => EventHookType::OnAdapterLoaded,
            PluginEvent::AdapterUnloaded(_) => EventHookType::OnAdapterUnloaded,
            PluginEvent::Audit(_) => EventHookType::OnAuditEvent,
            PluginEvent::MetricsTick(_) => EventHookType::OnMetricsTick,
            PluginEvent::InferenceComplete(_) => EventHookType::OnInferenceComplete,
            PluginEvent::PolicyViolation(_) => EventHookType::OnPolicyViolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::{AdapterEvent, AosError, PluginConfig, PluginHealth, PluginStatus};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestPlugin {
        name: &'static str,
        events: Vec<EventHookType>,
        call_count: Arc<AtomicUsize>,
        should_panic: bool,
        should_timeout: bool,
        should_error: bool,
    }

    impl TestPlugin {
        fn new(name: &'static str, events: Vec<EventHookType>) -> Self {
            Self {
                name,
                events,
                call_count: Arc::new(AtomicUsize::new(0)),
                should_panic: false,
                should_timeout: false,
                should_error: false,
            }
        }

        fn with_panic(mut self) -> Self {
            self.should_panic = true;
            self
        }

        fn with_timeout(mut self) -> Self {
            self.should_timeout = true;
            self
        }

        fn with_error(mut self) -> Self {
            self.should_error = true;
            self
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn load(&self, _config: &PluginConfig) -> adapteros_core::Result<()> {
            Ok(())
        }

        async fn start(&self) -> adapteros_core::Result<()> {
            Ok(())
        }

        async fn stop(&self) -> adapteros_core::Result<()> {
            Ok(())
        }

        async fn reload(&self, _config: &PluginConfig) -> adapteros_core::Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> adapteros_core::Result<PluginHealth> {
            Ok(PluginHealth {
                status: PluginStatus::Started,
                details: None,
            })
        }

        fn subscribed_events(&self) -> Vec<EventHookType> {
            self.events.clone()
        }

        async fn on_event(&self, _event: &PluginEvent) -> adapteros_core::Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            if self.should_panic {
                panic!("Test panic");
            }

            if self.should_timeout {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }

            if self.should_error {
                return Err(AosError::Other("Test error".to_string()));
            }

            Ok(())
        }

        async fn set_tenant_enabled(
            &self,
            _tenant_id: &str,
            _enabled: bool,
        ) -> adapteros_core::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_event_bus_basic_dispatch() {
        let bus = EventBus::new(10);

        let plugin = TestPlugin::new("test-plugin", vec![EventHookType::OnAdapterRegistered]);
        let plugin = Arc::new(plugin);

        bus.register_plugin("test-plugin".to_string(), plugin.clone())
            .await;

        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        assert!(result.is_ok());
        assert_eq!(plugin.call_count(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_panic_isolation() {
        let bus = EventBus::new(10);

        let plugin =
            TestPlugin::new("panic-plugin", vec![EventHookType::OnAdapterRegistered]).with_panic();
        let plugin = Arc::new(plugin);

        bus.register_plugin("panic-plugin".to_string(), plugin.clone())
            .await;

        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        // Should return error but not crash
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_timeout_enforcement() {
        let bus = EventBus::new(10);

        let plugin = TestPlugin::new("timeout-plugin", vec![EventHookType::OnAdapterRegistered])
            .with_timeout();
        let plugin = Arc::new(plugin);

        bus.register_plugin("timeout-plugin".to_string(), plugin.clone())
            .await;

        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        // Should timeout and return error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_error_handling() {
        let bus = EventBus::new(10);

        let plugin =
            TestPlugin::new("error-plugin", vec![EventHookType::OnAdapterRegistered]).with_error();
        let plugin = Arc::new(plugin);

        bus.register_plugin("error-plugin".to_string(), plugin.clone())
            .await;

        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(10);

        let plugin1 = TestPlugin::new("plugin1", vec![EventHookType::OnAdapterRegistered]);
        let plugin2 = TestPlugin::new("plugin2", vec![EventHookType::OnAdapterRegistered]);

        let plugin1 = Arc::new(plugin1);
        let plugin2 = Arc::new(plugin2);

        bus.register_plugin("plugin1".to_string(), plugin1.clone())
            .await;
        bus.register_plugin("plugin2".to_string(), plugin2.clone())
            .await;

        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        assert!(result.is_ok());
        assert_eq!(plugin1.call_count(), 1);
        assert_eq!(plugin2.call_count(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_selective_subscription() {
        let bus = EventBus::new(10);

        let plugin1 = TestPlugin::new("plugin1", vec![EventHookType::OnAdapterRegistered]);
        let plugin2 = TestPlugin::new("plugin2", vec![EventHookType::OnAdapterLoaded]);

        let plugin1 = Arc::new(plugin1);
        let plugin2 = Arc::new(plugin2);

        bus.register_plugin("plugin1".to_string(), plugin1.clone())
            .await;
        bus.register_plugin("plugin2".to_string(), plugin2.clone())
            .await;

        // Emit OnAdapterRegistered event
        let event = PluginEvent::AdapterRegistered(AdapterEvent {
            adapter_id: "test".to_string(),
            action: "registered".to_string(),
            hash: None,
            tier: None,
            rank: None,
            tenant_id: None,
            lifecycle_state: None,
            timestamp: "2025-11-25T12:00:00Z".to_string(),
            metadata: HashMap::new(),
        });

        let result = bus.emit(event).await;
        assert!(result.is_ok());

        // Only plugin1 should have been called
        assert_eq!(plugin1.call_count(), 1);
        assert_eq!(plugin2.call_count(), 0);
    }
}
