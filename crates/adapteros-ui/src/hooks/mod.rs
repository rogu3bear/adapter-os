//! Custom hooks for common patterns
//!
//! Leptos-style hooks for data fetching, state management, etc.

pub mod use_delete_dialog;
pub mod use_list_controls;
pub mod use_sse_notifications;

pub use use_delete_dialog::{use_delete_dialog, DeleteDialogState};
pub use use_list_controls::{use_list_controls, ListControls, DEFAULT_PAGE_SIZE};
pub use use_sse_notifications::use_sse_notifications;

// Re-export the Refetch type since it's part of the use_api_resource return type
// (Callers already import from `crate::hooks`).

#[cfg(target_arch = "wasm32")]
use crate::api::report_error;
use crate::api::{use_api_client, ApiClient, ApiError, ApiResult};
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// SWR cache — thread_local HashMap for stale-while-revalidate
// ---------------------------------------------------------------------------

/// TTL constants for cached API resources.
pub struct CacheTtl;

impl CacheTtl {
    /// 5 seconds — for lists that change frequently (workers, adapters, jobs).
    pub const LIST: f64 = 5_000.0;
    /// 30 seconds — for status endpoints that change slowly.
    pub const STATUS: f64 = 30_000.0;
    /// 10 seconds — for single-entity detail views.
    pub const DETAIL: f64 = 10_000.0;
}

#[cfg(target_arch = "wasm32")]
struct ApiCacheEntry {
    timestamp_ms: f64,
    data: Box<dyn std::any::Any>,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static API_CACHE: std::cell::RefCell<std::collections::HashMap<String, ApiCacheEntry>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(target_arch = "wasm32")]
fn cache_get<T: Clone + 'static>(key: &str) -> Option<(T, f64)> {
    API_CACHE.with(|cache| {
        let map = cache.borrow();
        let entry = map.get(key)?;
        let data = entry.data.downcast_ref::<T>()?;
        Some((data.clone(), entry.timestamp_ms))
    })
}

#[cfg(target_arch = "wasm32")]
fn cache_set<T: Clone + 'static>(key: &str, data: &T) {
    API_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        map.insert(
            key.to_string(),
            ApiCacheEntry {
                timestamp_ms: js_sys::Date::now(),
                data: Box::new(data.clone()),
            },
        );
    });
}

/// Invalidate a specific cache entry.
#[cfg(target_arch = "wasm32")]
pub fn cache_invalidate(key: &str) {
    API_CACHE.with(|cache| {
        cache.borrow_mut().remove(key);
    });
}

/// No-op stub for non-WASM targets so callers can use `cache_invalidate` unconditionally.
#[cfg(not(target_arch = "wasm32"))]
pub fn cache_invalidate(_key: &str) {}

// ---------------------------------------------------------------------------
// Refetch — a scope-safe handle for re-triggering API resource fetches
// ---------------------------------------------------------------------------

/// A scope-safe handle for re-fetching API data.
///
/// Unlike [`Callback`], calling [`Refetch::run`] after the owning reactive
/// scope has been disposed is a silent no-op instead of a panic.  This makes
/// it safe to call from `spawn_local` async blocks or `Effect`s that may
/// outlive the component that created them.
///
/// `Refetch` is [`Copy`] and can be moved into closures freely.
#[derive(Clone, Copy)]
pub struct Refetch(StoredValue<Arc<dyn Fn() + Send + Sync>>);

impl Refetch {
    /// Create a new refetch handle from the given closure.
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(StoredValue::new(Arc::new(f) as Arc<dyn Fn() + Send + Sync>))
    }

    /// Run the refetch.  No-op if the reactive scope has been disposed.
    pub fn run(&self, _input: ()) {
        let _ = self.0.try_with_value(|f| f());
    }

    /// Convert to a [`Callback<()>`] for use as a component prop.
    ///
    /// The returned `Callback` internally delegates to `Refetch::run`,
    /// so it is safe even if the *original* scope is disposed — though the
    /// `Callback` itself must still be alive (true for synchronous prop usage
    /// like button clicks).
    pub fn as_callback(self) -> Callback<()> {
        Callback::new(move |_| self.run(()))
    }
}

impl From<Refetch> for Callback<()> {
    fn from(r: Refetch) -> Self {
        r.as_callback()
    }
}

// ---------------------------------------------------------------------------
// Scope-alive guard — prevents panics when calling Callback from spawn_local
// ---------------------------------------------------------------------------

/// Returns an `Arc<AtomicBool>` that reads `true` while the current reactive
/// scope is alive and flips to `false` on cleanup.
///
/// Use this to guard `Callback.run()` calls inside `spawn_local` blocks:
///
/// ```rust,ignore
/// let alive = use_scope_alive();
/// spawn_local(async move {
///     // ... async work ...
///     if alive.load(Ordering::SeqCst) {
///         on_submit.run(());
///     }
/// });
/// ```
///
/// In WASM's single-threaded execution model there is no race between the
/// check and the call.
pub fn use_scope_alive() -> Arc<AtomicBool> {
    let alive = Arc::new(AtomicBool::new(true));
    let alive_for_cleanup = Arc::clone(&alive);
    on_cleanup(move || {
        alive_for_cleanup.store(false, Ordering::SeqCst);
    });
    alive
}

/// Get the current page path for error reporting
#[allow(dead_code)] // Reserved for future error reporting enhancements
fn get_current_path() -> Option<String> {
    web_sys::window().and_then(|w| w.location().pathname().ok())
}

/// Resource loading state
#[derive(Debug, Clone, Default)]
pub enum LoadingState<T> {
    /// Not started
    Idle,
    /// Loading
    #[default]
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
/// Uses a version counter to invalidate stale requests on component unmount.
pub fn use_api_resource<T, F, Fut>(fetch: F) -> (ReadSignal<LoadingState<T>>, Refetch)
where
    T: Clone + Send + Sync + 'static,
    F: Fn(Arc<ApiClient>) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = ApiResult<T>> + 'static,
{
    let (state, set_state) = signal(LoadingState::<T>::Idle);
    let client = use_api_client();
    let is_authenticated = client.is_authenticated();
    let fetch_version = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Increment version to invalidate any in-flight requests on cleanup
    // This is Send+Sync safe and prevents stale updates after unmount
    let fetch_version_for_cleanup = Arc::clone(&fetch_version);
    on_cleanup(move || {
        // Bump the version so any in-flight requests know they're stale
        fetch_version_for_cleanup.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });

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
            let set_state_loading = set_state;
            let set_state_result = set_state;
            let fetch_version_check = Arc::clone(&fetch_version);

            // The timeout callback is a one-shot operation that runs immediately (0ms delay).
            // While the Timeout handle is forgotten (WASM limitation), the version counter
            // ensures stale responses are discarded after component unmount.
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
        // Non-WASM branch is intentionally empty; this hook only runs in browsers.
        // The cfg gate exists to satisfy cargo check on native targets during CI.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (
                &set_state,
                &fetch,
                &client,
                &fetch_version,
                version,
                is_authenticated,
            );
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

    (state, Refetch::new(refetch))
}

/// Create a resource that fetches data from the API with stale-while-revalidate caching.
///
/// On mount, returns cached data immediately (avoiding spinner flash on re-navigation),
/// then revalidates in the background if the cache entry is stale (older than `ttl_ms`).
/// Fresh cache hits skip the network request entirely.
///
/// Falls back to `use_api_resource` on non-WASM targets.
pub fn use_cached_api_resource<T, F, Fut>(
    cache_key: &str,
    ttl_ms: f64,
    fetch: F,
) -> (ReadSignal<LoadingState<T>>, Refetch)
where
    T: Clone + Send + Sync + 'static,
    F: Fn(Arc<ApiClient>) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = ApiResult<T>> + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (cache_key, ttl_ms);
        use_api_resource(fetch)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let cache_key = cache_key.to_string();

        // Check cache for initial state
        let (initial_state, needs_fetch) = match cache_get::<T>(&cache_key) {
            Some((data, ts)) => {
                let age = js_sys::Date::now() - ts;
                if age < ttl_ms {
                    // Fresh hit — skip fetch entirely
                    (LoadingState::Loaded(data), false)
                } else {
                    // Stale hit — show cached data, revalidate in background
                    (LoadingState::Loaded(data), true)
                }
            }
            None => (LoadingState::Idle, true),
        };

        let (state, set_state) = signal(initial_state);
        let client = use_api_client();
        let is_authenticated = client.is_authenticated();
        let fetch_version = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let fetch_version_for_cleanup = Arc::clone(&fetch_version);
        on_cleanup(move || {
            fetch_version_for_cleanup.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });

        let cache_key_for_refetch = cache_key.clone();
        let fetch_clone = fetch.clone();
        let client_clone = Arc::clone(&client);
        let fetch_version_clone = Arc::clone(&fetch_version);
        let state_for_refetch = state;
        let refetch = move || {
            let client = Arc::clone(&client_clone);
            let fetch = fetch_clone.clone();
            let fetch_version = Arc::clone(&fetch_version_clone);
            let version = fetch_version.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let cache_key = cache_key_for_refetch.clone();

            let set_state_result = set_state;
            let fetch_version_check = Arc::clone(&fetch_version);

            gloo_timers::callback::Timeout::new(0, move || {
                // Only show loading spinner if we don't have cached data displayed
                if !matches!(state_for_refetch.try_get(), Some(LoadingState::Loaded(_))) {
                    let _ = set_state_result.try_set(LoadingState::Loading);
                }

                wasm_bindgen_futures::spawn_local(async move {
                    match fetch(client).await {
                        Ok(data) => {
                            if fetch_version_check.load(std::sync::atomic::Ordering::SeqCst)
                                == version
                            {
                                cache_set(&cache_key, &data);
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
        };

        if needs_fetch {
            let refetch_init = refetch.clone();
            Effect::new(move || {
                untrack(|| {
                    refetch_init();
                });
            });
        }

        (state, Refetch::new(refetch))
    }
}

/// Simple polling hook with automatic cleanup
///
/// Returns a cancel function that stops the polling when called.
/// The interval is automatically cleared when the component unmounts.
///
/// # Implementation Note
/// Uses raw `setInterval`/`clearInterval` via web_sys to enable proper cleanup.
/// The interval ID is stored atomically for Send+Sync cleanup compatibility.
/// The closure is leaked (WASM limitation), but the interval is properly cleared.
pub fn use_polling<F, Fut>(interval_ms: u32, fetch: F) -> impl Fn()
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // Store interval ID for cleanup (-1 = no interval)
    let interval_id = Arc::new(AtomicI32::new(-1));
    let interval_id_for_cleanup = Arc::clone(&interval_id);
    let interval_id_for_cancel = Arc::clone(&interval_id);

    // Track if we've already initialized (prevent re-initialization on Effect re-run)
    let initialized = Arc::new(AtomicBool::new(false));

    // Register cleanup to clear interval on unmount
    on_cleanup(move || {
        let id = interval_id_for_cleanup.swap(-1, Ordering::SeqCst);
        if id >= 0 {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(id);
            }
        }
    });

    Effect::new(move || {
        // Only initialize once - Effects can re-run but we don't want duplicate intervals
        // or leaked closures
        if initialized.swap(true, Ordering::SeqCst) {
            return;
        }

        let fetch = fetch.clone();
        let interval_id = Arc::clone(&interval_id);

        // Initial fetch - deferred via Timeout to avoid RefCell re-entrancy
        // panic in wasm-bindgen-futures when spawn_local is called from Effect body
        let fetch_init = fetch.clone();
        gloo_timers::callback::Timeout::new(0, move || {
            wasm_bindgen_futures::spawn_local(async move {
                fetch_init().await;
            });
        })
        .forget();

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
///
/// # Implementation Note
/// Each time should_poll transitions to true, a new closure is created and leaked.
/// This is acceptable because:
/// 1. The interval is properly cleared when should_poll becomes false
/// 2. The transition should happen infrequently (not on every render)
/// 3. In WASM, closures cannot be dropped without cooperation from JS
pub fn use_conditional_polling<F, Fut>(
    interval_ms: u32,
    should_poll: Signal<bool>,
    fetch: F,
) -> impl Fn()
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // Store interval ID for cleanup (-1 = no interval)
    let interval_id = Arc::new(AtomicI32::new(-1));
    let interval_id_for_cleanup = Arc::clone(&interval_id);
    let interval_id_for_cancel = Arc::clone(&interval_id);

    // Track if permanently cancelled
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_for_effect = Arc::clone(&cancelled);
    let cancelled_for_cancel = Arc::clone(&cancelled);

    // Register cleanup to clear interval on unmount
    on_cleanup(move || {
        let id = interval_id_for_cleanup.swap(-1, Ordering::SeqCst);
        if id >= 0 {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(id);
            }
        }
    });

    Effect::new(move || {
        let fetch = fetch.clone();
        let interval_id = Arc::clone(&interval_id);
        let is_polling = should_poll.try_get().unwrap_or(false);
        let cancelled = Arc::clone(&cancelled_for_effect);

        // If permanently cancelled, do nothing
        if cancelled.load(Ordering::SeqCst) {
            return;
        }

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

        // Initial fetch when polling starts - deferred via Timeout to avoid
        // RefCell re-entrancy in wasm-bindgen-futures when called from Effect body
        let fetch_init = fetch.clone();
        gloo_timers::callback::Timeout::new(0, move || {
            wasm_bindgen_futures::spawn_local(async move {
                fetch_init().await;
            });
        })
        .forget();

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
        // Mark as permanently cancelled
        cancelled_for_cancel.store(true, Ordering::SeqCst);
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

/// Get the shared API client from context, falling back to a fresh instance.
pub fn use_api() -> Arc<ApiClient> {
    use_context::<Arc<ApiClient>>().unwrap_or_else(|| Arc::new(ApiClient::new()))
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
                    let _ = state.pending.try_set(false);
                }
                Err(e) => {
                    // Failure - rollback to previous value
                    let _ = state.value.try_set(old_value);
                    let _ = state.pending.try_set(false);
                    let _ = state.error.try_set(Some(e.clone()));

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
                    let _ = state.pending.try_set(false);
                }
                Err(e) => {
                    // Failure - rollback to previous value
                    let _ = state.value.try_set(old_value);
                    let _ = state.pending.try_set(false);
                    let _ = state.error.try_set(Some(e.clone()));

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
