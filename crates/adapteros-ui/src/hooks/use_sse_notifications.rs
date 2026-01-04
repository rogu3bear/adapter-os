//! SSE notification bridge hook
//!
//! Bridges SSE connection state changes to the notification system.
//! Provides user-friendly notifications for connection state transitions
//! with debouncing to prevent notification spam.

use crate::api::SseState;
use crate::signals::notifications::{use_notifications, ToastSeverity};
use leptos::prelude::*;

/// Debounce window in milliseconds to prevent notification spam
const DEBOUNCE_MS: u32 = 500;

/// State transition tracking for the SSE notification bridge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransitionState {
    /// Previous SSE state
    previous: SseState,
    /// Whether the circuit breaker was active in this cycle
    circuit_breaker_active: bool,
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            previous: SseState::Disconnected,
            circuit_breaker_active: false,
        }
    }
}

/// Result of processing an SSE state transition
#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionResult {
    /// Notification to show, if any: (severity, title, message)
    notification: Option<(ToastSeverity, &'static str, &'static str)>,
    /// New transition state
    new_state: TransitionState,
}

/// Process an SSE state transition and determine if notification is needed.
/// Pure function for testability.
fn process_transition(
    previous: SseState,
    current: SseState,
    circuit_breaker_active: bool,
) -> TransitionResult {
    match (previous, current) {
        // Connected -> Error: Connection lost
        (SseState::Connected, SseState::Error) => TransitionResult {
            notification: Some((
                ToastSeverity::Warning,
                "Connection Issue",
                "Connection lost, retrying...",
            )),
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active: false,
            },
        },

        // Error -> CircuitOpen: Circuit breaker activated
        (SseState::Error, SseState::CircuitOpen) => TransitionResult {
            notification: Some((
                ToastSeverity::Error,
                "Connection Failed",
                "Circuit breaker activated. Connection will retry automatically.",
            )),
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active: true,
            },
        },

        // CircuitOpen -> Connected: Connection restored (only if circuit was active)
        (SseState::CircuitOpen, SseState::Connected) => TransitionResult {
            notification: if circuit_breaker_active {
                Some((
                    ToastSeverity::Success,
                    "Connection Restored",
                    "Real-time updates are now active.",
                ))
            } else {
                None
            },
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active: false,
            },
        },

        // Any -> Connected: Reset circuit breaker flag
        (_, SseState::Connected) => TransitionResult {
            notification: None,
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active: false,
            },
        },

        // Any -> CircuitOpen: Mark circuit breaker as active
        (_, SseState::CircuitOpen) => TransitionResult {
            notification: if previous != SseState::Error {
                // Direct transition to circuit open (shouldn't normally happen)
                Some((
                    ToastSeverity::Error,
                    "Connection Failed",
                    "Circuit breaker activated. Connection will retry automatically.",
                ))
            } else {
                None
            },
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active: true,
            },
        },

        // Other transitions: just update state
        _ => TransitionResult {
            notification: None,
            new_state: TransitionState {
                previous: current,
                circuit_breaker_active,
            },
        },
    }
}

/// Hook that bridges SSE connection state to the notification system.
///
/// Watches for SSE state transitions and shows appropriate notifications:
/// - `Connected -> Error`: Warning "Connection lost, retrying..."
/// - `Error -> CircuitOpen`: Error "Circuit breaker activated"
/// - `CircuitOpen -> Connected`: Success "Connection restored" (only after circuit was active)
///
/// Implements a 500ms debounce to prevent notification spam during rapid state changes.
///
/// # Arguments
///
/// * `sse_state` - A reactive signal containing the current SSE connection state
///
/// # Example
///
/// ```rust,ignore
/// use leptos::prelude::*;
/// use adapteros_ui::api::{use_sse, SseState};
/// use adapteros_ui::hooks::use_sse_notifications;
///
/// #[component]
/// fn MyComponent() -> impl IntoView {
///     let (sse_state, _reconnect) = use_sse("/api/v1/events", |_event| {});
///
///     // Bridge SSE state changes to notifications
///     use_sse_notifications(sse_state.read_only());
///
///     view! { <div>"My component"</div> }
/// }
/// ```
pub fn use_sse_notifications(sse_state: ReadSignal<SseState>) {
    // Track transition state across effect runs
    let transition_state = RwSignal::new(TransitionState::default());

    // Track last notification time for debouncing
    let last_notification_time = StoredValue::new(0.0_f64);

    // Get notifications context
    let notifications = use_notifications();

    // Effect that watches SSE state changes
    Effect::new(move || {
        let current = sse_state.get();
        let state = transition_state.get();
        let previous = state.previous;

        // Skip if state hasn't changed
        if current == previous {
            return;
        }

        // Check debounce window
        let now = current_time_ms();
        let last = last_notification_time.get_value();
        if now - last < f64::from(DEBOUNCE_MS) {
            // Still update the tracked state even if we skip the notification
            transition_state.set(TransitionState {
                previous: current,
                circuit_breaker_active: state.circuit_breaker_active
                    || current == SseState::CircuitOpen,
            });
            return;
        }

        // Process state transition using pure function
        let result = process_transition(previous, current, state.circuit_breaker_active);

        // Show notification if needed
        if let Some((severity, title, message)) = result.notification {
            last_notification_time.set_value(now);
            notifications.show(severity, title, message);
        }

        // Update transition state
        transition_state.set(result.new_state);
    });
}

/// Get current time in milliseconds
fn current_time_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transition_state_default() {
        let state = TransitionState::default();
        assert_eq!(state.previous, SseState::Disconnected);
        assert!(!state.circuit_breaker_active);
    }

    #[test]
    fn test_connected_to_error_shows_warning() {
        let result = process_transition(SseState::Connected, SseState::Error, false);

        assert_eq!(
            result.notification,
            Some((
                ToastSeverity::Warning,
                "Connection Issue",
                "Connection lost, retrying..."
            ))
        );
        assert_eq!(result.new_state.previous, SseState::Error);
        assert!(!result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_error_to_circuit_open_shows_error() {
        let result = process_transition(SseState::Error, SseState::CircuitOpen, false);

        assert_eq!(
            result.notification,
            Some((
                ToastSeverity::Error,
                "Connection Failed",
                "Circuit breaker activated. Connection will retry automatically."
            ))
        );
        assert_eq!(result.new_state.previous, SseState::CircuitOpen);
        assert!(result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_circuit_open_to_connected_with_active_circuit_shows_success() {
        let result = process_transition(SseState::CircuitOpen, SseState::Connected, true);

        assert_eq!(
            result.notification,
            Some((
                ToastSeverity::Success,
                "Connection Restored",
                "Real-time updates are now active."
            ))
        );
        assert_eq!(result.new_state.previous, SseState::Connected);
        assert!(!result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_circuit_open_to_connected_without_active_circuit_no_notification() {
        let result = process_transition(SseState::CircuitOpen, SseState::Connected, false);

        assert!(result.notification.is_none());
        assert_eq!(result.new_state.previous, SseState::Connected);
        assert!(!result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_disconnected_to_connected_no_notification() {
        let result = process_transition(SseState::Disconnected, SseState::Connected, false);

        assert!(result.notification.is_none());
        assert_eq!(result.new_state.previous, SseState::Connected);
        assert!(!result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_direct_to_circuit_open_shows_error() {
        // Direct transition from Disconnected to CircuitOpen (unusual but possible)
        let result = process_transition(SseState::Disconnected, SseState::CircuitOpen, false);

        assert_eq!(
            result.notification,
            Some((
                ToastSeverity::Error,
                "Connection Failed",
                "Circuit breaker activated. Connection will retry automatically."
            ))
        );
        assert!(result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_error_to_error_preserves_circuit_state() {
        // Same state transition - shouldn't happen but test edge case
        let result = process_transition(SseState::Error, SseState::Error, true);

        assert!(result.notification.is_none());
        assert_eq!(result.new_state.previous, SseState::Error);
        assert!(result.new_state.circuit_breaker_active); // Preserved
    }

    #[test]
    fn test_connecting_to_connected_no_notification() {
        let result = process_transition(SseState::Connecting, SseState::Connected, false);

        assert!(result.notification.is_none());
        assert_eq!(result.new_state.previous, SseState::Connected);
        assert!(!result.new_state.circuit_breaker_active);
    }

    #[test]
    fn test_current_time_ms_returns_positive() {
        let time = current_time_ms();
        // Non-WASM should return actual system time in milliseconds
        // WASM would return js_sys::Date::now()
        // Both should be positive (unless system clock is wrong)
        assert!(time >= 0.0);
    }
}
