//! PRD-UI-000: Offline Banner Component
//!
//! Displays a banner when the backend API is unreachable.
//! Polls `/healthz` every 30 seconds to detect connectivity issues.

use gloo_net::http::Request;
use leptos::prelude::*;

/// Connection state for the offline banner
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnectionState {
    /// Connected to the backend
    Online,
    /// Checking connection status
    Checking,
    /// Backend is unreachable
    Offline,
}

/// Offline banner component that polls /healthz every 30 seconds
/// and displays a warning banner when the API is unreachable.
#[component]
pub fn OfflineBanner() -> impl IntoView {
    let (connection_state, set_connection_state) = signal(ConnectionState::Checking);
    let (last_check, set_last_check) = signal(String::new());
    let (retry_count, set_retry_count) = signal(0u32);

    // Initial check on mount
    Effect::new(move |_| {
        check_health(set_connection_state, set_last_check, set_retry_count);
    });

    // Set up polling interval (every 30 seconds)
    Effect::new(move |_| {
        let interval = gloo_timers::callback::Interval::new(30_000, move || {
            check_health(set_connection_state, set_last_check, set_retry_count);
        });
        // Keep the interval alive for the component lifetime
        std::mem::forget(interval);
    });

    view! {
        // Only show banner when offline
        <Show when=move || connection_state.get() == ConnectionState::Offline>
            <div class="fixed top-0 left-0 right-0 z-[9999] bg-destructive text-destructive-foreground">
                <div class="flex items-center justify-between px-4 py-2 text-sm">
                    <div class="flex items-center gap-3">
                        <OfflineIcon/>
                        <span class="font-medium">"Backend Unreachable"</span>
                        <span class="opacity-80">
                            "Cannot connect to the API server. Some features may not work."
                        </span>
                    </div>

                    <div class="flex items-center gap-4">
                        // Last check time
                        <span class="text-xs opacity-70">
                            "Last check: " {move || last_check.get()}
                        </span>

                        // Retry count
                        {move || {
                            let count = retry_count.get();
                            if count > 0 {
                                view! {
                                    <span class="text-xs opacity-70">
                                        "(" {count} " retries)"
                                    </span>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }
                        }}

                        // Manual retry button
                        <button
                            class="px-3 py-1 rounded text-xs font-medium bg-destructive-foreground/20 hover:bg-destructive-foreground/30 transition-colors"
                            on:click=move |_| {
                                set_connection_state.set(ConnectionState::Checking);
                                check_health(set_connection_state, set_last_check, set_retry_count);
                            }
                        >
                            "Retry Now"
                        </button>

                        // Safe mode link
                        <a
                            href="/safe"
                            class="px-3 py-1 rounded text-xs font-medium bg-destructive-foreground/20 hover:bg-destructive-foreground/30 transition-colors"
                        >
                            "Safe Mode"
                        </a>
                    </div>
                </div>
            </div>
        </Show>

        // Show subtle indicator when checking
        <Show when=move || connection_state.get() == ConnectionState::Checking>
            <div class="fixed top-0 right-4 z-[9999]">
                <div class="flex items-center gap-2 px-3 py-1 text-xs text-muted-foreground bg-muted/50 rounded-b">
                    <span class="w-2 h-2 rounded-full bg-yellow-500 animate-pulse"></span>
                    "Checking connection..."
                </div>
            </div>
        </Show>
    }
}

/// Perform health check against /healthz endpoint
fn check_health(
    set_state: WriteSignal<ConnectionState>,
    set_last_check: WriteSignal<String>,
    set_retry_count: WriteSignal<u32>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        // Update last check time
        let now = get_current_time();
        set_last_check.set(now);

        // Perform the health check
        let result = Request::get("/healthz")
            .header("Accept", "application/json")
            .send()
            .await;

        match result {
            Ok(response) if response.ok() => {
                set_state.set(ConnectionState::Online);
                set_retry_count.set(0);
            }
            Ok(_) => {
                // Got a response but not OK status
                set_state.set(ConnectionState::Offline);
                set_retry_count.update(|c| *c += 1);
            }
            Err(_) => {
                // Network error
                set_state.set(ConnectionState::Offline);
                set_retry_count.update(|c| *c += 1);
            }
        }
    });
}

/// Get current time formatted as HH:MM:SS
fn get_current_time() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::Date;
        let date = Date::new_0();
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        let seconds = date.get_seconds();
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "00:00:00".to_string()
    }
}

/// Offline icon component
#[component]
fn OfflineIcon() -> impl IntoView {
    view! {
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 5.636a9 9 0 010 12.728m0 0l-2.829-2.829m2.829 2.829L21 21M15.536 8.464a5 5 0 010 7.072m0 0l-2.829-2.829m-4.243 2.829a4.978 4.978 0 01-1.414-2.83m-1.414 5.658a9 9 0 01-2.167-9.238m7.824 2.167a1 1 0 111.414 1.414m-1.414-1.414L3 3m8.293 8.293l1.414 1.414"/>
        </svg>
    }
}
