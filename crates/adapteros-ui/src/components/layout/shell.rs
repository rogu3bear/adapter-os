//! Shell - Main Application Frame
//!
//! The top-level application shell with top bar, sidebar, and main workspace.
//! Includes global keyboard shortcuts:
//! - Ctrl+K / Cmd+K: Command Palette
//! - Alt+1..Alt+8: Jump to workflow group

use super::nav_registry::route_for_alt_shortcut;
use super::sidebar::{provide_sidebar_context, SidebarNav};
use super::topbar::TopBar;
use crate::api::sse::{
    use_adapter_lifecycle_sse, use_health_lifecycle_sse, use_training_lifecycle_sse,
};
use crate::components::inference_banner::InferenceBanner;
use crate::components::offline_banner::OfflineBanner;
use crate::components::status_center::StatusCenterProvider;
use crate::components::workspace::Workspace;
use crate::signals::{
    provide_route_context, use_route_context, use_search, use_settings, use_ui_profile,
};
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::{use_location, use_navigate};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    static SHELL_KEYDOWN_LISTENER: RefCell<Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>> =
        RefCell::new(None);
}

#[cfg(target_arch = "wasm32")]
fn register_shell_keydown_listener(listener: Closure<dyn FnMut(web_sys::KeyboardEvent)>) {
    SHELL_KEYDOWN_LISTENER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(window) = web_sys::window() {
            if let Some(existing) = slot.take() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    existing.as_ref().unchecked_ref(),
                );
            }
            let _ = window
                .add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
            *slot = Some(listener);
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn register_shell_keydown_listener(_listener: Closure<dyn FnMut(web_sys::KeyboardEvent)>) {}

#[cfg(target_arch = "wasm32")]
fn clear_shell_keydown_listener() {
    SHELL_KEYDOWN_LISTENER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(existing) = slot.take() {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    existing.as_ref().unchecked_ref(),
                );
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_shell_keydown_listener() {}

/// Application shell with top bar, bottom taskbar, and main workspace.
/// Uses Outlet to render the matched child route from ParentRoute.
#[component]
pub fn Shell() -> impl IntoView {
    #[cfg(all(feature = "hydrate", target_arch = "wasm32"))]
    crate::debug_log!("[Shell] Rendering...");
    provide_sidebar_context();
    provide_route_context();
    let search = use_search();
    let route_context = use_route_context();

    // SSE lifecycle subscriptions — Shell is inside ProtectedRoute so these
    // only activate when the user is authenticated. The hooks dispatch into
    // the refetch system so UI components auto-refresh on backend events.
    let _adapter_sse = use_adapter_lifecycle_sse();
    let _training_sse = use_training_lifecycle_sse();
    let _health_sse = use_health_lifecycle_sse();

    // Track route changes for contextual actions in Command Palette
    let location = use_location();
    Effect::new(move || {
        let Some(pathname) = location.pathname.try_get() else {
            return;
        };
        route_context.set_route(&pathname);
        route_context.clear_selected();

        // Update document title based on current route
        let title = match pathname.as_str() {
            "/" | "/dashboard" => "Home",
            "/adapters" => "Adapters",
            "/update-center" => "Versions",
            "/training" => "Build",
            "/chat" => "Chat",
            "/models" => "Models",
            "/workers" => "Workers",
            "/settings" => "Settings",
            "/documents" => "Documents",
            "/datasets" => "Datasets",
            "/policies" => "Policies",
            "/audit" => "Audit Log",
            "/admin" => "Admin",
            "/runs" => "Execution Records",
            "/user" => "Settings",
            "/welcome" => "Welcome",
            "/system" => "System",
            "/chat/history" => "Chat History",
            _ if pathname.starts_with("/chat/s/") => "Chat Session",
            _ if pathname.starts_with("/training/") => "Build Details",
            _ if pathname.starts_with("/runs/") => "Execution Record Detail",
            _ if pathname.starts_with("/adapters/") => "Adapter Detail",
            _ if pathname.starts_with("/workers/") => "Worker Detail",
            _ if pathname.starts_with("/models/") => "Model Details",
            _ if pathname.starts_with("/documents/") => "Document Details",
            _ if pathname.starts_with("/datasets/") => "Dataset Detail",
            _ => "AdapterOS",
        };
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                document.set_title(&format!("{} \u{2014} AdapterOS", title));
            }

            // Clear panic overlay on navigation (it's outside Leptos, so we call JS directly)
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(overlay) = document.get_element_by_id("aos-panic-overlay") {
                        let _ = overlay.class_list().remove_1("visible");
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = title;
    });

    // Get UI profile for alt shortcuts
    let ui_profile = use_ui_profile();
    let navigate = use_navigate();

    // Global keyboard handler for Command Palette and Alt+1-8 shortcuts.
    // Register once and remove on cleanup so remounts do not accumulate handlers.
    Effect::new(move || {
        let search = search.clone();
        let navigate = navigate.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            let ctrl_or_cmd = event.ctrl_key() || event.meta_key();
            let alt_key = event.alt_key();

            // Check if we're in an input field
            if let Some(target) = event.target() {
                if let Some(element) = target.dyn_ref::<web_sys::HtmlElement>() {
                    let tag = element.tag_name().to_lowercase();
                    if tag == "input" || tag == "textarea" {
                        // Allow Escape to blur
                        if key == "Escape" {
                            let _ = element.blur();
                            event.prevent_default();
                            return;
                        }
                        // Don't intercept other keys in inputs (except Ctrl+K)
                        if !(ctrl_or_cmd && key == "k") {
                            return;
                        }
                    }
                }
            }

            // Ctrl+K or Cmd+K opens command palette
            if ctrl_or_cmd && key == "k" {
                event.prevent_default();
                search.toggle();
                return;
            }

            // "/" opens command palette when not in input
            if key == "/"
                && !search
                    .command_palette_open
                    .try_get_untracked()
                    .unwrap_or(false)
            {
                event.prevent_default();
                search.open();
                return;
            }

            // Alt+0 jumps to Dashboard, Alt+1..Alt+8 jumps to workflow group
            // ASSUMPTION: On macOS, Option key maps to alt_key
            if alt_key && !ctrl_or_cmd {
                if let Some(digit) = key.chars().next().and_then(|c| c.to_digit(10)) {
                    if digit == 0 {
                        event.prevent_default();
                        navigate("/", Default::default());
                    } else if (1..=8).contains(&digit) {
                        if let Some(profile) = ui_profile.try_get_untracked() {
                            if let Some(route) = route_for_alt_shortcut(profile, digit as u8) {
                                event.prevent_default();
                                navigate(route, Default::default());
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        register_shell_keydown_listener(closure);
        on_cleanup(clear_shell_keydown_listener);
    });

    let settings = use_settings();
    let shell_class = move || {
        if settings
            .try_get()
            .map(|s| s.density.is_compact())
            .unwrap_or(false)
        {
            "shell compact"
        } else {
            "shell"
        }
    };

    view! {
        <StatusCenterProvider>
            <div class=shell_class>
                // Skip to main content link for keyboard accessibility
                <a
                    href="#main-content"
                    class="skip-to-main"
                >
                    "Skip to main content"
                </a>

                // PRD-UI-000: Offline banner for API connectivity status
                <OfflineBanner/>
                // Inference readiness banner (e.g., no model loaded / no workers)
                <InferenceBanner/>
                // Streaming health indicator (SSE) could go here if needed

                // Top bar
                <TopBar/>
                // Main content area with sidebar + workspace
                <div class="shell-content">
                    // Left sidebar navigation
                    <SidebarNav/>

                    // Main workspace wrapper
                    <Workspace class="shell-workspace">
                        <main id="main-content" class="shell-main" tabindex="-1">
                            <Outlet/>
                        </main>
                    </Workspace>

                </div>

            </div>
        </StatusCenterProvider>
    }
}
