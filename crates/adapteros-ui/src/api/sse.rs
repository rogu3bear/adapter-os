//! Server-Sent Events (SSE) client
//!
//! Provides reactive SSE connections with circuit breaker pattern
//! and automatic reconnection.

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{EventSource, MessageEvent};

use super::api_base_url;

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
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
            reset_timeout_ms: 60000,
        }
    }
}

/// SSE connection with circuit breaker and auto-reconnection
pub struct SseConnection {
    endpoint: String,
    state: RwSignal<SseState>,
    failure_count: Rc<RefCell<u32>>,
    event_source: Rc<RefCell<Option<EventSource>>>,
    config: CircuitBreakerConfig,
    #[allow(clippy::type_complexity)]
    closures: Rc<RefCell<Vec<Closure<dyn FnMut(MessageEvent)>>>>,
}

impl SseConnection {
    /// Create a new SSE connection
    pub fn new(endpoint: &str) -> Self {
        Self::with_config(endpoint, CircuitBreakerConfig::default())
    }

    /// Create with custom circuit breaker configuration
    pub fn with_config(endpoint: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            state: RwSignal::new(SseState::Disconnected),
            failure_count: Rc::new(RefCell::new(0)),
            event_source: Rc::new(RefCell::new(None)),
            config,
            closures: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Get current connection state as a signal
    pub fn state(&self) -> RwSignal<SseState> {
        self.state
    }

    /// Get the full URL for the SSE endpoint
    fn full_url(&self) -> String {
        format!("{}{}", api_base_url(), self.endpoint)
    }

    /// Connect and start receiving events
    pub fn connect<F>(&self, on_event: F) -> Result<(), crate::api::ApiError>
    where
        F: Fn(SseEvent) + Clone + 'static,
    {
        // Check circuit breaker
        if self.state.get() == SseState::CircuitOpen {
            return Err(crate::api::ApiError::Network(
                "Circuit breaker open".to_string(),
            ));
        }

        // Clean up any existing connection and closures before reconnecting
        // This prevents closure accumulation on reconnect cycles
        self.disconnect();

        self.state.set(SseState::Connecting);

        let url = self.full_url();
        let event_source = EventSource::new(&url).map_err(|e| {
            crate::api::ApiError::Network(format!("Failed to create EventSource: {:?}", e))
        })?;

        let state = self.state;
        let failure_count = self.failure_count.clone();

        // Handle open event
        let on_open = Closure::wrap(Box::new(move || {
            state.set(SseState::Connected);
            *failure_count.borrow_mut() = 0;
        }) as Box<dyn FnMut()>);
        event_source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // Handle error event
        let state_err = self.state;
        let failure_count_err = self.failure_count.clone();
        let threshold = self.config.failure_threshold;
        let on_error = Closure::wrap(Box::new(move || {
            let mut count = failure_count_err.borrow_mut();
            *count += 1;
            if *count >= threshold {
                state_err.set(SseState::CircuitOpen);
            } else {
                state_err.set(SseState::Error);
            }
        }) as Box<dyn FnMut()>);
        event_source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        // Handle message events (default event type)
        let on_event_clone = on_event.clone();
        let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            let last_event_id = event.last_event_id();
            let last_event_id = if last_event_id.is_empty() {
                None
            } else {
                Some(last_event_id)
            };

            on_event_clone(SseEvent {
                event_type: "message".to_string(),
                data,
                last_event_id,
            });
        }) as Box<dyn FnMut(MessageEvent)>);
        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Store closures to prevent dropping
        self.closures.borrow_mut().push(on_message);

        // Store the EventSource
        *self.event_source.borrow_mut() = Some(event_source);

        Ok(())
    }

    /// Subscribe to a specific event type
    pub fn subscribe<F>(&self, event_type: &str, on_event: F)
    where
        F: Fn(SseEvent) + 'static,
    {
        if let Some(es) = self.event_source.borrow().as_ref() {
            let event_type_owned = event_type.to_string();
            let callback = Closure::wrap(Box::new(move |event: MessageEvent| {
                let data = event.data().as_string().unwrap_or_default();
                let last_event_id = event.last_event_id();
                let last_event_id = if last_event_id.is_empty() {
                    None
                } else {
                    Some(last_event_id)
                };

                on_event(SseEvent {
                    event_type: event_type_owned.clone(),
                    data,
                    last_event_id,
                });
            }) as Box<dyn FnMut(MessageEvent)>);

            es.add_event_listener_with_callback(event_type, callback.as_ref().unchecked_ref())
                .ok();

            self.closures.borrow_mut().push(callback);
        }
    }

    /// Disconnect from SSE stream
    pub fn disconnect(&self) {
        if let Some(es) = self.event_source.borrow_mut().take() {
            es.close();
        }
        self.closures.borrow_mut().clear();
        self.state.set(SseState::Disconnected);
    }

    /// Reset circuit breaker
    pub fn reset_circuit(&self) {
        *self.failure_count.borrow_mut() = 0;
        self.state.set(SseState::Disconnected);
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state.get() == SseState::Connected
    }
}

impl Drop for SseConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Hook to use an SSE connection with automatic lifecycle management
pub fn use_sse<F>(endpoint: &str, on_event: F) -> (RwSignal<SseState>, impl Fn())
where
    F: Fn(SseEvent) + Clone + 'static,
{
    let connection = SseConnection::new(endpoint);
    let state = connection.state();
    let on_event_clone = on_event.clone();

    // Connect on mount
    Effect::new(move || {
        let _ = connection.connect(on_event_clone.clone());
    });

    // Create reconnect function - note: creates new connection for reconnect
    // The connect() method now calls disconnect() first, preventing closure accumulation
    let connection_reconnect = SseConnection::new(endpoint);
    let reconnect = move || {
        connection_reconnect.reset_circuit();
        let _ = connection_reconnect.connect(on_event.clone());
    };

    (state, reconnect)
}

/// Hook to use SSE with parsed JSON events
pub fn use_sse_json<T, F>(endpoint: &str, on_event: F) -> (RwSignal<SseState>, impl Fn())
where
    T: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(T) + Clone + 'static,
{
    use_sse(endpoint, move |event| {
        if let Ok(parsed) = serde_json::from_str::<T>(&event.data) {
            on_event(parsed);
        }
    })
}
