//! In-flight adapters context
//!
//! Polls the backend to track which adapters are currently being used
//! for inference. Components can check this to show "In Use" badges
//! and disable modification controls.

use crate::api::ApiClient;
use crate::hooks::use_polling;
use leptos::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

/// Polling interval for in-flight status (5 seconds)
const POLL_INTERVAL_MS: u32 = 5000;

/// Context providing in-flight adapter tracking
#[derive(Clone)]
pub struct InFlightContext {
    /// Set of adapter IDs currently in use
    pub adapter_ids: Signal<HashSet<String>>,
    /// Count of active inferences
    pub inference_count: Signal<usize>,
}

impl InFlightContext {
    /// Check if a specific adapter is in-flight
    pub fn is_in_flight(&self, adapter_id: &str) -> bool {
        self.adapter_ids.get().contains(adapter_id)
    }
}

/// Hook to access in-flight context.
///
/// # Panics
///
/// Panics if called outside of an `InFlightProvider`. Use `try_use_in_flight()`
/// for fallible access that returns `None` if the context is unavailable.
pub fn use_in_flight() -> InFlightContext {
    expect_context::<InFlightContext>()
}

/// Try to access in-flight context without panicking.
///
/// Returns `None` if called outside of an `InFlightProvider`.
/// Useful for components that may be rendered in contexts where
/// the provider is not guaranteed to exist.
pub fn try_use_in_flight() -> Option<InFlightContext> {
    use_context::<InFlightContext>()
}

/// Provider component that polls in-flight status
#[component]
pub fn InFlightProvider(children: Children) -> impl IntoView {
    let adapter_ids = RwSignal::new(HashSet::<String>::new());
    let inference_count = RwSignal::new(0usize);

    let client = Arc::new(ApiClient::new());

    // Use the existing use_polling hook which handles cleanup properly
    let _cancel = use_polling(POLL_INTERVAL_MS, {
        let client = Arc::clone(&client);
        move || {
            let client = Arc::clone(&client);
            async move {
                if let Ok(response) = client.get_in_flight_adapters().await {
                    adapter_ids.set(response.adapter_ids.into_iter().collect());
                    inference_count.set(response.inference_count);
                }
            }
        }
    });

    let context = InFlightContext {
        adapter_ids: adapter_ids.into(),
        inference_count: inference_count.into(),
    };

    provide_context(context);

    children()
}
