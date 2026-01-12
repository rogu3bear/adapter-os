//! Custom hooks for common patterns
//!
//! Leptos-style hooks for data fetching, state management, etc.

pub mod use_sse_notifications;

pub use use_sse_notifications::use_sse_notifications;

use crate::api::{report_error, ApiClient, ApiError, ApiResult};
use leptos::prelude::*;
use std::sync::Arc;

/// Get the current page path for error reporting
fn get_current_path() -> Option<String> {
    web_sys::window().and_then(|w| w.location().pathname().ok())
}

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
///
/// Automatically reports API errors to the server for persistent logging.
pub fn use_api_resource<T, F, Fut>(fetch: F) -> (ReadSignal<LoadingState<T>>, impl Fn())
where
    T: Clone + Send + Sync + 'static,
    F: Fn(Arc<ApiClient>) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = ApiResult<T>> + 'static,
{
    let (state, set_state) = signal(LoadingState::<T>::Idle);
    let client = Arc::new(ApiClient::new());
    let is_authenticated = client.is_authenticated();
    let fetch_version = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let fetch_clone = fetch.clone();
    let client_clone = Arc::clone(&client);
    let fetch_version_clone = Arc::clone(&fetch_version);
    let refetch = move || {
        let client = Arc::clone(&client_clone);
        let fetch = fetch_clone.clone();
        let fetch_version = Arc::clone(&fetch_version_clone);
        let version = fetch_version.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        set_state.set(LoadingState::Loading);

        wasm_bindgen_futures::spawn_local(async move {
            match fetch(client).await {
                Ok(data) => {
                    // Ignore stale responses from earlier refetches
                    if fetch_version.load(std::sync::atomic::Ordering::SeqCst) == version {
                        set_state.set(LoadingState::Loaded(data));
                    }
                }
                Err(e) => {
                    if e.is_aborted() {
                        return;
                    }
                    if fetch_version.load(std::sync::atomic::Ordering::SeqCst) == version {
                        // Report error to server (fire-and-forget)
                        let page = get_current_path();
                        report_error(&e, page.as_deref(), is_authenticated);

                        set_state.set(LoadingState::Error(e));
                    }
                }
            }
        });
    };

    // Initial fetch - use untracked to avoid reactive re-runs causing RefCell re-entrancy
    // The caller controls re-fetching via the returned refetch function
    let refetch_init = refetch.clone();
    Effect::new(move || {
        // Run untracked to prevent this effect from re-running on signal changes
        // inside the fetch closure (e.g., route params). This avoids synchronous
        // re-entrancy in wasm-bindgen-futures task queue.
        untrack(|| {
            refetch_init();
        });
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
                // Skip polling when tab is hidden to avoid needless load
                let should_skip = web_sys::window()
                    .and_then(|w| w.document())
                    .map(|d| d.hidden())
                    .unwrap_or(false)
                    || web_sys::window()
                        .map(|w| !w.navigator().on_line())
                        .unwrap_or(false);
                if should_skip {
                    return;
                }
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
