//! Custom hooks for common patterns
//!
//! Leptos-style hooks for data fetching, state management, etc.

pub mod use_sse_notifications;

pub use use_sse_notifications::use_sse_notifications;

use crate::api::{ApiClient, ApiError, ApiResult};
use leptos::prelude::*;
use std::sync::Arc;

/// Resource loading state
#[derive(Debug, Clone)]
pub enum LoadingState<T> {
    /// Not started
    Idle,
    /// Loading
    Loading,
    /// Loaded with data
    Loaded(T),
    /// Error occurred
    Error(ApiError),
}

impl<T> LoadingState<T> {
    /// Check if loading
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// Get data if loaded
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Loaded(data) => Some(data),
            _ => None,
        }
    }

    /// Get error if any
    pub fn error(&self) -> Option<&ApiError> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Create a resource that fetches data from the API
pub fn use_api_resource<T, F, Fut>(fetch: F) -> (ReadSignal<LoadingState<T>>, impl Fn())
where
    T: Clone + Send + Sync + 'static,
    F: Fn(Arc<ApiClient>) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = ApiResult<T>> + 'static,
{
    let (state, set_state) = signal(LoadingState::<T>::Idle);
    let client = Arc::new(ApiClient::new());

    let fetch_clone = fetch.clone();
    let client_clone = Arc::clone(&client);
    let refetch = move || {
        let client = Arc::clone(&client_clone);
        let fetch = fetch_clone.clone();

        set_state.set(LoadingState::Loading);

        wasm_bindgen_futures::spawn_local(async move {
            match fetch(client).await {
                Ok(data) => set_state.set(LoadingState::Loaded(data)),
                Err(e) => set_state.set(LoadingState::Error(e)),
            }
        });
    };

    // Initial fetch
    let refetch_init = refetch.clone();
    Effect::new(move || {
        refetch_init();
    });

    (state, refetch)
}

/// Simple polling hook with automatic cleanup
///
/// Returns a cancel function that stops the polling when called.
/// The interval is automatically cleared when the component unmounts.
///
/// # Implementation Note
/// Uses raw `setInterval`/`clearInterval` via web_sys to enable proper cleanup.
/// The JS closure is leaked (unavoidable with WASM), but the interval itself
/// is cleared on unmount, preventing continued execution.
pub fn use_polling<F, Fut>(interval_ms: u32, fetch: F) -> impl Fn()
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // Store interval ID for cleanup (-1 = no interval)
    let interval_id = Arc::new(AtomicI32::new(-1));
    let interval_id_for_cleanup = Arc::clone(&interval_id);
    let interval_id_for_cancel = Arc::clone(&interval_id);

    // Register cleanup to clear interval on unmount
    on_cleanup(move || {
        let id = interval_id_for_cleanup.load(Ordering::SeqCst);
        if id >= 0 {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(id);
            }
        }
    });

    Effect::new(move || {
        let fetch = fetch.clone();
        let interval_id = Arc::clone(&interval_id);

        // Initial fetch
        let fetch_init = fetch.clone();
        wasm_bindgen_futures::spawn_local(async move {
            fetch_init().await;
        });

        // Set up interval using web_sys for cleanup capability
        let callback = Closure::wrap(Box::new(move || {
            let fetch = fetch.clone();
            wasm_bindgen_futures::spawn_local(async move {
                fetch().await;
            });
        }) as Box<dyn FnMut()>);

        if let Some(window) = web_sys::window() {
            if let Ok(id) = window.set_interval_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                interval_ms as i32,
            ) {
                interval_id.store(id, Ordering::SeqCst);
            }
        }

        // Closure must be leaked (WASM limitation), but interval is cleared on cleanup
        callback.forget();
    });

    // Return cancel function
    move || {
        let id = interval_id_for_cancel.swap(-1, Ordering::SeqCst);
        if id >= 0 {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(id);
            }
        }
    }
}

/// Navigate hook
pub fn use_navigate() -> impl Fn(&str) {
    let navigate = leptos_router::hooks::use_navigate();
    move |path: &str| {
        navigate(path, Default::default());
    }
}

/// Get an API client for making requests
pub fn use_api() -> Arc<ApiClient> {
    Arc::new(ApiClient::new())
}
