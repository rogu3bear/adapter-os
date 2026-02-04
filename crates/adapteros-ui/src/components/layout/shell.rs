//! Shell - Main Application Frame
//!
//! The top-level application shell with top bar, bottom taskbar, and main workspace.

use super::taskbar::Taskbar;
use super::topbar::TopBar;
use crate::components::chat_dock::{ChatDockPanel, MobileChatOverlay, NarrowChatDock};
use crate::components::error_history_panel::ErrorHistory;
use crate::components::offline_banner::OfflineBanner;
use crate::components::status_center::StatusCenterProvider;
use crate::components::telemetry_overlay::TelemetryOverlay;
use crate::components::workspace::Workspace;
use crate::signals::{
    provide_route_context, provide_ui_profile_context, use_chat, use_route_context, use_search,
    DockState,
};
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Application shell with top bar, bottom taskbar, and main workspace
#[component]
pub fn Shell(children: Children) -> impl IntoView {
    web_sys::console::log_1(&"[Shell] Rendering...".into());
    provide_ui_profile_context();
    provide_route_context();
    let (chat_state, _chat_action) = use_chat();
    let search = use_search();
    let route_context = use_route_context();
    web_sys::console::log_1(&"[Shell] Got chat context".into());

    // Track route changes for contextual actions in Command Palette
    let location = use_location();
    Effect::new(move || {
        let pathname = location.pathname.get();
        route_context.set_route(&pathname);
        // Clear selection when route changes
        route_context.clear_selected();
    });

    // Global keyboard handler for Command Palette
    let keyboard_handler_set = StoredValue::new(false);
    Effect::new(move || {
        if keyboard_handler_set.get_value() {
            return;
        }
        keyboard_handler_set.set_value(true);

        let search = search.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            let ctrl_or_cmd = event.ctrl_key() || event.meta_key();

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
            if key == "/" && !search.command_palette_open.get_untracked() {
                event.prevent_default();
                search.open();
            }
        }) as Box<dyn FnMut(_)>);

        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });

    view! {
        <StatusCenterProvider>
            <div class="shell">
                // Skip to main content link for keyboard accessibility
                <a
                    href="#main-content"
                    class="skip-to-main"
                >
                    "Skip to main content"
                </a>

                // PRD-UI-000: Offline banner for API connectivity status
                <OfflineBanner/>
                // Streaming health indicator (SSE) could go here if needed

                // Top bar
                <TopBar/>

                // Main content area with workspace
                <div class="shell-content">
                    // Main workspace wrapper
                    <Workspace class="shell-workspace">
                        <main id="main-content" class="shell-main" tabindex="-1">
                            {children()}
                        </main>
                    </Workspace>

                    // Chat dock (collapsible right panel)
                    {move || {
                        match chat_state.get().dock_state {
                            DockState::Docked => view! { <ChatDockPanel/> }.into_any(),
                            DockState::Narrow => view! { <NarrowChatDock/> }.into_any(),
                            DockState::Hidden => view! {}.into_any(),
                        }
                    }}
                </div>

                // Bottom taskbar
                <Taskbar/>

                // Mobile chat overlay
                <MobileChatOverlay/>

                // Telemetry overlay (Ctrl+Shift+T toggle, off by default)
                <TelemetryOverlay/>

                // Error history panel (Ctrl+Shift+E toggle)
                <ErrorHistory/>
            </div>
        </StatusCenterProvider>
    }
}
