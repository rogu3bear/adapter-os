//! PRD-UI-000: Safe Mode Page
//!
//! A minimal diagnostics UI that does NOT make any API calls.
//! Used for recovery when the main application fails to load or
//! when the backend is unreachable.
//!
//! Features:
//! - Build hash display
//! - Browser information
//! - Recovery actions (reload, clear storage, etc.)
//! - No network requests

use leptos::prelude::*;

/// Safe mode page - minimal diagnostics UI with no API calls
#[component]
pub fn Safe() -> impl IntoView {
    // Get browser info without making API calls
    let browser_info = get_browser_info();
    let build_info = get_build_info();
    let storage_info = get_storage_info();
    let boot_errors = get_boot_errors();

    view! {
        <div class="min-h-screen bg-background p-6">
            <div class="max-w-2xl mx-auto space-y-6">
                // Header
                <div class="text-center py-8">
                    <h1 class="text-3xl font-bold text-foreground">"AdapterOS Safe Mode"</h1>
                    <p class="mt-2 text-muted-foreground">
                        "Minimal diagnostics mode - no API calls are made from this page"
                    </p>
                </div>

                // Build Information
                <div class="rounded-lg border border-border bg-card p-6">
                    <h2 class="text-lg font-semibold mb-4 flex items-center gap-2">
                        <BuildIcon/>
                        "Build Information"
                    </h2>
                    <div class="space-y-2 font-mono text-sm">
                        <InfoRow label="Build Hash" value=build_info.hash.clone()/>
                        <InfoRow label="Build Time" value=build_info.time.clone()/>
                        <InfoRow label="Current Route" value=build_info.route.clone()/>
                        <InfoRow label="Boot Elapsed" value=build_info.elapsed.clone()/>
                    </div>
                </div>

                // Browser Information
                <div class="rounded-lg border border-border bg-card p-6">
                    <h2 class="text-lg font-semibold mb-4 flex items-center gap-2">
                        <BrowserIcon/>
                        "Browser Information"
                    </h2>
                    <div class="space-y-2 font-mono text-sm">
                        <InfoRow label="User Agent" value=browser_info.user_agent.clone()/>
                        <InfoRow label="Platform" value=browser_info.platform.clone()/>
                        <InfoRow label="Language" value=browser_info.language.clone()/>
                        <InfoRow label="Cookies Enabled" value=if browser_info.cookies_enabled { "Yes".to_string() } else { "No".to_string() }/>
                        <InfoRow label="Online" value=if browser_info.online { "Yes".to_string() } else { "No".to_string() }/>
                        <InfoRow label="Screen" value=browser_info.screen.clone()/>
                    </div>
                </div>

                // Storage Information
                <div class="rounded-lg border border-border bg-card p-6">
                    <h2 class="text-lg font-semibold mb-4 flex items-center gap-2">
                        <StorageIcon/>
                        "Local Storage"
                    </h2>
                    <div class="space-y-2 font-mono text-sm">
                        <InfoRow label="Items Count" value=storage_info.items_count.to_string()/>
                        <InfoRow label="Legacy Auth Token (should be No)" value=if storage_info.has_auth_token { "Yes (INSECURE)".to_string() } else { "No (Using secure cookies)".to_string() }/>
                    </div>
                </div>

                // Boot Errors (if any)
                {(!boot_errors.is_empty()).then(|| view! {
                    <div class="rounded-lg border border-destructive bg-destructive/10 p-6">
                        <h2 class="text-lg font-semibold mb-4 flex items-center gap-2 text-destructive">
                            <ErrorIcon/>
                            "Boot Errors"
                        </h2>
                        <ul class="space-y-2 font-mono text-sm text-destructive">
                            {boot_errors.iter().map(|err| view! {
                                <li class="flex items-start gap-2">
                                    <span class="text-destructive">"*"</span>
                                    <span>{err.clone()}</span>
                                </li>
                            }).collect::<Vec<_>>()}
                        </ul>
                    </div>
                })}

                // Recovery Actions
                <div class="rounded-lg border border-border bg-card p-6">
                    <h2 class="text-lg font-semibold mb-4 flex items-center gap-2">
                        <RecoveryIcon/>
                        "Recovery Actions"
                    </h2>
                    <div class="flex flex-wrap gap-3">
                        <button
                            class="px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                            on:click=|_| {
                                let _ = web_sys::window().map(|w| w.location().reload());
                            }
                        >
                            "Reload Page"
                        </button>

                        <button
                            class="px-4 py-2 rounded-md bg-secondary text-secondary-foreground hover:bg-secondary/80 transition-colors"
                            on:click=|_| {
                                clear_local_storage();
                                let _ = web_sys::window().map(|w| w.location().reload());
                            }
                        >
                            "Clear Storage & Reload"
                        </button>

                        <a
                            href="/"
                            class="px-4 py-2 rounded-md bg-muted text-foreground hover:bg-muted/80 transition-colors inline-block"
                        >
                            "Try Home Page"
                        </a>

                        <a
                            href="/login"
                            class="px-4 py-2 rounded-md bg-muted text-foreground hover:bg-muted/80 transition-colors inline-block"
                        >
                            "Go to Login"
                        </a>
                    </div>
                </div>

                // Troubleshooting Tips
                <div class="rounded-lg border border-border bg-card p-6">
                    <h2 class="text-lg font-semibold mb-4">"Troubleshooting Tips"</h2>
                    <ul class="space-y-2 text-sm text-muted-foreground">
                        <li class="flex items-start gap-2">
                            <span class="text-primary">"1."</span>
                            <span>"If the page is blank, try clearing local storage and reloading."</span>
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-primary">"2."</span>
                            <span>"Check if the backend server is running (typically on port 8080)."</span>
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-primary">"3."</span>
                            <span>"Open browser developer tools (F12) to check for JavaScript errors."</span>
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-primary">"4."</span>
                            <span>"Verify network connectivity if the backend health checks fail."</span>
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-primary">"5."</span>
                            <span>"If problems persist, try a different browser or incognito mode."</span>
                        </li>
                    </ul>
                </div>

                // Footer
                <div class="text-center text-sm text-muted-foreground py-4">
                    <p>"This page is intentionally minimal and makes no API calls."</p>
                </div>
            </div>
        </div>
    }
}

/// Information row component
#[component]
fn InfoRow(label: &'static str, value: String) -> impl IntoView {
    let title = value.clone();
    view! {
        <div class="flex justify-between items-start gap-4">
            <span class="text-muted-foreground shrink-0">{label}":"</span>
            <span class="text-foreground break-all text-right" title=title>{value}</span>
        </div>
    }
}

// --- Icon Components ---

#[component]
fn BuildIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z"/>
        </svg>
    }
}

#[component]
fn BrowserIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/>
        </svg>
    }
}

#[component]
fn StorageIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4"/>
        </svg>
    }
}

#[component]
fn ErrorIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
        </svg>
    }
}

#[component]
fn RecoveryIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
        </svg>
    }
}

// --- Helper Structs and Functions ---

struct BrowserInfo {
    user_agent: String,
    platform: String,
    language: String,
    cookies_enabled: bool,
    online: bool,
    screen: String,
}

struct BuildInfo {
    hash: String,
    time: String,
    route: String,
    elapsed: String,
}

struct StorageInfo {
    items_count: usize,
    has_auth_token: bool,
}

/// Get browser information without making any API calls
fn get_browser_info() -> BrowserInfo {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().expect("no window");
        let navigator = window.navigator();

        let user_agent = navigator
            .user_agent()
            .unwrap_or_else(|_| "Unknown".to_string());
        let platform = navigator
            .platform()
            .unwrap_or_else(|_| "Unknown".to_string());
        let language = navigator
            .language()
            .unwrap_or_else(|| "Unknown".to_string());
        // cookie_enabled requires web-sys feature, assume true for basic compatibility
        let cookies_enabled = true;
        let online = navigator.on_line();

        // screen() requires web-sys Screen feature, use viewport dimensions instead
        let screen_info = {
            let inner_width = window
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u32;
            let inner_height = window
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u32;
            format!("{}x{} (viewport)", inner_width, inner_height)
        };

        BrowserInfo {
            user_agent,
            platform,
            language,
            cookies_enabled,
            online,
            screen: screen_info,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        BrowserInfo {
            user_agent: "N/A (not in browser)".to_string(),
            platform: "N/A".to_string(),
            language: "N/A".to_string(),
            cookies_enabled: false,
            online: false,
            screen: "N/A".to_string(),
        }
    }
}

/// Get build information from the JS global
fn get_build_info() -> BuildInfo {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_name = "aosGetBuildInfo")]
            fn aos_get_build_info() -> JsValue;
        }

        let info = aos_get_build_info();
        if info.is_object() {
            let hash = js_sys::Reflect::get(&info, &"hash".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "unknown".to_string());
            let time = js_sys::Reflect::get(&info, &"time".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "unknown".to_string());
            let route = js_sys::Reflect::get(&info, &"route".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "/".to_string());
            let elapsed = js_sys::Reflect::get(&info, &"elapsed".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "0s".to_string());

            BuildInfo {
                hash,
                time,
                route,
                elapsed,
            }
        } else {
            BuildInfo {
                hash: "unknown".to_string(),
                time: "unknown".to_string(),
                route: "/safe".to_string(),
                elapsed: "0s".to_string(),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        BuildInfo {
            hash: "dev".to_string(),
            time: "N/A".to_string(),
            route: "/safe".to_string(),
            elapsed: "0s".to_string(),
        }
    }
}

/// Get local storage information
fn get_storage_info() -> StorageInfo {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().expect("no window");
        let storage = window.local_storage().ok().flatten();

        if let Some(storage) = storage {
            let items_count = storage.length().unwrap_or(0) as usize;
            // Auth tokens are now in httpOnly cookies, not localStorage
            // We can check for legacy tokens that should not exist
            let has_legacy_token = storage.get_item("aos_auth_token").ok().flatten().is_some()
                || storage.get_item("auth_token").ok().flatten().is_some()
                || storage.get_item("token").ok().flatten().is_some();

            StorageInfo {
                items_count,
                // has_auth_token now indicates if LEGACY tokens exist (should be false)
                // Auth is now via httpOnly cookies which can't be read from JS
                has_auth_token: has_legacy_token,
            }
        } else {
            StorageInfo {
                items_count: 0,
                has_auth_token: false,
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        StorageInfo {
            items_count: 0,
            has_auth_token: false,
        }
    }
}

/// Get boot errors from the JS global
fn get_boot_errors() -> Vec<String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_name = "aosGetBuildInfo")]
            fn aos_get_build_info() -> JsValue;
        }

        let info = aos_get_build_info();
        if info.is_object() {
            if let Ok(errors) = js_sys::Reflect::get(&info, &"errors".into()) {
                if let Some(array) = errors.dyn_ref::<js_sys::Array>() {
                    return array.iter().filter_map(|v| v.as_string()).collect();
                }
            }
        }
        Vec::new()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Clear local storage
fn clear_local_storage() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.clear();
            }
        }
    }
}
