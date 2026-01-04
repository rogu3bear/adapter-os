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

/// Simple polling hook
///
/// # Resource Leak Note
/// The interval handle is intentionally leaked via `std::mem::forget` because:
/// 1. Leptos 0.7's `on_cleanup` requires `Send + Sync` closures
/// 2. `gloo_timers::Interval` is `!Send` (uses browser APIs)
/// 3. In WASM, page unload or component unmount will release resources anyway
///
/// For long-lived SPAs with frequent component remounts, consider using
/// `AbortController` patterns or a global polling manager instead.
pub fn use_polling<F, Fut>(interval_ms: u32, fetch: F)
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    Effect::new(move || {
        let fetch = fetch.clone();

        // Initial fetch
        let fetch_init = fetch.clone();
        wasm_bindgen_futures::spawn_local(async move {
            fetch_init().await;
        });

        // Set up interval
        let interval_handle = gloo_timers::callback::Interval::new(interval_ms, move || {
            let fetch = fetch.clone();
            wasm_bindgen_futures::spawn_local(async move {
                fetch().await;
            });
        });

        // Keep interval alive - see doc comment for rationale
        std::mem::forget(interval_handle);
    });
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
