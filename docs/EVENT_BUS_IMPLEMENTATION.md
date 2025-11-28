# EventBus Implementation Summary

**Date:** 2025-11-25
**Status:** ✅ Implemented
**PRD Reference:** PRD-PLUG-01

## Overview

Implemented a fault-tolerant plugin event bus for dispatching events to registered plugins with panic isolation, timeout enforcement, and error handling.

## Files Created/Modified

### Created Files

1. **`/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/event_bus.rs`**
   - Core EventBus implementation
   - Panic isolation using `std::panic::AssertUnwindSafe` and `futures_util::FutureExt::catch_unwind`
   - 5-second timeout per plugin event handler
   - Automatic subscription management based on plugin's `subscribed_events()`
   - Broadcast channel for event distribution
   - Comprehensive test suite (6 tests)

### Modified Files

1. **`/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/state.rs`**
   - Added `event_bus: Option<Arc<EventBus>>` field to `AppState`
   - Added `with_event_bus()` builder method

2. **`/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/lib.rs`**
   - Added `pub mod event_bus;` module declaration
   - Added `pub use event_bus::EventBus;` export

## Implementation Details

### EventBus Structure

```rust
pub struct EventBus {
    registry: Arc<RwLock<HashMap<String, Arc<dyn Plugin>>>>,
    subscriptions: Arc<RwLock<HashMap<EventHookType, Vec<String>>>>,
    event_tx: broadcast::Sender<PluginEvent>,
}
```

### Key Methods

1. **`new(capacity: usize)`** - Create event bus with broadcast channel capacity
2. **`register_plugin(&self, name: String, plugin: Arc<dyn Plugin>)`** - Register plugin and auto-subscribe to events
3. **`emit(&self, event: PluginEvent)`** - Dispatch event to all subscribed plugins with timeout and panic isolation
4. **`spawn_dispatcher(&self)`** - Spawn background task for long-lived event processing

### Fault Tolerance Features

#### 1. Panic Isolation
```rust
use futures_util::FutureExt;

let result = AssertUnwindSafe(plugin.on_event(&event))
    .catch_unwind()
    .await;
```

- Plugin panics are caught and logged
- Server continues running even if plugin crashes
- Panic messages extracted and reported

#### 2. Timeout Enforcement
```rust
tokio::time::timeout(
    Duration::from_secs(5),
    Self::isolated_dispatch(plugin, event_clone, plugin_name.clone()),
)
.await
```

- 5-second timeout per plugin handler
- Timeouts logged as errors
- Plugin marked as failed on timeout

#### 3. Error Logging
```rust
error!(
    plugin = %plugin_name,
    event_type = ?event_type,
    error = %e,
    "Plugin handler returned error"
);
```

- Structured logging with `tracing` macros
- All errors logged but don't stop event dispatch
- Failed plugins returned as list for monitoring

### Event Flow

1. **Register Phase:**
   - Plugin registers with `event_bus.register_plugin(name, plugin)`
   - EventBus calls `plugin.subscribed_events()` to get event types
   - Subscriptions stored in `subscriptions` map

2. **Emit Phase:**
   - Handler calls `event_bus.emit(event)`
   - EventBus maps event to `EventHookType`
   - Looks up subscribed plugins for that event type
   - Dispatches to each plugin with timeout and panic isolation
   - Broadcasts event on channel (non-blocking)

3. **Dispatch Phase (per plugin):**
   - Wrap `plugin.on_event(&event)` in `AssertUnwindSafe`
   - Apply 5-second timeout
   - Catch panics with `catch_unwind()`
   - Log errors but continue to next plugin

### Test Coverage

Implemented 6 comprehensive tests:

1. **`test_event_bus_basic_dispatch`** - Verify basic event delivery
2. **`test_event_bus_panic_isolation`** - Verify panic catching works
3. **`test_event_bus_timeout_enforcement`** - Verify 5-second timeout
4. **`test_event_bus_error_handling`** - Verify error propagation
5. **`test_event_bus_multiple_subscribers`** - Verify multi-plugin dispatch
6. **`test_event_bus_selective_subscription`** - Verify event filtering

### Integration with AppState

```rust
// Create event bus
let event_bus = Arc::new(EventBus::new(1000));

// Register plugins
event_bus.register_plugin("my-plugin".to_string(), plugin).await;

// Attach to AppState
let state = AppState::new(...)
    .with_event_bus(event_bus);

// Emit events from handlers
if let Some(ref bus) = state.event_bus {
    let event = PluginEvent::AdapterRegistered(...);
    bus.emit(event).await?;
}
```

## Usage Example

```rust
use adapteros_server_api::EventBus;
use adapteros_core::{PluginEvent, AdapterEvent};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create event bus with capacity for 1000 events
    let event_bus = Arc::new(EventBus::new(1000));

    // Register plugins
    let plugin: Arc<dyn Plugin> = Arc::new(MyPlugin::new());
    event_bus.register_plugin("my-plugin".to_string(), plugin).await;

    // Emit events
    let event = PluginEvent::AdapterRegistered(AdapterEvent {
        adapter_id: "my-adapter".to_string(),
        action: "registered".to_string(),
        hash: None,
        tier: None,
        rank: None,
        tenant_id: None,
        lifecycle_state: None,
        timestamp: "2025-11-25T12:00:00Z".to_string(),
        metadata: HashMap::new(),
    });

    // Dispatch to all subscribed plugins
    event_bus.emit(event).await?;

    Ok(())
}
```

## Event Types Supported

From `adapteros_core::EventHookType`:

1. **OnTrainingJobEvent** - Training job status changes
2. **OnAdapterRegistered** - Adapter registration
3. **OnAdapterLoaded** - Adapter loaded into memory
4. **OnAdapterUnloaded** - Adapter unloaded from memory
5. **OnAuditEvent** - Audit events
6. **OnMetricsTick** - Periodic metrics collection
7. **OnInferenceComplete** - Inference completion
8. **OnPolicyViolation** - Policy violations

## Error Handling

The EventBus never crashes the server:

- **Plugin panics:** Caught and logged, server continues
- **Plugin timeouts:** Logged as errors, server continues
- **Plugin errors:** Logged and returned in failure list
- **Broadcast failures:** Logged as warnings (no receivers)

```rust
let result = event_bus.emit(event).await;
match result {
    Ok(()) => println!("All plugins handled event successfully"),
    Err(failures) => println!("Plugins that failed: {:?}", failures),
}
```

## Performance Characteristics

- **Registration:** O(1) HashMap insert + O(k) subscription updates (k = event types)
- **Emission:** O(n) where n = number of subscribed plugins for event type
- **Dispatch:** Parallel timeout futures (5s max per plugin)
- **Memory:** Broadcast channel capacity + subscription maps

## Compliance with CLAUDE.md Standards

✅ **Code Style:** PascalCase types, snake_case functions, SCREAMING_SNAKE_CASE constants
✅ **Documentation:** Comprehensive rustdoc with examples
✅ **Error Handling:** Uses `Result<(), Vec<String>>` for error reporting
✅ **Logging:** Uses `tracing` macros (info!, warn!, error!, debug!)
✅ **Testing:** 6 unit tests with async runtime
✅ **No duplication:** Extracted `isolated_dispatch` helper method

## Next Steps

To complete plugin event dispatch integration:

1. **Emit events from handlers:**
   - Adapter registration → `PluginEvent::AdapterRegistered`
   - Adapter load/unload → `PluginEvent::AdapterLoaded/Unloaded`
   - Training job updates → `PluginEvent::TrainingJob`
   - Audit events → `PluginEvent::Audit`
   - Inference completion → `PluginEvent::InferenceComplete`
   - Policy violations → `PluginEvent::PolicyViolation`

2. **Initialize EventBus in main.rs:**
   ```rust
   let event_bus = Arc::new(EventBus::new(1000));
   let state = AppState::new(...)
       .with_event_bus(event_bus.clone());

   // Start background dispatcher
   event_bus.spawn_dispatcher();
   ```

3. **Register plugins on startup:**
   ```rust
   for (name, plugin) in load_plugins()? {
       event_bus.register_plugin(name, plugin).await;
   }
   ```

## References

- **PRD:** PRD-PLUG-01 (Plugin Event Hooks)
- **Core Types:** `crates/adapteros-core/src/plugins.rs`
- **Event Payloads:** `crates/adapteros-core/src/plugin_events.rs`
- **Implementation:** `crates/adapteros-server-api/src/event_bus.rs`

---

**Implementation Author:** Claude (Sonnet 4.5)
**Review Status:** Ready for integration
**Compilation Status:** ✅ Compiles successfully (no errors in event_bus.rs)
