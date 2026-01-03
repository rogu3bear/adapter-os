//! AdapterOS Leptos UI
//!
//! A Leptos-based web frontend for the AdapterOS control plane.
//! This crate provides a CSR (Client-Side Rendered) application that
//! communicates with the AdapterOS backend via REST and SSE.
//!
//! # Architecture
//!
//! - `api/` - HTTP client and SSE infrastructure
//! - `components/` - Reusable UI components (buttons, dialogs, etc.)
//! - `pages/` - Route page components
//! - `signals/` - Reactive state management
//! - `hooks/` - Leptos-style hooks for common patterns
//!
//! # Features
//!
//! - `hydrate` - Enable client-side hydration (for SSR + hydration mode)
//! - `ssr` - Enable server-side rendering

// Leptos view! macro patterns that trigger clippy but are idiomatic
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]
// Callback<T> is Copy but .clone() is often clearer in closures
#![allow(clippy::clone_on_copy)]

pub mod api;
pub mod components;
pub mod hooks;
pub mod pages;
pub mod signals;
pub mod sse;
pub mod theme;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::*;
use leptos_router::path;

use components::{AuthProvider, ProtectedRoute, Shell};
use signals::provide_chat_context;

/// Main application component - full app with all routes
#[component]
pub fn App() -> impl IntoView {
    web_sys::console::log_1(&"[App] Rendering App component...".into());

    // Provides context for meta tags
    provide_meta_context();

    web_sys::console::log_1(&"[App] Meta context provided, creating view...".into());

    view! {
        <Title text="AdapterOS"/>
        <Meta charset="utf-8"/>
        <Meta name="viewport" content="width=device-width, initial-scale=1"/>

        <AuthProvider>
            <ChatProvider>
                <Router>
                    <Routes fallback=|| view! { <pages::NotFound/> }>
                        // Public routes (no shell)
                        <Route path=path!("/login") view=pages::Login/>

                        // Protected routes (with shell layout)
                        <Route path=path!("/") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Dashboard/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/dashboard") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Dashboard/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/adapters") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Adapters/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/adapters/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::AdapterDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/chat") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Chat/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/chat/:session_id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::ChatSession/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/training") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Training/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/system") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::System/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/settings") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Settings/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/models") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Models/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/policies") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Policies/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/stacks") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Stacks/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/stacks/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::StackDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/collections") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Collections/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/collections/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::CollectionDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/documents") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Documents/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/documents/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::DocumentDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/admin") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Admin/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/audit") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Audit/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/workers") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Workers/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/workers/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::WorkerDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/repositories") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::Repositories/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                        <Route path=path!("/repositories/:id") view=|| view! {
                            <ProtectedRoute>
                                <Shell>
                                    <pages::RepositoryDetail/>
                                </Shell>
                            </ProtectedRoute>
                        }/>
                    </Routes>
                </Router>
            </ChatProvider>
        </AuthProvider>
    }
}

/// Chat context provider component
#[component]
fn ChatProvider(children: Children) -> impl IntoView {
    provide_chat_context();
    children()
}

/// Mount point for the application
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn mount() {
    // Set up panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize tracing for WASM
    tracing_wasm::set_as_global_default();

    web_sys::console::log_1(&"[mount] Starting app mount...".into());

    // Mount the app to the document body
    leptos::mount::mount_to_body(App);

    web_sys::console::log_1(&"[mount] App mounted successfully".into());
}
