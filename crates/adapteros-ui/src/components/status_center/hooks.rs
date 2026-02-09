//! Status Center hooks
//!
//! Custom hooks for keyboard shortcuts and data fetching.

use crate::api::{ApiClient, ApiError};
use adapteros_api_types::{SystemStateResponse, SystemStatusResponse};
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Counter signal that increments when a keyboard shortcut is triggered
///
/// # Arguments
/// * `key` - The key to listen for (e.g., "s", "Escape")
/// * `ctrl` - Whether Ctrl key must be pressed
/// * `shift` - Whether Shift key must be pressed
///
/// # Returns
/// A read signal that increments each time the shortcut is pressed
///
/// # Implementation Note
/// Uses an atomic flag to disable the handler on component unmount.
/// The closure is leaked (WASM limitation), but becomes a no-op after cleanup.
pub fn use_keyboard_shortcut(key: &'static str, ctrl: bool, shift: bool) -> ReadSignal<u32> {
    let (count, set_count) = signal(0u32);

    // Track if the component is still mounted (Send+Sync for on_cleanup)
    let is_active = Arc::new(AtomicBool::new(true));
    let is_active_for_cleanup = Arc::clone(&is_active);

    // Track if we've already registered the listener
    let registered = Arc::new(AtomicBool::new(false));

    // Register cleanup to disable the handler on unmount
    on_cleanup(move || {
        is_active_for_cleanup.store(false, Ordering::SeqCst);
    });

    Effect::new(move || {
        // Only register once - prevent re-registration on Effect re-run
        if registered.swap(true, Ordering::SeqCst) {
            return;
        }

        let Some(window) = web_sys::window() else {
            tracing::error!("use_keyboard_shortcut: no window object");
            return;
        };
        let Some(document) = window.document() else {
            tracing::error!("use_keyboard_shortcut: no document object");
            return;
        };

        let is_active = Arc::clone(&is_active);
        let closure =
            Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
                // Check if component is still mounted before handling
                if !is_active.load(Ordering::SeqCst) {
                    return;
                }

                let key_matches = event.key().to_lowercase() == key.to_lowercase();
                let ctrl_matches = !ctrl || event.ctrl_key() || event.meta_key();
                let shift_matches = !shift || event.shift_key();

                if key_matches && ctrl_matches && shift_matches {
                    event.prevent_default();
                    set_count.update(|c| *c = c.wrapping_add(1));
                }
            });

        if let Err(e) =
            document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
        {
            tracing::error!(
                "use_keyboard_shortcut: failed to add keydown listener: {:?}",
                e
            );
            return;
        }

        // Closure must be leaked (WASM limitation), but handler becomes no-op after cleanup
        closure.forget();
    });

    count
}

/// Combined status data from both endpoints
#[derive(Debug, Clone)]
pub struct CombinedStatus {
    /// System status from /v1/system/status
    pub status: SystemStatusResponse,
    /// System state from /v1/system/state
    pub state: SystemStateResponse,
}

/// Loading state for status data
#[derive(Debug, Clone)]
pub enum StatusLoadingState {
    /// Initial state, not yet loaded
    Idle,
    /// Currently loading
    Loading,
    /// Successfully loaded (boxed to reduce enum size)
    Loaded(Box<CombinedStatus>),
    /// Error occurred
    Error(ApiError),
}

impl StatusLoadingState {
    /// Check if currently loading
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Get the loaded data if available
    pub fn data(&self) -> Option<&CombinedStatus> {
        match self {
            Self::Loaded(data) => Some(data),
            _ => None,
        }
    }

    /// Get the error if any
    pub fn error(&self) -> Option<&ApiError> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Hook for fetching combined status data
///
/// Returns a tuple of:
/// - Read signal with current loading state
/// - Refetch callback to trigger a new fetch
pub fn use_status_data() -> (ReadSignal<StatusLoadingState>, impl Fn() + Clone) {
    let (state, set_state) = signal(StatusLoadingState::Idle);
    let client = Arc::new(ApiClient::new());

    let client_clone = Arc::clone(&client);
    let refetch = move || {
        let client = Arc::clone(&client_clone);
        let _ = set_state.try_set(StatusLoadingState::Loading);

        // Defer spawn_local via Timeout to avoid RefCell re-entrancy panic
        // in wasm-bindgen-futures when called from within a reactive Effect body.
        gloo_timers::callback::Timeout::new(0, move || {
            wasm_bindgen_futures::spawn_local(async move {
                // Fetch both endpoints concurrently
                let status_future = client.system_status();
                let state_future = fetch_system_state(&client);

                match futures::future::join(status_future, state_future).await {
                    (Ok(status), Ok(state)) => {
                        let _ = set_state.try_set(StatusLoadingState::Loaded(Box::new(CombinedStatus {
                            status,
                            state,
                        })));
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        let _ = set_state.try_set(StatusLoadingState::Error(e));
                    }
                }
            });
        })
        .forget();
    };

    // Initial fetch
    let refetch_init = refetch.clone();
    Effect::new(move || {
        refetch_init();
    });

    (state, refetch)
}

/// Fetch system state from /v1/system/state
async fn fetch_system_state(client: &ApiClient) -> Result<SystemStateResponse, ApiError> {
    client.get("/v1/system/state").await
}

/// Hook for detecting Escape key press
pub fn use_escape_key() -> ReadSignal<u32> {
    use_keyboard_shortcut("Escape", false, false)
}

/// Hook for detecting Ctrl+Shift+S shortcut
pub fn use_status_center_shortcut() -> ReadSignal<u32> {
    use_keyboard_shortcut("s", true, true)
}
