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
pub fn use_api_resource<T, F, Fut>(fetch: F) -> (ReadSignal<LoadingState<T>>, Callback<()>)
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

        // Defer ENTIRE refetch to next microtask to avoid RefCell re-entrancy
        // when called from click handlers. Moving spawn_local inside the timeout
        // ensures the click handler's borrow is released before any signal mutations.
        #[cfg(target_arch = "wasm32")]
        {
            let set_state_loading = set_state.clone();
            let set_state_result = set_state.clone();
            let fetch_version_check = Arc::clone(&fetch_version);
            gloo_timers::callback::Timeout::new(0, move || {
                let _ = set_state_loading.try_set(LoadingState::Loading);

                wasm_bindgen_futures::spawn_local(async move {
                    match fetch(client).await {
                        Ok(data) => {
                            if fetch_version_check.load(std::sync::atomic::Ordering::SeqCst)
                                == version
                            {
                                let _ = set_state_result.try_set(LoadingState::Loaded(data));
                            }
                        }
                        Err(e) => {
                            if e.is_aborted() {
                                return;
                            }
                            if fetch_version_check.load(std::sync::atomic::Ordering::SeqCst)
                                == version
                            {
                                let page = get_current_path();
                                report_error(&e, page.as_deref(), is_authenticated);
                                let _ = set_state_result.try_set(LoadingState::Error(e));
                            }
                        }
                    }
                });
            })
            .forget();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = set_state.try_set(LoadingState::Loading);
            wasm_bindgen_futures::spawn_local(async move {
                match fetch(client).await {
                    Ok(data) => {
                        if fetch_version.load(std::sync::atomic::Ordering::SeqCst) == version {
                            let _ = set_state.try_set(LoadingState::Loaded(data));
                        }
                    }
                    Err(e) => {
                        if e.is_aborted() {
                            return;
                        }
                        if fetch_version.load(std::sync::atomic::Ordering::SeqCst) == version {
                            let page = get_current_path();
                            report_error(&e, page.as_deref(), is_authenticated);
                            let _ = set_state.try_set(LoadingState::Error(e));
                        }
                    }
                }
            });
        }
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

    (state, Callback::new(move |_| refetch()))
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

/// Conditional polling hook - only polls when the condition signal is true
///
/// Similar to `use_polling`, but accepts a reactive `should_poll` signal.
/// When `should_poll` becomes false, polling stops. When it becomes true again,
/// polling resumes. This is useful for scenarios like polling for running jobs
/// only when there are actually running jobs to poll.
///
/// Returns a cancel function that permanently stops the polling.
pub fn use_conditional_polling<F, Fut>(
    interval_ms: u32,
    should_poll: Signal<bool>,
    fetch: F,
) -> impl Fn()
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
        let is_polling = should_poll.get();

        // Clear any existing interval first
        let old_id = interval_id.swap(-1, Ordering::SeqCst);
        if old_id >= 0 {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(old_id);
            }
        }

        // Only set up polling if should_poll is true
        if !is_polling {
            return;
        }

        // Initial fetch when polling starts
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

/// State for optimistic updates with rollback capability
#[derive(Clone)]
pub struct OptimisticState<T: Clone + Send + Sync + 'static> {
    /// The actual state signal
    pub value: RwSignal<T>,
    /// Whether an update is in flight
    pub pending: RwSignal<bool>,
    /// Error from the last update attempt
    pub error: RwSignal<Option<ApiError>>,
}

impl<T: Clone + Send + Sync + 'static> OptimisticState<T> {
    /// Create a new optimistic state with an initial value
    pub fn new(initial: T) -> Self {
        Self {
            value: RwSignal::new(initial),
            pending: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    /// Get the current value
    pub fn get(&self) -> T {
        self.value.get()
    }

    /// Check if an update is pending
    pub fn is_pending(&self) -> bool {
        self.pending.get()
    }

    /// Get the error from the last update, if any
    pub fn get_error(&self) -> Option<ApiError> {
        self.error.get()
    }

    /// Clear any error state
    pub fn clear_error(&self) {
        self.error.set(None);
    }
}

/// Hook for optimistic updates with automatic rollback on error.
///
/// Updates the UI immediately, then makes the API call. If the call fails,
/// rolls back to the previous value and shows a toast notification.
///
/// # Example
/// ```rust,ignore
/// let (toggle_state, update_toggle) = use_optimistic(
///     false,
///     move |new_value| async move {
///         api.update_setting(new_value).await
///     }
/// );
///
/// // In view:
/// <Toggle
///     checked=Signal::derive(move || toggle_state.get())
///     on_change=move |val| update_toggle(val)
/// />
/// ```
pub fn use_optimistic<T, F, Fut>(
    initial: T,
    update_fn: F,
) -> (OptimisticState<T>, impl Fn(T) + Clone)
where
    T: Clone + PartialEq + Send + Sync + 'static,
    F: Fn(T) -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ApiResult<()>> + 'static,
{
    let state = OptimisticState::new(initial);
    let state_for_update = state.clone();

    let update = move |new_value: T| {
        let state = state_for_update.clone();
        let update_fn = update_fn.clone();

        // Capture the old value for rollback
        let old_value = state.value.get_untracked();

        // Skip if value hasn't changed
        if old_value == new_value {
            return;
        }

        // Optimistically update immediately
        state.value.set(new_value.clone());
        state.pending.set(true);
        state.error.set(None);

        // Make the API call
        wasm_bindgen_futures::spawn_local(async move {
            match update_fn(new_value).await {
                Ok(()) => {
                    // Success - update is already applied
                    state.pending.set(false);
                }
                Err(e) => {
                    // Failure - rollback to previous value
                    state.value.set(old_value);
                    state.pending.set(false);
                    state.error.set(Some(e.clone()));

                    // Show error toast if notifications context is available
                    if let Some(notifications) =
                        crate::signals::notifications::try_use_notifications()
                    {
                        notifications.error("Update failed", &e.to_string());
                    }
                }
            }
        });
    };

    (state, update)
}

/// Variant of use_optimistic that returns data from the update
pub fn use_optimistic_with_response<T, R, F, Fut>(
    initial: T,
    update_fn: F,
) -> (OptimisticState<T>, impl Fn(T) + Clone)
where
    T: Clone + PartialEq + Send + Sync + 'static,
    R: 'static,
    F: Fn(T) -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ApiResult<R>> + 'static,
{
    let state = OptimisticState::new(initial);
    let state_for_update = state.clone();

    let update = move |new_value: T| {
        let state = state_for_update.clone();
        let update_fn = update_fn.clone();

        // Capture the old value for rollback
        let old_value = state.value.get_untracked();

        // Skip if value hasn't changed
        if old_value == new_value {
            return;
        }

        // Optimistically update immediately
        state.value.set(new_value.clone());
        state.pending.set(true);
        state.error.set(None);

        // Make the API call
        wasm_bindgen_futures::spawn_local(async move {
            match update_fn(new_value).await {
                Ok(_response) => {
                    // Success - update is already applied
                    state.pending.set(false);
                }
                Err(e) => {
                    // Failure - rollback to previous value
                    state.value.set(old_value);
                    state.pending.set(false);
                    state.error.set(Some(e.clone()));

                    // Show error toast if notifications context is available
                    if let Some(notifications) =
                        crate::signals::notifications::try_use_notifications()
                    {
                        notifications.error("Update failed", &e.to_string());
                    }
                }
            }
        });
    };

    (state, update)
}
