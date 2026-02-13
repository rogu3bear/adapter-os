//! Server-Sent Events (SSE) client
//!
//! Provides reactive SSE connections with circuit breaker pattern
//! and automatic reconnection.

use crate::api::api_base_url;
#[cfg(target_arch = "wasm32")]
use crate::api::ApiClient;
use gloo_timers::callback::{Interval, Timeout};
use js_sys::Date;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{EventSource, EventSourceInit, MessageEvent};

type MessageClosure = Closure<dyn FnMut(MessageEvent)>;
type MessageClosureList = Rc<RefCell<Vec<MessageClosure>>>;
type EventClosure = Closure<dyn FnMut(web_sys::Event)>;
type EventClosureList = Rc<RefCell<Vec<EventClosure>>>;
type SubscriptionList = Rc<RefCell<Vec<(String, MessageClosure)>>>;
type SseHandler = Rc<RefCell<Option<Rc<dyn Fn(SseEvent)>>>>;

/// SSE connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseState {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and receiving events
    Connected,
    /// Connection error, will retry
    Error,
    /// Circuit breaker open (too many failures)
    CircuitOpen,
}

/// SSE event received from server
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (e.g., "message", "heartbeat", "system_status")
    pub event_type: String,
    /// Event data (usually JSON)
    pub data: String,
    /// Last event ID for resumption
    pub last_event_id: Option<String>,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Time to wait before attempting reconnection (ms)
    pub retry_delay_ms: u32,
    /// Maximum retry delay (exponential backoff cap)
    pub max_retry_delay_ms: u32,
    /// Time after which circuit resets to half-open
    pub reset_timeout_ms: u32,
    /// Idle timeout for no events before reconnect (ms)
    pub idle_timeout_ms: Option<u32>,
    /// Whether to include credentials (cookies) on cross-origin SSE
    pub with_credentials: bool,
    /// Optional query param for auth (key, value)
    pub auth_query_param: Option<(String, String)>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
            reset_timeout_ms: 60000,
            // Keep connections resilient to slow or intermittent proxies by avoiding
            // aggressive idle-driven reconnects; explicit onerror/reconnect handles liveness.
            idle_timeout_ms: None,
            // SSE uses cookies for auth; default to sending credentials for same-origin
            with_credentials: true,
            auth_query_param: None,
        }
    }
}

impl CircuitBreakerConfig {
    /// Enable or disable sending credentials on cross-origin SSE.
    pub fn with_credentials(mut self, enabled: bool) -> Self {
        self.with_credentials = enabled;
        self
    }

    /// Set an idle timeout (ms) after which the connection is recycled.
    pub fn with_idle_timeout_ms(mut self, idle_timeout_ms: Option<u32>) -> Self {
        self.idle_timeout_ms = idle_timeout_ms;
        self
    }

    /// Attach a query parameter for auth (key/value).
    pub fn with_auth_query_param(mut self, key: &str, value: &str) -> Self {
        self.auth_query_param = Some((key.to_string(), value.to_string()));
        self
    }
}

#[derive(Clone)]
struct SseContext {
    endpoint: String,
    state: RwSignal<SseState>,
    failure_count: Rc<RefCell<u32>>,
    event_source: Rc<RefCell<Option<EventSource>>>,
    config: CircuitBreakerConfig,
    message_closures: MessageClosureList,
    event_closures: EventClosureList,
    subscriptions: SubscriptionList,
    last_event_at: RwSignal<Option<f64>>,
    reconnect_timeout: Rc<RefCell<Option<Timeout>>>,
    watchdog_handle: Rc<RefCell<Option<Interval>>>,
    handler: SseHandler,
}

/// SSE connection with circuit breaker and auto-reconnection
pub struct SseConnection {
    ctx: SseContext,
}

impl SseConnection {
    /// Create a new SSE connection
    pub fn new(endpoint: &str) -> Self {
        Self::with_config(endpoint, CircuitBreakerConfig::default())
    }

    /// Create with custom circuit breaker configuration
    pub fn with_config(endpoint: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            ctx: SseContext {
                endpoint: endpoint.to_string(),
                state: RwSignal::new(SseState::Disconnected),
                failure_count: Rc::new(RefCell::new(0)),
                event_source: Rc::new(RefCell::new(None)),
                config,
                message_closures: Rc::new(RefCell::new(Vec::new())),
                event_closures: Rc::new(RefCell::new(Vec::new())),
                subscriptions: Rc::new(RefCell::new(Vec::new())),
                last_event_at: RwSignal::new(None),
                reconnect_timeout: Rc::new(RefCell::new(None)),
                watchdog_handle: Rc::new(RefCell::new(None)),
                handler: Rc::new(RefCell::new(None)),
            },
        }
    }

    /// Get current connection state as a signal
    pub fn state(&self) -> RwSignal<SseState> {
        self.ctx.state
    }

    /// Get the last event timestamp (ms since epoch)
    pub fn last_event_at(&self) -> ReadSignal<Option<f64>> {
        self.ctx.last_event_at.read_only()
    }

    /// Connect and start receiving events
    pub fn connect<F>(&self, on_event: F) -> Result<(), crate::api::ApiError>
    where
        F: Fn(SseEvent) + Clone + 'static,
    {
        let handler: Rc<dyn Fn(SseEvent)> = Rc::new(on_event);
        *self.ctx.handler.borrow_mut() = Some(handler.clone());
        connect_with_handler(self.ctx.clone(), handler)
    }

    /// Connect and subscribe to specific event types.
    pub fn connect_with_event_types<F>(
        &self,
        event_types: &[&str],
        on_event: F,
    ) -> Result<(), crate::api::ApiError>
    where
        F: Fn(SseEvent) + Clone + 'static,
    {
        for event_type in event_types {
            if self
                .ctx
                .subscriptions
                .borrow()
                .iter()
                .any(|(existing, _)| existing == event_type)
            {
                continue;
            }
            self.subscribe(event_type, on_event.clone());
        }
        self.connect(on_event)
    }

    /// Subscribe to a specific event type
    pub fn subscribe<F>(&self, event_type: &str, on_event: F)
    where
        F: Fn(SseEvent) + 'static,
    {
        let event_type_owned = event_type.to_string();
        let event_type_for_event = event_type_owned.clone();
        let last_event_at = self.ctx.last_event_at;
        let callback = Closure::wrap(Box::new(move |event: MessageEvent| {
            // Use try_set to avoid panic if signal is disposed during navigation
            let _ = last_event_at.try_set(Some(Date::now()));
            let data = event.data().as_string().unwrap_or_default();
            let last_event_id = event.last_event_id();
            let last_event_id = if last_event_id.is_empty() {
                None
            } else {
                Some(last_event_id)
            };

            on_event(SseEvent {
                event_type: event_type_for_event.clone(),
                data,
                last_event_id,
            });
        }) as Box<dyn FnMut(MessageEvent)>);

        let registered = if let Some(es) = self.ctx.event_source.borrow().as_ref() {
            match es.add_event_listener_with_callback(event_type, callback.as_ref().unchecked_ref())
            {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("Failed to add event listener for '{}': {:?}", event_type, e);
                    false
                }
            }
        } else {
            // No active EventSource, store for later attachment
            true
        };

        if registered {
            self.ctx
                .subscriptions
                .borrow_mut()
                .push((event_type_owned, callback));
        }
    }

    /// Disconnect from SSE stream
    pub fn disconnect(&self) {
        disconnect_inner(&self.ctx, true);
    }

    /// Reset circuit breaker
    pub fn reset_circuit(&self) {
        disconnect_inner(&self.ctx, true);
        *self.ctx.failure_count.borrow_mut() = 0;
        // Use try_set to avoid panic if signal is disposed during navigation
        let _ = self.ctx.last_event_at.try_set(None);
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.ctx.state.try_get_untracked() == Some(SseState::Connected)
    }
}

impl Drop for SseConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn build_full_url(endpoint: &str, config: &CircuitBreakerConfig) -> String {
    let mut url = format!("{}{}", api_base_url(), endpoint);
    if let Some((ref key, ref value)) = config.auth_query_param {
        let sep = if url.contains('?') { "&" } else { "?" };
        let key = encode_component(key);
        let value = encode_component(value);
        url.push_str(sep);
        url.push_str(&key);
        url.push('=');
        url.push_str(&value);
    }
    url
}

fn encode_component(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_else(|| value.to_string())
}

fn create_event_source(
    url: &str,
    config: &CircuitBreakerConfig,
) -> Result<EventSource, crate::api::ApiError> {
    let init = EventSourceInit::new();
    if config.with_credentials {
        init.set_with_credentials(true);
    }
    EventSource::new_with_event_source_init_dict(url, &init).map_err(|e| {
        crate::api::ApiError::Network(format!("Failed to create EventSource: {:?}", e))
    })
}

fn connect_with_handler(
    ctx: SseContext,
    handler: Rc<dyn Fn(SseEvent)>,
) -> Result<(), crate::api::ApiError> {
    if ctx.state.try_get_untracked() == Some(SseState::CircuitOpen) {
        let _ = ctx.state.try_set(SseState::CircuitOpen);
        return Err(crate::api::ApiError::Network(
            "Circuit breaker open".to_string(),
        ));
    }

    disconnect_inner(&ctx, false);

    let _ = ctx.state.try_set(SseState::Connecting);

    let url = build_full_url(&ctx.endpoint, &ctx.config);
    let event_source = match create_event_source(&url, &ctx.config) {
        Ok(source) => source,
        Err(err) => {
            handle_failure(ctx, "eventsource init failed");
            return Err(err);
        }
    };

    let ctx_for_open = ctx.clone();
    let on_open = Closure::wrap(Box::new(move |_evt: web_sys::Event| {
        let _ = ctx_for_open.state.try_set(SseState::Connected);
        reset_failures(&ctx_for_open);
        clear_reconnect(&ctx_for_open);
        // Use try_set to avoid panic if signal is disposed during navigation
        let _ = ctx_for_open.last_event_at.try_set(Some(Date::now()));
    }) as Box<dyn FnMut(web_sys::Event)>);
    event_source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    ctx.event_closures.borrow_mut().push(on_open);

    let ctx_for_error = ctx.clone();
    let handler_for_error = handler.clone();
    let on_error = Closure::wrap(Box::new(move |evt: web_sys::Event| {
        // Some SSE streams emit application-level errors using `event: error`.
        // In browsers, that can dispatch as an EventSource "error" event even though the
        // transport is healthy. If we treat those as failures, we can create reconnect loops
        // and trip the circuit breaker spuriously.
        if let Some(msg) = evt.dyn_ref::<MessageEvent>() {
            // If an explicit "error" subscription exists, let it handle the MessageEvent and
            // skip transport failure handling. Otherwise, route it through the generic handler.
            if ctx_for_error
                .subscriptions
                .borrow()
                .iter()
                .any(|(t, _)| t == "error")
            {
                return;
            }

            let _ = ctx_for_error.last_event_at.try_set(Some(Date::now()));
            let data = msg.data().as_string().unwrap_or_default();
            let last_event_id = msg.last_event_id();
            let last_event_id = if last_event_id.is_empty() {
                None
            } else {
                Some(last_event_id)
            };

            handler_for_error(SseEvent {
                event_type: "error".to_string(),
                data,
                last_event_id,
            });
            return;
        }

        handle_failure(ctx_for_error.clone(), "eventsource error");
    }) as Box<dyn FnMut(web_sys::Event)>);
    event_source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    ctx.event_closures.borrow_mut().push(on_error);

    let ctx_for_message = ctx.clone();
    let handler_clone = handler.clone();
    let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        // Use try_set to avoid panic if signal is disposed during navigation
        let _ = ctx_for_message.last_event_at.try_set(Some(Date::now()));
        let data = event.data().as_string().unwrap_or_default();
        let last_event_id = event.last_event_id();
        let last_event_id = if last_event_id.is_empty() {
            None
        } else {
            Some(last_event_id)
        };

        handler_clone(SseEvent {
            event_type: "message".to_string(),
            data,
            last_event_id,
        });
    }) as Box<dyn FnMut(MessageEvent)>);
    event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    ctx.message_closures.borrow_mut().push(on_message);

    *ctx.event_source.borrow_mut() = Some(event_source);
    if let Some(es) = ctx.event_source.borrow().as_ref() {
        attach_subscriptions(&ctx, es);
    }
    start_watchdog(ctx);

    Ok(())
}

fn disconnect_inner(ctx: &SseContext, update_state: bool) {
    clear_reconnect(ctx);
    clear_watchdog(ctx);

    let event_source = ctx.event_source.borrow_mut().take();
    ctx.message_closures.borrow_mut().clear();
    ctx.event_closures.borrow_mut().clear();

    if let Some(es) = event_source {
        for (event_type, callback) in ctx.subscriptions.borrow().iter() {
            let _ = es
                .remove_event_listener_with_callback(event_type, callback.as_ref().unchecked_ref());
        }
        es.set_onmessage(None);
        es.set_onopen(None);
        es.set_onerror(None);
        es.close();
    }

    if update_state {
        let _ = ctx.state.try_set(SseState::Disconnected);
    }
}

fn attach_subscriptions(ctx: &SseContext, es: &EventSource) {
    for (event_type, callback) in ctx.subscriptions.borrow().iter() {
        if let Err(e) =
            es.add_event_listener_with_callback(event_type, callback.as_ref().unchecked_ref())
        {
            tracing::warn!(
                "Failed to reattach subscription for event type '{}': {:?}",
                event_type,
                e
            );
        }
    }
}

fn handle_failure(ctx: SseContext, reason: &str) {
    disconnect_inner(&ctx, false);

    let failures = increment_failures(&ctx);
    let threshold = ctx.config.failure_threshold.max(1);
    if failures >= threshold {
        let _ = ctx.state.try_set(SseState::CircuitOpen);
        tracing::warn!(
            "SSE circuit open for {} after {} failures: {}",
            ctx.endpoint,
            failures,
            reason
        );
        probe_auth_and_stop_on_unauthorized(&ctx);
        schedule_circuit_reset(ctx);
        return;
    }

    let _ = ctx.state.try_set(SseState::Error);
    tracing::warn!(
        "SSE error for {} (attempt {}): {}",
        ctx.endpoint,
        failures,
        reason
    );
    schedule_reconnect(ctx, failures);
}

fn increment_failures(ctx: &SseContext) -> u32 {
    let mut count = ctx.failure_count.borrow_mut();
    *count += 1;
    *count
}

fn reset_failures(ctx: &SseContext) {
    *ctx.failure_count.borrow_mut() = 0;
}

#[cfg(target_arch = "wasm32")]
fn probe_auth_and_stop_on_unauthorized(ctx: &SseContext) {
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let client = Arc::new(ApiClient::new());
        if let Err(crate::api::ApiError::Unauthorized) = client.me().await {
            clear_reconnect(&ctx);
            clear_watchdog(&ctx);
            let _ = ctx.state.try_set(SseState::CircuitOpen);
            tracing::warn!(
                "SSE halted for {} due to auth expiry; awaiting re-auth",
                ctx.endpoint
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn probe_auth_and_stop_on_unauthorized(_ctx: &SseContext) {}

fn schedule_reconnect(ctx: SseContext, failures: u32) {
    clear_reconnect(&ctx);

    let delay_ms = compute_backoff_ms(
        ctx.config.retry_delay_ms,
        ctx.config.max_retry_delay_ms,
        failures,
    );
    if delay_ms == 0 {
        return;
    }

    let handler = ctx.handler.borrow().clone();
    let Some(handler) = handler else {
        return;
    };

    let ctx_for_timeout = ctx.clone();
    let timeout = Timeout::new(delay_ms, move || {
        let _ = ctx_for_timeout.state.try_set(SseState::Connecting);
        let _ = connect_with_handler(ctx_for_timeout.clone(), handler.clone());
    });

    *ctx.reconnect_timeout.borrow_mut() = Some(timeout);
}

fn schedule_circuit_reset(ctx: SseContext) {
    clear_reconnect(&ctx);
    let delay_ms = ctx.config.reset_timeout_ms;
    if delay_ms == 0 {
        return;
    }

    let handler = ctx.handler.borrow().clone();
    let ctx_for_timeout = ctx.clone();
    let timeout = Timeout::new(delay_ms, move || {
        reset_failures(&ctx_for_timeout);
        let _ = ctx_for_timeout.state.try_set(SseState::Disconnected);
        if let Some(handler) = handler.clone() {
            let _ = connect_with_handler(ctx_for_timeout.clone(), handler);
        }
    });

    *ctx.reconnect_timeout.borrow_mut() = Some(timeout);
}

fn clear_reconnect(ctx: &SseContext) {
    ctx.reconnect_timeout.borrow_mut().take();
}

fn clear_watchdog(ctx: &SseContext) {
    ctx.watchdog_handle.borrow_mut().take();
}

fn start_watchdog(ctx: SseContext) {
    let Some(idle_timeout_ms) = ctx.config.idle_timeout_ms else {
        return;
    };

    clear_watchdog(&ctx);
    let interval_ms = (idle_timeout_ms / 2).max(1000);
    let ctx_for_watchdog = ctx.clone();
    let handle = Interval::new(interval_ms, move || {
        // Use try_ variants to avoid panic if signals are disposed during navigation
        if ctx_for_watchdog.state.try_get_untracked() != Some(SseState::Connected) {
            return;
        }

        if let Some(last_event_at) = ctx_for_watchdog.last_event_at.try_get_untracked().flatten() {
            let elapsed = Date::now() - last_event_at;
            if elapsed.is_sign_negative() {
                return;
            }
            if elapsed >= idle_timeout_ms as f64 {
                handle_failure(ctx_for_watchdog.clone(), "idle timeout");
            }
        }
    });

    *ctx.watchdog_handle.borrow_mut() = Some(handle);
}

fn compute_backoff_ms(base_ms: u32, max_ms: u32, failures: u32) -> u32 {
    if base_ms == 0 {
        return 0;
    }
    let exp = failures.saturating_sub(1).min(16);
    let delay = (base_ms as u64).saturating_mul(1u64 << exp);
    let capped = std::cmp::min(delay, max_ms as u64);
    capped as u32
}

/// Hook to use an SSE connection with automatic lifecycle management
pub fn use_sse<F>(endpoint: &str, on_event: F) -> (RwSignal<SseState>, impl Fn())
where
    F: Fn(SseEvent) + Clone + 'static,
{
    use_sse_with_config(endpoint, CircuitBreakerConfig::default(), on_event)
}

/// Hook to use SSE with custom configuration
pub fn use_sse_with_config<F>(
    endpoint: &str,
    config: CircuitBreakerConfig,
    on_event: F,
) -> (RwSignal<SseState>, impl Fn())
where
    F: Fn(SseEvent) + Clone + 'static,
{
    let endpoint_name = endpoint.to_string();
    let connection = Rc::new(SseConnection::with_config(endpoint, config));
    let state = connection.state();
    let on_event_clone = on_event.clone();

    // Connect on mount
    let connection_for_effect = Rc::clone(&connection);
    let endpoint_name_for_effect = endpoint_name.clone();
    Effect::new(move || {
        if let Err(err) = connection_for_effect.connect(on_event_clone.clone()) {
            tracing::warn!(
                "SSE connect failed for {}: {}",
                endpoint_name_for_effect,
                err
            );
        }
    });

    // Cleanup on unmount
    let connection_for_cleanup = SendWrapper::new(Rc::clone(&connection));
    on_cleanup(move || {
        connection_for_cleanup.disconnect();
    });

    // Reconnect function (same connection, reset circuit)
    let connection_reconnect = Rc::clone(&connection);
    let on_event_reconnect = on_event.clone();
    let endpoint_name_reconnect = endpoint_name.clone();
    let reconnect = move || {
        connection_reconnect.reset_circuit();
        if let Err(err) = connection_reconnect.connect(on_event_reconnect.clone()) {
            tracing::warn!(
                "SSE reconnect failed for {}: {}",
                endpoint_name_reconnect,
                err
            );
        }
    };

    (state, reconnect)
}

/// Hook to use SSE with parsed JSON events
pub fn use_sse_json<T, F>(endpoint: &str, on_event: F) -> (RwSignal<SseState>, impl Fn())
where
    T: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(T) + Clone + 'static,
{
    use_sse_json_with_config(endpoint, CircuitBreakerConfig::default(), on_event)
}

/// Hook to use SSE with parsed JSON events for specific event types
pub fn use_sse_json_events<T, F>(
    endpoint: &str,
    event_types: &[&str],
    on_event: F,
) -> (RwSignal<SseState>, impl Fn())
where
    T: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(T) + Clone + 'static,
{
    let endpoint_name = endpoint.to_string();
    let event_types: Vec<String> = event_types
        .iter()
        .map(|event| (*event).to_string())
        .collect();
    let connection = Rc::new(SseConnection::with_config(
        endpoint,
        CircuitBreakerConfig::default(),
    ));
    let connection_weak: Weak<SseConnection> = Rc::downgrade(&connection);
    let state = connection.state();
    let parse_failures = Rc::new(std::cell::Cell::new(0u32));

    fn make_parsing_handler<T, F>(
        endpoint_name: String,
        on_event: F,
        parse_failures: Rc<std::cell::Cell<u32>>,
        connection_weak: Weak<SseConnection>,
        allowed_event_types: Vec<String>,
    ) -> impl Fn(SseEvent) + Clone
    where
        T: for<'de> serde::Deserialize<'de> + 'static,
        F: Fn(T) + Clone + 'static,
    {
        let allowed_event_types = std::rc::Rc::new(allowed_event_types);
        move |event: SseEvent| {
            let Some(_) = allowed_event_types
                .iter()
                .find(|event_type| event_type.as_str() == event.event_type)
            else {
                return;
            };

            let data = event.data.trim();
            if data.is_empty() || data == "[DONE]" {
                return;
            }

            match serde_json::from_str::<T>(data) {
                Ok(parsed) => {
                    parse_failures.set(0);
                    on_event(parsed);
                }
                Err(err) => {
                    let failures = parse_failures.get().saturating_add(1);
                    parse_failures.set(failures);
                    let preview: String = data.chars().take(200).collect();
                    tracing::warn!(
                        "SSE JSON parse failed for {}: {} (payload: {})",
                        endpoint_name,
                        err,
                        preview
                    );
                    if failures >= 3 {
                        tracing::warn!(
                            "SSE JSON parse failed {} times for {}; reconnecting",
                            failures,
                            endpoint_name
                        );
                        parse_failures.set(0);
                        if let Some(conn) = connection_weak.upgrade() {
                            conn.reset_circuit();
                            let handler = make_parsing_handler::<T, F>(
                                endpoint_name.clone(),
                                on_event.clone(),
                                Rc::clone(&parse_failures),
                                connection_weak.clone(),
                                allowed_event_types.iter().cloned().collect(),
                            );
                            let event_type_refs: Vec<&str> = allowed_event_types
                                .iter()
                                .map(|event_type| event_type.as_str())
                                .collect();
                            if let Err(err) =
                                conn.connect_with_event_types(&event_type_refs, handler.clone())
                            {
                                tracing::warn!(
                                    "SSE reconnect failed for {}: {}",
                                    endpoint_name,
                                    err
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    let handler = make_parsing_handler::<T, F>(
        endpoint_name.clone(),
        on_event,
        Rc::clone(&parse_failures),
        connection_weak.clone(),
        event_types.clone(),
    );

    // Connect on mount
    let connection_for_effect = Rc::clone(&connection);
    let endpoint_name_for_effect = endpoint_name.clone();
    let event_types_for_effect = event_types.clone();
    let handler_for_effect = handler.clone();
    Effect::new(move || {
        let event_type_refs: Vec<&str> = event_types_for_effect
            .iter()
            .map(|event| event.as_str())
            .collect();
        if let Err(err) = connection_for_effect
            .connect_with_event_types(&event_type_refs, handler_for_effect.clone())
        {
            tracing::warn!(
                "SSE connect failed for {}: {}",
                endpoint_name_for_effect,
                err
            );
        }
    });

    // Cleanup on unmount
    let connection_for_cleanup = SendWrapper::new(Rc::clone(&connection));
    on_cleanup(move || {
        connection_for_cleanup.disconnect();
    });

    // Reconnect function (same connection, reset circuit)
    let connection_reconnect = Rc::clone(&connection);
    let handler_reconnect = handler.clone();
    let event_types_for_reconnect = event_types.clone();
    let endpoint_name_reconnect = endpoint_name.clone();
    let reconnect = move || {
        connection_reconnect.reset_circuit();
        let event_type_refs: Vec<&str> = event_types_for_reconnect
            .iter()
            .map(|event| event.as_str())
            .collect();
        if let Err(err) = connection_reconnect.connect_with_event_types(
            &event_type_refs,
            handler_reconnect.clone(),
        )
        {
            tracing::warn!(
                "SSE reconnect failed for {}: {}",
                endpoint_name_reconnect,
                err
            );
        }
    };

    (state, reconnect)
}

/// Hook to use SSE with parsed JSON events and custom configuration
pub fn use_sse_json_with_config<T, F>(
    endpoint: &str,
    config: CircuitBreakerConfig,
    on_event: F,
) -> (RwSignal<SseState>, impl Fn())
where
    T: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(T) + Clone + 'static,
{
    let endpoint_name = endpoint.to_string();
    use_sse_with_config(endpoint, config, move |event| {
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }

        match serde_json::from_str::<T>(data) {
            Ok(parsed) => on_event(parsed),
            Err(err) => {
                let preview: String = data.chars().take(200).collect();
                tracing::warn!(
                    "SSE JSON parse failed for {}: {} (payload: {})",
                    endpoint_name,
                    err,
                    preview
                );
            }
        }
    })
}

// =============================================================================
// Streaming Inference with Lifecycle Events Support
// =============================================================================

/// Streaming lifecycle event types
#[derive(Debug, Clone)]
pub enum StreamLifecycleEvent {
    /// Stream has started - contains stream_id and optional idempotency_key
    Started {
        stream_id: String,
        request_id: String,
        idempotency_key: Option<String>,
    },
    /// Stream has finished - contains summary information
    Finished {
        stream_id: String,
        request_id: String,
        total_tokens: usize,
        duration_ms: u64,
        finish_reason: Option<String>,
    },
}

/// Callback type for stream lifecycle events
pub type OnStreamLifecycle = Box<dyn Fn(StreamLifecycleEvent)>;

/// Callback type for token events (receives token string)
pub type OnTokenCallback = Box<dyn Fn(String)>;

/// Callback type for done events (receives finish reason)
pub type OnDoneCallback = Box<dyn Fn(String)>;

/// Callback type for error events (receives error message and recoverable flag)
pub type OnErrorCallback = Box<dyn Fn(String, bool)>;

/// Streaming inference event handler with lifecycle awareness
///
/// This handler processes SSE events from the inference stream and:
/// 1. Detects stream_started events for recovery tracking
/// 2. Accumulates tokens for incremental rendering
/// 3. Detects stream_finished events for completion confirmation
/// 4. Handles reconnection with idempotency key
pub struct StreamingInferenceHandler {
    /// Current stream ID (set on stream_started)
    stream_id: Rc<RefCell<Option<String>>>,
    /// Current request ID
    request_id: Rc<RefCell<Option<String>>>,
    /// Idempotency key for recovery
    idempotency_key: Rc<RefCell<Option<String>>>,
    /// Token callback
    on_token: Rc<RefCell<Option<OnTokenCallback>>>,
    /// Lifecycle callback
    on_lifecycle: Rc<RefCell<Option<OnStreamLifecycle>>>,
    /// Done callback (finish_reason)
    on_done: Rc<RefCell<Option<OnDoneCallback>>>,
    /// Error callback
    on_error: Rc<RefCell<Option<OnErrorCallback>>>,
}

impl StreamingInferenceHandler {
    /// Create a new streaming inference handler
    pub fn new() -> Self {
        Self {
            stream_id: Rc::new(RefCell::new(None)),
            request_id: Rc::new(RefCell::new(None)),
            idempotency_key: Rc::new(RefCell::new(None)),
            on_token: Rc::new(RefCell::new(None)),
            on_lifecycle: Rc::new(RefCell::new(None)),
            on_done: Rc::new(RefCell::new(None)),
            on_error: Rc::new(RefCell::new(None)),
        }
    }

    /// Set token callback
    pub fn on_token<F>(self, callback: F) -> Self
    where
        F: Fn(String) + 'static,
    {
        *self.on_token.borrow_mut() = Some(Box::new(callback));
        self
    }

    /// Set lifecycle callback
    pub fn on_lifecycle<F>(self, callback: F) -> Self
    where
        F: Fn(StreamLifecycleEvent) + 'static,
    {
        *self.on_lifecycle.borrow_mut() = Some(Box::new(callback));
        self
    }

    /// Set done callback
    pub fn on_done<F>(self, callback: F) -> Self
    where
        F: Fn(String) + 'static,
    {
        *self.on_done.borrow_mut() = Some(Box::new(callback));
        self
    }

    /// Set error callback (message, retryable)
    pub fn on_error<F>(self, callback: F) -> Self
    where
        F: Fn(String, bool) + 'static,
    {
        *self.on_error.borrow_mut() = Some(Box::new(callback));
        self
    }

    /// Get the current stream ID (if stream has started)
    pub fn stream_id(&self) -> Option<String> {
        self.stream_id.borrow().clone()
    }

    /// Get the current idempotency key (for recovery)
    pub fn idempotency_key(&self) -> Option<String> {
        self.idempotency_key.borrow().clone()
    }

    /// Process an SSE event from the inference stream
    pub fn handle_event(&self, event: SseEvent) {
        let data = event.data.trim();

        // Handle stream_started lifecycle event
        if event.event_type == "stream_started" {
            if let Ok(evt) = serde_json::from_str::<adapteros_api_types::StreamStartedEvent>(data) {
                *self.stream_id.borrow_mut() = Some(evt.stream_id.clone());
                *self.request_id.borrow_mut() = Some(evt.request_id.clone());
                *self.idempotency_key.borrow_mut() = evt.idempotency_key.clone();

                if let Some(ref callback) = *self.on_lifecycle.borrow() {
                    callback(StreamLifecycleEvent::Started {
                        stream_id: evt.stream_id,
                        request_id: evt.request_id,
                        idempotency_key: evt.idempotency_key,
                    });
                }
            }
            return;
        }

        // Handle stream_finished lifecycle event
        if event.event_type == "stream_finished" {
            if let Ok(evt) = serde_json::from_str::<adapteros_api_types::StreamFinishedEvent>(data)
            {
                if let Some(ref callback) = *self.on_lifecycle.borrow() {
                    callback(StreamLifecycleEvent::Finished {
                        stream_id: evt.stream_id,
                        request_id: evt.request_id,
                        total_tokens: evt.total_tokens,
                        duration_ms: evt.duration_ms,
                        finish_reason: evt.finish_reason,
                    });
                }
            }
            return;
        }

        // Handle error event
        if event.event_type == "error" {
            if let Ok(evt) = serde_json::from_str::<adapteros_api_types::StreamErrorEvent>(data) {
                let message = if evt.message.is_empty() {
                    "Unknown error".to_string()
                } else {
                    evt.message
                };
                if let Some(ref callback) = *self.on_error.borrow() {
                    callback(message, evt.retryable);
                }
            }
            return;
        }

        // Handle standard SSE messages (OpenAI-compatible format)
        if data.is_empty() || data == "[DONE]" {
            return;
        }

        // Parse OpenAI-compatible streaming chunk
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
            // Extract token content from delta
            if let Some(choices) = parsed.get("choices").and_then(|v| v.as_array()) {
                for choice in choices {
                    // Check for finish_reason (done event)
                    if let Some(finish_reason) =
                        choice.get("finish_reason").and_then(|v| v.as_str())
                    {
                        if !finish_reason.is_empty() && finish_reason != "null" {
                            if let Some(ref callback) = *self.on_done.borrow() {
                                callback(finish_reason.to_string());
                            }
                        }
                    }

                    // Extract content token
                    if let Some(content) = choice
                        .get("delta")
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            if let Some(ref callback) = *self.on_token.borrow() {
                                callback(content.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for StreamingInferenceHandler {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Lifecycle SSE Subscriptions
//
// These hooks connect to the backend lifecycle SSE streams and dispatch
// incoming events to the refetch system so that UI components automatically
// refresh when adapters, training jobs, or system health change.
// =============================================================================

/// Subscribe to adapter lifecycle events via SSE.
///
/// Connects to `/v1/stream/adapters` and dispatches incoming
/// [`AdapterLifecycleEvent`] / [`AdapterVersionEvent`] to the refetch system.
///
/// Returns `(sse_state, reconnect_fn)`.
pub fn use_adapter_lifecycle_sse() -> (RwSignal<SseState>, impl Fn()) {
    use crate::api::types::{AdapterLifecycleEvent, AdapterVersionEvent};
    use crate::signals::refetch::use_refetch;

    let refetch = use_refetch();

    use_sse("/v1/stream/adapters", move |event: SseEvent| {
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }

        // Try adapter lifecycle event first
        if let Ok(parsed) = serde_json::from_str::<AdapterLifecycleEvent>(data) {
            refetch.dispatch_adapter_event(&parsed);
            return;
        }

        // Try adapter version event
        if let Ok(parsed) = serde_json::from_str::<AdapterVersionEvent>(data) {
            refetch.dispatch_adapter_version_event(&parsed);
        }
    })
}

/// Subscribe to training lifecycle events via SSE.
///
/// Connects to `/v1/streams/training` and dispatches incoming
/// [`TrainingLifecycleEvent`] to the refetch system.
///
/// Returns `(sse_state, reconnect_fn)`.
pub fn use_training_lifecycle_sse() -> (RwSignal<SseState>, impl Fn()) {
    use crate::api::types::TrainingLifecycleEvent;
    use crate::signals::refetch::use_refetch;

    let refetch = use_refetch();

    use_sse("/v1/streams/training", move |event: SseEvent| {
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }

        if let Ok(parsed) = serde_json::from_str::<TrainingLifecycleEvent>(data) {
            refetch.dispatch_training_event(&parsed);
        }
    })
}

/// Subscribe to system health transition events via SSE.
///
/// Connects to `/v1/stream/telemetry` and dispatches incoming
/// [`SystemHealthTransitionEvent`] to the refetch system.
///
/// Returns `(sse_state, reconnect_fn)`.
pub fn use_health_lifecycle_sse() -> (RwSignal<SseState>, impl Fn()) {
    use crate::api::types::SystemHealthTransitionEvent;
    use crate::signals::refetch::use_refetch;

    let refetch = use_refetch();

    use_sse("/v1/stream/telemetry", move |event: SseEvent| {
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }

        if let Ok(parsed) = serde_json::from_str::<SystemHealthTransitionEvent>(data) {
            refetch.dispatch_health_event(&parsed);
        }
    })
}
