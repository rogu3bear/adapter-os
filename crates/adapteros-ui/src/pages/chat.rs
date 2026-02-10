//! Chat page with SSE streaming support
//!
//! This module provides the chat interface. The full chat page uses
//! the global chat state from signals/chat.rs for unified state management
//! with the dock panel.
//!
//! ## Performance Characteristics
//!
//! Streaming updates flow through:
//! 1. SSE token -> `stream_inference_to_state` (signals/chat.rs)
//! 2. Token appended via `push_str` (O(1) amortized)
//! 3. Signal update triggers reactive subscribers
//! 4. Message list re-renders (O(n) clone of messages Vec)
//!
//! The dock (`chat_dock.rs`) uses `<For>` with keyed iteration for O(1)
//! per-message updates. The full chat page uses a simpler pattern that
//! is acceptable for typical message counts (<100).
//!
//! Enable `show_telemetry_overlay` in settings for perf timing.

use crate::api::{api_base_url, ApiClient};
use crate::components::inference_guidance::guidance_for;
use crate::components::status_center::use_status_center;
use crate::components::{
    use_is_tablet_or_smaller, AdapterHeat, AdapterMagnet, Badge, BadgeVariant, Button, ButtonSize,
    ButtonVariant, ChatAdaptersRegion, Checkbox, ConfirmationDialog, ConfirmationSeverity, Dialog,
    Input, Spinner, SuggestedAdapterView, Textarea, TraceButton, TracePanel,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{
    use_chat, ChatSessionMeta, ChatSessionsManager, ChatTarget, StreamNoticeTone,
};
use adapteros_api_types::training::ChatMessageInput;
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::sync::Arc;
use wasm_bindgen::JsCast;

/// Maximum prompt length for URL-embedded prompts (bytes).
/// This prevents DoS attacks from extremely long URLs that could:
/// 1. Exceed browser URL limits (typically 2KB-8KB)
/// 2. Exhaust memory when decoded
/// 3. Overwhelm the inference endpoint
const MAX_URL_PROMPT_LENGTH: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AttachMode {
    #[default]
    Upload,
    Paste,
    Chat,
}

/// Chat landing page - redirects to the most recent session or shows empty state.
/// Route: /chat
#[component]
pub fn Chat() -> impl IntoView {
    let navigate = use_navigate();
    let sessions = ChatSessionsManager::load_sessions();

    // Redirect to the most recent session so the URL always reflects what's shown.
    if let Some(recent) = sessions.first() {
        // Preserve query params (?prompt=, ?adapter=) across the redirect.
        let search = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let path = format!("/chat/{}{}", recent.id, search);
        // Defer navigate to avoid RefCell re-entrancy: component creation runs
        // inside the wasm-bindgen-futures task queue, and navigate() internally
        // uses spawn_local, causing a double-borrow panic.
        gloo_timers::callback::Timeout::new(0, move || {
            navigate(
                &path,
                leptos_router::NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        })
        .forget();
        // Return empty view during redirect to avoid creating signals/effects
        // that would panic when this component is disposed by the route change.
        return view! { <div class="chat-redirect" /> }.into_any();
    }

    // No sessions exist — render empty-state workspace.
    // Also defer mounting to avoid the same wasm-bindgen-futures re-entrancy.
    let selected_signal = Signal::derive(|| None);
    let mounted = RwSignal::new(false);
    gloo_timers::callback::Timeout::new(0, move || {
        mounted.set(true);
    })
    .forget();

    view! {
        <>
            <h1 class="sr-only">"Chat"</h1>
            <Show when=move || mounted.try_get().unwrap_or(false) fallback=|| view! {
                <div class="chat-loading-placeholder" style="display:flex;align-items:center;justify-content:center;height:100%;opacity:0.5;">
                    <Spinner />
                </div>
            }>
                <ChatWorkspace selected_session_id=selected_signal />
            </Show>
        </>
    }
    .into_any()
}

/// Chat session page - renders workspace with session from route param.
/// Route: /chat/:session_id
///
/// Uses deferred mounting: renders a lightweight placeholder first, then mounts
/// the full ChatWorkspace on the next tick. This avoids a wasm-bindgen-futures
/// RefCell re-entrancy panic (#2562) that occurs when Leptos builds the complex
/// ChatWorkspace component tree inside the task queue during SPA navigation.
#[component]
pub fn ChatSession() -> impl IntoView {
    let params = use_params_map();
    let selected_id = Signal::derive(move || {
        let id = params.get().get("session_id").unwrap_or_default();
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    });

    // Defer heavy component tree construction to next tick to break out of the
    // wasm-bindgen-futures task queue context and avoid RefCell re-entrancy.
    let mounted = RwSignal::new(false);
    gloo_timers::callback::Timeout::new(0, move || {
        mounted.set(true);
    })
    .forget();

    view! {
        <>
            <h1 class="sr-only">"Chat Session"</h1>
            <Show when=move || mounted.try_get().unwrap_or(false) fallback=|| view! {
                <div class="chat-loading-placeholder" style="display:flex;align-items:center;justify-content:center;height:100%;opacity:0.5;">
                    <Spinner />
                </div>
            }>
                <ChatWorkspace selected_session_id=selected_id handle_query_params=true />
            </Show>
        </>
    }
}

// ---------------------------------------------------------------------------
// ChatWorkspace - two-column layout with session list + conversation
// ---------------------------------------------------------------------------

/// Chat workspace with session list sidebar (desktop) and conversation panel.
/// On mobile, session list is available via a slide-out overlay.
#[component]
fn ChatWorkspace(
    /// The currently selected session ID. None means no session selected.
    selected_session_id: Signal<Option<String>>,
    /// Whether to handle ?prompt= and ?adapter= query parameters
    #[prop(default = false)]
    handle_query_params: bool,
) -> impl IntoView {
    let is_compact = use_is_tablet_or_smaller();
    let show_mobile_sessions = RwSignal::new(false);
    let navigate = use_navigate();
    let sessions = RwSignal::new(ChatSessionsManager::load_sessions());
    let refresh_sessions = {
        Callback::new(move |_: ()| {
            sessions.set(ChatSessionsManager::load_sessions());
        })
    };

    // Derive a non-optional session ID for the conversation panel
    let session_id_for_panel =
        Signal::derive(move || selected_session_id.try_get().flatten().unwrap_or_default());
    let has_selection = Signal::derive(move || {
        selected_session_id
            .get()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    });

    // Refresh sessions list when selection changes (picks up auto-saved sessions)
    {
        Effect::new(move |_| {
            let _ = selected_session_id.try_get().flatten();
            sessions.set(ChatSessionsManager::load_sessions());
        });
    }

    // Handle session deletion
    let on_delete_session = {
        let navigate = navigate.clone();
        Callback::new(move |deleted_id: String| {
            ChatSessionsManager::delete_session(&deleted_id);
            sessions.set(ChatSessionsManager::load_sessions());
            // If deleted session was selected, go to /chat to auto-select next
            if selected_session_id.get_untracked().as_deref() == Some(deleted_id.as_str()) {
                navigate("/chat", Default::default());
            }
        })
    };

    // Close mobile overlay when a session is clicked (navigates via <a> href)
    {
        Effect::new(move |_| {
            let _ = selected_session_id.try_get().flatten();
            show_mobile_sessions.set(false);
        });
    }

    view! {
        <div class="flex h-full min-h-0">
            // Desktop: persistent session list sidebar
            {move || {
                if !is_compact.try_get().unwrap_or(false) {
                    Some(view! {
                        <div class="w-72 xl:w-80 border-r border-border flex-shrink-0 flex flex-col h-full overflow-hidden">
                            <SessionListPanel
                                selected_id=selected_session_id
                                sessions=sessions
                                on_delete=on_delete_session
                            />
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Conversation area
            <div class="flex-1 min-w-0 flex flex-col h-full">
                // Mobile: sessions toggle button
                {move || {
                    if is_compact.try_get().unwrap_or(false) {
                        Some(view! {
                            <div class="flex items-center gap-2 px-4 py-2 border-b border-border bg-background/80 shrink-0">
                                <button
                                    class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-border hover:bg-muted/50 transition-colors"
                                    on:click=move |_| show_mobile_sessions.update(|v| *v = !*v)
                                    aria-label="Toggle session list"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16"/>
                                    </svg>
                                    "Sessions"
                                </button>
                            </div>
                        })
                    } else {
                        None
                    }
                }}

                // Conversation panel or empty state.
                // IMPORTANT: Use <Show> instead of reactive `if` — a bare reactive
                // closure (`{move || if ... { view_a } else { view_b }}`) tears down
                // and rebuilds children on EVERY dependency re-emission, even when the
                // branch is unchanged.  Show caches and only swaps on actual changes.
                <div class="flex-1 min-h-0">
                    <Show
                        when=move || has_selection.try_get().unwrap_or(false)
                        fallback=|| view! { <ChatEmptyWorkspace/> }
                    >
                        <ChatConversationPanel
                            session_id_signal=session_id_for_panel
                            handle_query_params=handle_query_params
                            refresh_sessions=refresh_sessions
                        />
                    </Show>
                </div>
            </div>

            // Mobile: session list overlay (slide-in from left)
            {move || {
                if is_compact.try_get().unwrap_or(false) && show_mobile_sessions.try_get().unwrap_or(false) {
                    Some(view! {
                        <div
                            class="fixed inset-0 z-40 bg-background/80 backdrop-blur-sm"
                            on:click=move |_| show_mobile_sessions.set(false)
                        />
                        <div class="fixed inset-y-0 left-0 z-50 w-80 bg-background border-r border-border shadow-xl flex flex-col overflow-hidden">
                            <div class="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
                                <h2 class="text-sm font-semibold">"Sessions"</h2>
                                <button
                                    class="p-1.5 rounded hover:bg-muted/50 text-muted-foreground"
                                    on:click=move |_| show_mobile_sessions.set(false)
                                    aria-label="Close session list"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                    </svg>
                                </button>
                            </div>
                            <SessionListPanel
                                selected_id=selected_session_id
                                sessions=sessions
                                on_delete=on_delete_session
                            />
                        </div>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

/// Empty state shown when no session is selected in the workspace
#[component]
fn ChatEmptyWorkspace() -> impl IntoView {
    let navigate = use_navigate();
    let (_, chat_action) = use_chat();
    let create_session = {
        let navigate = navigate.clone();
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let navigate = navigate.clone();
            let action = action.clone();
            // Optimistic navigation: make the URL/session stable immediately, then swap to the
            // server-issued session id once created. This prevents "New Chat" from appearing
            // non-responsive during cold starts or transient backend delays.
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = format!("/chat/{}", placeholder_id);
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                // Create the session in the backend first; inference streaming requires
                // a server-issued session id.
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Chat".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        // Clean up placeholder (if untouched) before switching to the real id.
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let path = format!("/chat/{}", session_id);
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        web_sys::console::error_1(
                            &format!("[Chat] Failed to create backend session: {}", e).into(),
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };
    let go_to_training = {
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            navigate(
                "/training?open_wizard=1&return_to=/chat",
                Default::default(),
            );
        })
    };

    let go_to_adapters = {
        Callback::new(move |_: ()| {
            navigate("/adapters", Default::default());
        })
    };

    view! {
        <div class="flex h-full items-center justify-center p-6">
            <div class="text-center space-y-4 max-w-md">
                <div class="mx-auto w-14 h-14 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-7 w-7 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                    </svg>
                </div>
                <h3 class="heading-3">"Start a conversation"</h3>
                <p class="text-sm text-muted-foreground leading-relaxed">
                    "Create a new chat session to begin reasoning over your data with adaptive routing."
                </p>
                <div class="flex items-center justify-center gap-3">
                    <Button on_click=create_session>
                        "New Chat"
                    </Button>
                    <Button variant=ButtonVariant::Secondary on_click=go_to_training>
                        "Create Adapter"
                    </Button>
                </div>
                <p class="text-xs text-muted-foreground">
                    "or "
                    <a
                        href="/adapters"
                        class="underline hover:text-foreground transition-colors"
                        on:click=move |e: web_sys::MouseEvent| {
                            e.prevent_default();
                            go_to_adapters.run(());
                        }
                    >
                        "browse existing adapters"
                    </a>
                </p>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// SessionListPanel - left sidebar with session list, search, and actions
// ---------------------------------------------------------------------------

/// Session list panel for the workspace sidebar
#[component]
fn SessionListPanel(
    /// Currently selected session ID for highlighting
    selected_id: Signal<Option<String>>,
    /// All sessions (reactive)
    #[prop(into)]
    sessions: Signal<Vec<ChatSessionMeta>>,
    /// Callback when a session is deleted (passes deleted session ID)
    on_delete: Callback<String>,
) -> impl IntoView {
    let navigate = use_navigate();
    let search_query = RwSignal::new(String::new());
    let (chat_state, chat_action) = use_chat();

    // Filtered sessions based on search
    let filtered_sessions = Memo::new(move |_| {
        let query = search_query.try_get().unwrap_or_default().to_lowercase();
        let all = sessions.try_get().unwrap_or_default();
        if query.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|s| {
                    s.title.to_lowercase().contains(&query)
                        || s.preview.to_lowercase().contains(&query)
                })
                .collect()
        }
    });

    // Check if dock has unsaved messages
    let dock_has_messages =
        Memo::new(move |_| !chat_state.try_get().unwrap_or_default().messages.is_empty());
    let dock_message_count =
        Memo::new(move |_| chat_state.try_get().unwrap_or_default().messages.len());

    // Create new session
    let create_session = {
        let navigate = navigate.clone();
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let navigate = navigate.clone();
            let action = action.clone();
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = format!("/chat/{}", placeholder_id);
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Chat".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let path = format!("/chat/{}", session_id);
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        web_sys::console::error_1(
                            &format!("[Chat] Failed to create backend session: {}", e).into(),
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };

    // Save dock messages to a new session
    let save_dock_and_navigate = {
        let action = chat_action.clone();
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            let state = chat_state.get_untracked();
            let action = action.clone();
            let navigate = navigate.clone();
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = format!("/chat/{}", placeholder_id);
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Chat".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let session = ChatSessionsManager::session_from_state(&session_id, &state);
                        ChatSessionsManager::save_session(&session);
                        action.clear_messages();
                        let path = format!("/chat/{}", session_id);
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        web_sys::console::error_1(
                            &format!("[Chat] Failed to create backend session: {}", e).into(),
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };

    // Delete confirmation state
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let show_delete_confirm = RwSignal::new(false);

    let request_delete = move |id: String| {
        pending_delete_id.set(Some(id));
        show_delete_confirm.set(true);
    };

    let confirm_delete = move |_| {
        if let Some(id) = pending_delete_id.try_get().flatten() {
            on_delete.run(id);
        }
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    let cancel_delete = move |_| {
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    view! {
        <div class="flex flex-col h-full">
            // Header with search and new button
            <div class="p-3 space-y-2 border-b border-border shrink-0">
                <div class="flex items-center justify-between">
                    <h2 class="text-sm font-semibold">"Sessions"</h2>
                    <button
                        class="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                        on:click=move |_| create_session.run(())
                        title="New chat session"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
                        </svg>
                        "New"
                    </button>
                </div>
                <Input
                    value=search_query
                    placeholder="Search sessions...".to_string()
                />
            </div>

            // Continue from dock banner
            {move || {
                if dock_has_messages.try_get().unwrap_or(false) {
                    Some(view! {
                        <div class="px-3 py-2 border-b border-primary/20 bg-primary/5 shrink-0">
                            <div class="flex items-center justify-between gap-2">
                                <div class="min-w-0">
                                    <p class="text-xs font-medium truncate">"Continue conversation"</p>
                                    <p class="text-2xs text-muted-foreground">
                                        {move || format!("{} messages", dock_message_count.try_get().unwrap_or(0))}
                                    </p>
                                </div>
                                <button
                                    class="shrink-0 px-2 py-1 text-xs font-medium rounded border border-primary/30 text-primary hover:bg-primary/10 transition-colors"
                                    on:click=move |_| save_dock_and_navigate.run(())
                                >
                                    "Save & Open"
                                </button>
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Session list (scrollable)
            <div class="flex-1 overflow-y-auto">
                {move || {
                    let list = filtered_sessions.try_get().unwrap_or_default();
                    if list.is_empty() {
                        view! {
                            <div class="p-6 text-center">
                                <p class="text-xs text-muted-foreground">
                                    {move || if search_query.try_get().unwrap_or_default().is_empty() {
                                        "No sessions yet"
                                    } else {
                                        "No matching sessions"
                                    }}
                                </p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="divide-y divide-border">
                                {list.into_iter().map(|session| {
                                    let id = session.id.clone();
                                    let is_selected = {
                                        let id = id.clone();
                                        Signal::derive(move || selected_id.try_get().flatten().as_deref() == Some(id.as_str()))
                                    };
                                    let delete_id = id.clone();
                                    view! {
                                        <SessionListItem
                                            session=session
                                            selected=is_selected
                                            on_delete=Callback::new(move |_: ()| {
                                                request_delete(delete_id.clone());
                                            })
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Delete confirmation dialog
            <ConfirmationDialog
                open=show_delete_confirm
                title="Delete Session"
                description="Are you sure you want to delete this chat session? This action cannot be undone."
                severity=ConfirmationSeverity::Destructive
                confirm_text="Delete"
                on_confirm=Callback::new(confirm_delete)
                on_cancel=Callback::new(cancel_delete)
            />
        </div>
    }
}

/// Session list item in the workspace sidebar
#[component]
fn SessionListItem(
    session: ChatSessionMeta,
    /// Whether this session is currently selected
    #[prop(into)]
    selected: Signal<bool>,
    on_delete: Callback<()>,
) -> impl IntoView {
    let id = session.id.clone();
    let href = format!("/chat/{}", id);

    view! {
        <a
            href=href
            class=move || format!(
                "block group p-3 transition-colors {}",
                if selected.try_get().unwrap_or(false) {
                    "bg-primary/10 border-l-2 border-l-primary"
                } else {
                    "hover:bg-muted/50 border-l-2 border-l-transparent"
                }
            )
        >
            <div class="flex items-start justify-between gap-2">
                <div class="flex-1 min-w-0">
                    // Title
                    <h3 class="text-sm font-medium truncate">{session.title}</h3>

                    // Preview
                    {if !session.preview.is_empty() {
                        Some(view! {
                            <p class="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                                {session.preview}
                            </p>
                        })
                    } else {
                        None
                    }}

                    // Metadata
                    <div class="flex items-center gap-2 mt-1 text-2xs text-muted-foreground">
                        <span>{format_relative_time(&session.updated_at)}</span>
                        <span>"·"</span>
                        <span>{format!("{} msgs", session.message_count)}</span>
                    </div>
                </div>

                // Delete button (hover-revealed)
                <button
                    class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all shrink-0"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        on_delete.run(());
                    }
                    title="Delete session"
                    aria-label="Delete session"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                    </svg>
                </button>
            </div>
        </a>
    }
}

/// Format a timestamp as relative time
fn format_relative_time(timestamp: &str) -> String {
    use chrono::{DateTime, Utc};

    let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };

    let now = crate::utils::now_utc();
    let diff = now.signed_duration_since(dt.with_timezone(&Utc));

    if diff.num_minutes() < 1 {
        "Just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{} min ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{} hours ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{} days ago", diff.num_days())
    } else {
        dt.format("%b %d").to_string()
    }
}

fn generate_readable_id(_prefix: &str, _slug_source: &str) -> String {
    adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string()
}

/// Chat conversation panel - renders the full conversation experience for a session.
/// Used by both /chat and /chat/:session_id routes through the ChatWorkspace layout.
#[component]
fn ChatConversationPanel(
    /// Reactive session ID signal
    session_id_signal: Signal<String>,
    /// Whether to process ?prompt= and ?adapter= query parameters
    #[prop(default = false)]
    handle_query_params: bool,
    /// Callback to refresh the session list sidebar.
    refresh_sessions: Callback<()>,
) -> impl IntoView {
    let session_id = move || session_id_signal.try_get().unwrap_or_default();
    let session_label = move || {
        let id = session_id();
        if id.is_empty() {
            "unspecified".to_string()
        } else {
            id
        }
    };

    // Use global chat state
    let (chat_state, chat_action) = use_chat();
    let (system_status, _refetch_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });
    let status_center = use_status_center();

    // Local state for input and trace panel
    let message = RwSignal::new(String::new());
    let active_trace = RwSignal::new(Option::<String>::None);
    let session_loaded = RwSignal::new(false);
    let current_session_id = RwSignal::new(String::new());
    let session_error = RwSignal::new(Option::<String>::None);
    // Guard so deep-link query params are processed once per session ID.
    // This fixes in-app navigations like `/chat/<newid>?adapter=...` while already on /chat.
    let query_params_consumed_for_session = RwSignal::new(Option::<String>::None);

    // Auto-prune untouched placeholder sessions if the conversation panel unmounts.
    {
        on_cleanup(move || {
            let id = current_session_id.try_get_untracked().unwrap_or_default();
            if !id.is_empty() {
                ChatSessionsManager::prune_placeholder_session(&id);
                refresh_sessions.run(());
            }
        });
    }
    let verified_mode =
        Signal::derive(move || chat_state.try_get().unwrap_or_default().verified_mode);
    let show_attach_dialog = RwSignal::new(false);
    let attach_mode = RwSignal::new(AttachMode::Upload);
    let selected_file_name = RwSignal::new(Option::<String>::None);
    let selected_file = StoredValue::new_local(Option::<web_sys::File>::None);
    let attach_status = RwSignal::new(Option::<String>::None);
    let attach_error = RwSignal::new(Option::<String>::None);
    let attach_busy = RwSignal::new(false);
    let pasted_text = RwSignal::new(String::new());
    // Selected message indices for chat-to-dataset feature
    let selected_msg_indices = RwSignal::new(std::collections::HashSet::<usize>::new());
    // Cancellation signal to abort in-flight uploads when dialog is closed
    let upload_cancelled = RwSignal::new(false);
    #[cfg(target_arch = "wasm32")]
    let navigate = use_navigate();

    // Load session from localStorage when session ID changes
    // Tracks session_id() reactively to handle navigation between sessions
    {
        let action = chat_action.clone();
        Effect::new(move |prev_session_id: Option<String>| {
            let id = session_id();

            // Handle empty/invalid session ID - redirect to landing page
            if id.is_empty() {
                web_sys::console::warn_1(
                    &"[ChatSession] Empty session ID, redirecting to /chat".into(),
                );
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/chat");
                }
                return id;
            }

            // Skip if same session (effect re-ran for other reasons)
            if prev_session_id.as_ref() == Some(&id) {
                return id;
            }

            // Clear any existing messages from a different session before loading
            if let Some(ref prev) = prev_session_id {
                if !prev.is_empty() && prev != &id {
                    // Auto-prune untouched placeholder sessions when leaving.
                    ChatSessionsManager::prune_placeholder_session(prev);
                    refresh_sessions.run(());
                    action.clear_messages();
                }
            }

            // Validate session id before creating any placeholder state.
            if !ChatSessionsManager::is_valid_session_id(&id) {
                web_sys::console::warn_1(
                    &format!(
                        "[ChatSession] Invalid session ID '{}', redirecting to /chat",
                        id
                    )
                    .into(),
                );
                let navigate = use_navigate();
                navigate(
                    "/chat",
                    leptos_router::NavigateOptions {
                        replace: true,
                        ..Default::default()
                    },
                );
                return id;
            }

            current_session_id.set(id.clone());
            action.set_session_id(Some(id.clone()));
            session_loaded.set(false); // Reset for new session

            // Try to load session from localStorage
            if let Some(stored) = ChatSessionsManager::load_session(&id) {
                let msg_count = stored.messages.len();
                action.restore_session(stored);
                session_error.set(None);
                web_sys::console::log_1(
                    &format!("[Chat] Restored session {} with {} messages", id, msg_count).into(),
                );
            } else {
                // Session not found — create an empty placeholder so the URL is stable
                // and the session list can show it immediately.
                let placeholder = ChatSessionsManager::create_placeholder_session(&id);
                ChatSessionsManager::save_session(&placeholder);
                refresh_sessions.run(());
                action.restore_session(placeholder);
                session_error.set(None);
            }

            // Check for ?prompt= and ?adapter= query parameters once per session ID.
            if handle_query_params
                && query_params_consumed_for_session
                    .try_get_untracked()
                    .flatten()
                    .as_deref()
                    != Some(&id)
            {
                let mut consumed_any = false;
                #[cfg(target_arch = "wasm32")]
                let mut adapter_for_url: Option<String> = None;
                if let Some(window) = web_sys::window() {
                    if let Ok(search) = window.location().search() {
                        if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                            // Handle ?adapter= parameter - auto-pin the adapter
                            if let Some(adapter_id) = params.get("adapter") {
                                let decoded_adapter = js_sys::decode_uri_component(&adapter_id)
                                    .map(|s| s.as_string().unwrap_or_default())
                                    .unwrap_or(adapter_id);
                                if !decoded_adapter.is_empty() {
                                    let adapter = decoded_adapter;
                                    // Session-only pin (does not persist to localStorage)
                                    action.set_session_pinned_adapters(vec![adapter.clone()]);
                                    // Also set one-shot selected adapter so the first send definitely uses it.
                                    let Some(state) = chat_state.try_get_untracked() else {
                                        return id;
                                    };
                                    if state.selected_adapter.as_deref() != Some(adapter.as_str()) {
                                        action.select_next_adapter(&adapter);
                                    }
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        adapter_for_url = Some(adapter);
                                    }
                                    consumed_any = true;
                                }
                            }

                            // Handle ?prompt= parameter
                            if let Some(prompt) = params.get("prompt") {
                                let decoded = js_sys::decode_uri_component(&prompt)
                                    .map(|s| s.as_string().unwrap_or_default())
                                    .unwrap_or(prompt);
                                // Defense in depth: validate decoded prompt length
                                if decoded.len() > MAX_URL_PROMPT_LENGTH {
                                    web_sys::console::warn_1(
                                        &format!("Prompt parameter too long ({} bytes), rejecting for security", decoded.len()).into()
                                    );
                                    session_error.set(Some(format!(
                                        "Prompt too long ({} characters). Maximum is {} characters.",
                                        decoded.len(),
                                        MAX_URL_PROMPT_LENGTH
                                    )));
                                    return id;
                                }
                                if !decoded.is_empty() {
                                    action.send_message_streaming(decoded);
                                    consumed_any = true;
                                }
                            }
                        }
                    }
                }
                if consumed_any {
                    query_params_consumed_for_session.set(Some(id.clone()));
                    // If a prompt was consumed, drop it from the URL to avoid accidental re-send
                    // on refresh/back-button. Keep ?adapter= so a reload can re-apply session-only pins.
                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = use_navigate();
                        let mut path = format!("/chat/{}", id);
                        if let Some(adapter) = adapter_for_url {
                            let encoded = js_sys::encode_uri_component(&adapter)
                                .as_string()
                                .unwrap_or(adapter);
                            path = format!("{}?adapter={}", path, encoded);
                        }
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            }

            session_loaded.set(true);
            id
        });
    }

    // Auto-save session when messages change
    // Uses chat_state.try_get().unwrap_or_default() to create reactive dependency, then compares with previous state
    {
        Effect::new(move |prev_state: Option<(usize, bool, bool)>| {
            // Get state reactively to trigger effect when it changes
            let state = chat_state.try_get().unwrap_or_default();
            let msg_count = state.messages.len();
            let is_streaming = state.streaming;
            let verified_mode = state.verified_mode;
            // Get session ID untracked since we only care about state changes, not ID changes
            let id = current_session_id.try_get_untracked().unwrap_or_default();

            // Only save if:
            // 1. We have a session ID and messages
            // 2. Not currently streaming (wait for stream to complete)
            // 3. Message count changed OR streaming just stopped
            let should_save = !id.is_empty() && msg_count > 0 && !is_streaming;

            if should_save {
                if let Some((prev_count, was_streaming, prev_verified)) = prev_state {
                    // Save when message count changes, streaming just completed, or mode toggled
                    if msg_count != prev_count
                        || (was_streaming && !is_streaming)
                        || verified_mode != prev_verified
                    {
                        let session = ChatSessionsManager::session_from_state(&id, &state);
                        ChatSessionsManager::save_session(&session);
                        refresh_sessions.run(());
                        web_sys::console::log_1(
                            &format!("[Chat] Auto-saved session {} ({} messages)", id, msg_count)
                                .into(),
                        );
                    }
                }
            }

            (msg_count, is_streaming, verified_mode)
        });
    }

    // Reset attach dialog state when closed
    {
        Effect::new(move || {
            if !show_attach_dialog.try_get().unwrap_or(false) {
                // Signal cancellation to abort any in-flight uploads
                upload_cancelled.set(true);
                attach_mode.set(AttachMode::Upload);
                selected_file_name.set(None);
                selected_file.set_value(None);
                attach_status.set(None);
                attach_error.set(None);
                attach_busy.set(false);
                pasted_text.set(String::new());
                selected_msg_indices.set(std::collections::HashSet::new());
            }
        });
    }

    // Cleanup: Always cancel any pending stream when component unmounts
    {
        use leptos::prelude::on_cleanup;
        let action = chat_action.clone();
        on_cleanup(move || {
            // Always attempt to cancel to prevent stale updates after navigation
            action.cancel_stream();
            action.set_session_id(None);
        });
    }

    // Derived signals from global state - consolidated into single snapshot to avoid redundant subscriptions
    let chat_snapshot = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        (
            state.loading,
            state.streaming,
            state.error.clone(),
            state.stream_recovery.is_some(),
        )
    });

    let is_loading = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().0);
    let is_streaming = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().1);
    let is_busy = Signal::derive(move || {
        let (loading, streaming, _, _) = chat_snapshot.try_get().unwrap_or_default();
        loading || streaming
    });
    let can_send = Memo::new(move |_| {
        !message.try_get().unwrap_or_default().trim().is_empty()
            && !is_busy.try_get().unwrap_or(false)
    });
    let error = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().2);
    let can_retry = Signal::derive(move || {
        let (loading, streaming, _, has_recovery) = chat_snapshot.try_get().unwrap_or_default();
        !loading && !streaming && has_recovery
    });
    let retry_disabled = Signal::derive(move || !can_retry.try_get().unwrap_or(false));
    let base_model_label =
        Signal::derive(
            move || match chat_state.try_get().unwrap_or_default().target.clone() {
                ChatTarget::Model(name) => name,
                _ => "Default".to_string(),
            },
        );

    // Convert active_adapters to AdapterMagnets for the AdapterBar
    let adapter_magnets = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        let pinned = {
            let mut out = state.pinned_adapters.clone();
            for id in &state.session_pinned_adapters {
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
            out
        };
        state
            .active_adapters
            .iter()
            .map(|info| {
                let heat = match info.uses_per_minute {
                    n if n > 10 => AdapterHeat::Hot,
                    n if n > 0 => AdapterHeat::Warm,
                    _ => AdapterHeat::Cold,
                };
                AdapterMagnet {
                    adapter_id: info.adapter_id.clone(),
                    heat,
                    is_active: info.is_active,
                    is_pinned: pinned.contains(&info.adapter_id),
                }
            })
            .collect::<Vec<_>>()
    });

    // Pinned adapter IDs signal for ChatAdaptersRegion
    let pinned_adapters = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        let mut out = state.pinned_adapters.clone();
        for id in &state.session_pinned_adapters {
            if !out.contains(id) {
                out.push(id.clone());
            }
        }
        out
    });

    // Adapter selection pending flag (set on pin toggle, cleared on SSE update)
    let adapter_selection_pending = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .adapter_selection_pending
    });

    // Convert suggested_adapters for the SuggestedAdaptersBar
    // Name/purpose are populated from topology; other fields remain optional
    let suggested_adapters = Memo::new(move |_| {
        let selected = chat_state.try_get().unwrap_or_default().selected_adapter;
        chat_state
            .try_get()
            .unwrap_or_default()
            .suggested_adapters
            .iter()
            .map(|s| SuggestedAdapterView {
                adapter_id: s.adapter_id.clone(),
                display_name: s.name.clone().unwrap_or_else(|| s.adapter_id.clone()),
                confidence: s.confidence,
                is_pinned: s.is_pinned,
                is_selected: selected.as_deref() == Some(&s.adapter_id),
                // Use adapter name as description if available
                disabled_reason: None,
                description: s.purpose.clone(),
                tags: None,
            })
            .collect::<Vec<_>>()
    });

    // Debounce version counter for preview calls
    let preview_version = RwSignal::new(0u64);

    // Debounced effect to preview adapters when input changes
    {
        let action = chat_action.clone();
        Effect::new(move |_| {
            let text = message.try_get().unwrap_or_default();
            // Update version to invalidate pending previews
            preview_version.update(|v| *v += 1);
            let current_version = preview_version.try_get_untracked().unwrap_or(0);

            // Debounce: 300ms delay before calling preview
            let action = action.clone();
            set_timeout_simple(
                move || {
                    // Only proceed if this is still the latest version
                    // (bail if signal is disposed — component was unmounted)
                    let Some(v) = preview_version.try_get_untracked() else {
                        return;
                    };
                    if v != current_version {
                        return;
                    }
                    action.preview_adapters(text);
                },
                300,
            );
        });
    }

    // Toggle pin callback
    let on_toggle_pin = {
        let action = chat_action.clone();
        Callback::new(move |adapter_id: String| {
            action.toggle_pin_adapter(&adapter_id);
        })
    };

    // Select adapter for next message (one-shot override)
    let on_select_override = {
        let action = chat_action.clone();
        Callback::new(move |adapter_id: String| {
            action.select_next_adapter(&adapter_id);
        })
    };

    // Set full pinned adapter list (from manage dialog)
    let on_set_pinned = {
        let action = chat_action.clone();
        Callback::new(move |adapter_ids: Vec<String>| {
            action.set_pinned_adapters(adapter_ids);
        })
    };

    // Send message handler
    let do_send = {
        let action = chat_action.clone();
        move || {
            let msg = message.try_get().unwrap_or_default();
            if !msg.trim().is_empty() {
                message.set(String::new());
                action.send_message_streaming(msg);
            }
        }
    };

    // Keyboard handler for Enter-to-send (without Shift for newlines)
    let handle_keydown = {
        let do_send = do_send.clone();
        Callback::new(move |ev: web_sys::KeyboardEvent| {
            // Enter without Shift submits; Enter with Shift allows newline
            if ev.key() == "Enter" && !ev.shift_key() && can_send.try_get().unwrap_or(false) {
                ev.prevent_default();
                do_send();
            }
        })
    };

    // Cancel handler
    let do_cancel = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.cancel_stream();
        })
    };

    // Retry handler
    let do_retry = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.retry_last_stream();
        })
    };

    // Attach data -> dataset draft
    let create_draft = {
        #[cfg(target_arch = "wasm32")]
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            attach_error.set(None);
            let mode = attach_mode.try_get().unwrap_or(AttachMode::Upload);
            #[cfg(target_arch = "wasm32")]
            let base_model_param = {
                let base_model_id = match chat_state.try_get().unwrap_or_default().target.clone() {
                    ChatTarget::Model(name) => Some(name),
                    _ => None,
                };
                base_model_id
                    .as_ref()
                    .map(|val| {
                        let encoded = js_sys::encode_uri_component(val)
                            .as_string()
                            .unwrap_or_else(|| val.clone());
                        format!("&base_model_id={}", encoded)
                    })
                    .unwrap_or_default()
            };

            match mode {
                AttachMode::Upload => {
                    let Some(file) = selected_file.get_value() else {
                        attach_error.set(Some("Select a file to upload.".to_string()));
                        return;
                    };

                    let file_name = file.name();
                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some(format!("Uploading {}...", file_name)));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = navigate.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let base_model_param = base_model_param.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client.upload_document(&file).await {
                                Ok(doc) => {
                                    // Check cancellation before updating UI
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }

                                    // Show "already indexed" notice if document was deduplicated
                                    let info_suffix = if doc.deduplicated {
                                        " (already indexed)"
                                    } else {
                                        ""
                                    };
                                    attach_status.set(Some(format!(
                                        "Processing document{}...",
                                        info_suffix
                                    )));
                                    let doc_id = doc.document_id.clone();
                                    let mut chunk_count = doc.chunk_count.unwrap_or(0) as usize;

                                    for _ in 0..60 {
                                        // Check cancellation before each poll
                                        if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                            return;
                                        }
                                        gloo_timers::future::TimeoutFuture::new(1000).await;
                                        // Check cancellation after sleep
                                        if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                            return;
                                        }
                                        match client.get_document(&doc_id).await {
                                            Ok(status) => match status.status.as_str() {
                                                "indexed" => {
                                                    // Check cancellation before navigation
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    if let Some(count) = status.chunk_count {
                                                        chunk_count = count as usize;
                                                    }
                                                    let encoded_name =
                                                        js_sys::encode_uri_component(&file_name)
                                                            .as_string()
                                                            .unwrap_or_else(|| file_name.clone());
                                                    let path = format!(
                                                        "/datasets/draft?source=file&items={}&name={}&document_id={}{}",
                                                        chunk_count,
                                                        encoded_name,
                                                        doc_id,
                                                        base_model_param
                                                    );
                                                    navigate(&path, Default::default());
                                                    show_attach_dialog.set(false);
                                                    attach_busy.set(false);
                                                    attach_status.set(None);
                                                    return;
                                                }
                                                "failed" => {
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    attach_error.set(Some(format!(
                                                        "Document processing failed: {}",
                                                        status.error_message.unwrap_or_default()
                                                    )));
                                                    attach_busy.set(false);
                                                    attach_status.set(None);
                                                    return;
                                                }
                                                _ => {
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    attach_status.set(Some(format!(
                                                        "Processing document ({})...",
                                                        status.status
                                                    )));
                                                }
                                            },
                                            Err(e) => {
                                                if upload_cancelled
                                                    .try_get_untracked()
                                                    .unwrap_or(true)
                                                {
                                                    return;
                                                }
                                                attach_error.set(Some(format!(
                                                    "Failed to check status: {}",
                                                    e
                                                )));
                                                attach_busy.set(false);
                                                attach_status.set(None);
                                                return;
                                            }
                                        }
                                    }

                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error
                                        .set(Some("Document processing timed out.".to_string()));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error.set(Some(format!("Upload failed: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = file;
                        attach_error.set(Some(
                            "File upload is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
                AttachMode::Paste => {
                    let text = pasted_text.try_get().unwrap_or_default();
                    if text.trim().is_empty() {
                        attach_error.set(Some("Paste some text content first.".to_string()));
                        return;
                    }

                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some("Creating dataset from text...".to_string()));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = navigate.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let base_model_param = base_model_param.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client
                                .create_dataset_from_text(
                                    text,
                                    Some("pasted-text".to_string()),
                                    None,
                                )
                                .await
                            {
                                Ok(resp) => {
                                    // Check cancellation before navigation
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    let path = format!(
                                        "/datasets/draft?dataset_id={}{}",
                                        resp.dataset_id, base_model_param
                                    );
                                    navigate(&path, Default::default());
                                    show_attach_dialog.set(false);
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error
                                        .set(Some(format!("Failed to create dataset: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        attach_error.set(Some(
                            "Text dataset creation is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
                AttachMode::Chat => {
                    let indices = selected_msg_indices.try_get().unwrap_or_default();
                    if indices.is_empty() {
                        attach_error.set(Some("Select at least one message.".to_string()));
                        return;
                    }

                    let messages = chat_state.try_get().unwrap_or_default().messages;
                    let session_id = chat_state.try_get().unwrap_or_default().session_id.clone();

                    // Convert selected messages to ChatMessageInput format
                    let mut selected: Vec<(usize, ChatMessageInput)> = indices
                        .iter()
                        .filter_map(|&idx| {
                            messages.get(idx).map(|msg| {
                                (
                                    idx,
                                    ChatMessageInput {
                                        role: msg.role.clone(),
                                        content: msg.content.clone(),
                                        timestamp: Some(msg.timestamp.to_rfc3339()),
                                    },
                                )
                            })
                        })
                        .collect();
                    // Sort by index to preserve conversation order
                    selected.sort_by_key(|(idx, _)| *idx);
                    let chat_messages: Vec<ChatMessageInput> =
                        selected.into_iter().map(|(_, m)| m).collect();

                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some("Creating dataset from chat...".to_string()));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = navigate.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let base_model_param = base_model_param.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client
                                .create_dataset_from_chat(
                                    chat_messages,
                                    Some("chat-selection".to_string()),
                                    session_id,
                                )
                                .await
                            {
                                Ok(resp) => {
                                    // Check cancellation before navigation
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    let path = format!(
                                        "/datasets/draft?dataset_id={}{}",
                                        resp.dataset_id, base_model_param
                                    );
                                    navigate(&path, Default::default());
                                    show_attach_dialog.set(false);
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error
                                        .set(Some(format!("Failed to create dataset: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = (chat_messages, session_id);
                        attach_error.set(Some(
                            "Chat dataset creation is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
            }
        })
    };

    view! {
        <div class="p-6 flex h-full min-h-0 flex-col gap-4">
            // Header
            <div class="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-4">
                <div class="space-y-1">
                    <h2 class="heading-3">"Chat Session"</h2>
                    <div class="flex items-center gap-2 text-xs text-muted-foreground">
                        <span class="uppercase tracking-wider text-2xs font-medium">"Session"</span>
                        <span class="font-mono bg-muted/30 px-1.5 py-0.5 rounded text-2xs">{session_label}</span>
                    </div>
                </div>
                <div class="flex items-center gap-3">
                    // Target selector for choosing model, stack, or policy pack
                    <ChatTargetSelector/>
                    <Badge variant=BadgeVariant::Outline>
                        {move || format!("Base model: {}", base_model_label.try_get().unwrap_or_default())}
                    </Badge>
                    <div class="flex items-center rounded-full border border-border bg-muted/30 p-0.5 text-xs">
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            "px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        } else {
                                            "px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(false)
                                    type="button"
                                >
                                    "Fast"
                                </button>
                            }
                        }
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            "px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                        } else {
                                            "px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(true)
                                    type="button"
                                >
                                    "Verified"
                                </button>
                            }
                        }
                    </div>
                    // Status badge
                    {move || {
                        let err = error.try_get().flatten();
                        if err.is_some() {
                            view! {
                                <Badge variant=BadgeVariant::Destructive>"Error"</Badge>
                            }.into_any()
                        } else if is_loading.try_get().unwrap_or(false) {
                            // Waiting for first token
                            view! {
                                <Badge variant=BadgeVariant::Warning>"Connecting"</Badge>
                            }.into_any()
                        } else if is_streaming.try_get().unwrap_or(false) {
                            // Actively receiving tokens
                            view! {
                                <Badge variant=BadgeVariant::Success>"Streaming"</Badge>
                            }.into_any()
                        } else if chat_state
                            .try_get()
                            .unwrap_or_default()
                            .paused_inference
                            .is_some()
                        {
                            view! {
                                <Badge variant=BadgeVariant::Warning>"Paused"</Badge>
                            }.into_any()
                        } else {
                            view! {
                                <Badge variant=BadgeVariant::Secondary>"Ready"</Badge>
                            }.into_any()
                        }
                    }}
                </div>
            </div>

            // Stream status notice (transient info like "Waiting for server...", "Retrying...")
            // Warning/Error notices are shown in the error banner below for better UX
            {move || {
                chat_state.try_get().unwrap_or_default().stream_notice.clone().and_then(|notice| {
                    // Only show Info notices in header; Warning/Error go to error banner
                    if notice.tone != StreamNoticeTone::Info {
                        return None;
                    }
                    let message = notice.message.clone();
                    Some(view! {
                        <div class="flex items-center gap-3 text-xs" data-testid="chat-stream-status">
                            <Badge variant=BadgeVariant::Secondary>{message}</Badge>
                        </div>
                    })
                })
            }}

            // Pause notice + navigation to review flow
            {move || {
                let state = chat_state.try_get().unwrap_or_default();
                let pause = state.paused_inference.clone()?;
                let message = state
                    .stream_notice
                    .clone()
                    .map(|n| n.message)
                    .unwrap_or_else(|| "Paused: Awaiting review".to_string());
                let href_detail = format!("/reviews/{}", pause.pause_id);

                Some(view! {
                    <div class="flex items-center justify-between gap-3 text-xs" data-testid="chat-paused-notice">
                        <div class="flex items-center gap-2 min-w-0">
                            <Badge variant=BadgeVariant::Warning>"Paused"</Badge>
                            <span class="text-muted-foreground truncate">{message}</span>
                        </div>
                        <div class="flex items-center gap-3 flex-shrink-0">
                            <a href=href_detail class="text-xs font-medium text-primary hover:underline">
                                "Open Review"
                            </a>
                            <a href="/reviews" class="text-xs text-muted-foreground hover:text-primary">
                                "Queue"
                            </a>
                        </div>
                    </div>
                })
            }}

            // Unified Adapters Region: Active, Pinned, Suggested + Manage
            <ChatAdaptersRegion
                active_adapters=adapter_magnets
                pinned_adapters=pinned_adapters
                suggestions=suggested_adapters
                pending=adapter_selection_pending
                on_select_override=on_select_override
                on_toggle_pin=on_toggle_pin
                on_set_pinned=on_set_pinned
                loading=is_streaming
            />

            // Messages
            <div
                class="flex-1 overflow-y-auto rounded-lg border border-border bg-card"
                role="log"
                aria-live="polite"
                aria-label="Chat messages"
            >
                <div class="p-5">
                    {move || {
                        let msgs = chat_state.try_get().unwrap_or_default().messages;
                        if msgs.is_empty() {
                            view! {
                                <div class="flex h-full min-h-[200px] items-center justify-center py-12">
                                    <div class="text-center space-y-4 max-w-md px-4">
                                        // Conversation icon with gradient background
                                        <div class="mx-auto w-14 h-14 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-7 w-7 text-primary"
                                                fill="none"
                                                viewBox="0 0 24 24"
                                                stroke="currentColor"
                                                stroke-width="1.5"
                                            >
                                                <path
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
                                                />
                                            </svg>
                                        </div>
                                        <div class="space-y-2">
                                            <h3 class="heading-4 text-foreground">"What would you like to explore?"</h3>
                                            <p class="text-sm text-muted-foreground leading-relaxed">
                                                "Ask a question to begin. The system will automatically route your request to the best adapters for the task."
                                            </p>
                                        </div>
                                        // Suggestion chips
                                        <div class="flex flex-wrap justify-center gap-2 pt-2">
                                            <span class="text-xs px-3 py-1.5 rounded-full bg-muted text-muted-foreground">
                                                "Summarize a document"
                                            </span>
                                            <span class="text-xs px-3 py-1.5 rounded-full bg-muted text-muted-foreground">
                                                "Explain a concept"
                                            </span>
                                            <span class="text-xs px-3 py-1.5 rounded-full bg-muted text-muted-foreground">
                                                "Review code"
                                            </span>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-6">
                                    {msgs
                                        .into_iter()
                                        .map(|msg| {
                                            let is_user = msg.role == "user";
                                            let is_streaming = msg.is_streaming;
                                            let trace_id = msg.trace_id.clone();
                                            let latency_ms = msg.latency_ms;
                                            let token_count = msg.token_count;
                                            let prompt_tokens = msg.prompt_tokens;
                                            let completion_tokens = msg.completion_tokens;
                                            let role_label = if is_user { "You" } else { "Assistant" };
                                            view! {
                                                <div class=format!(
                                                    "flex {}",
                                                    if is_user { "justify-end" } else { "justify-start" }
                                                )>
                                                    <div class=format!(
                                                        "flex flex-col gap-1.5 max-w-[80%] {}",
                                                        if is_user { "items-end" } else { "items-start" }
                                                    )>
                                                        <span class="text-2xs uppercase tracking-wider font-medium text-muted-foreground px-1">
                                                            {role_label}
                                                        </span>
                                                        <div class=format!(
                                                            "rounded-lg px-4 py-3 {} {}",
                                                            if is_user {
                                                                "bg-primary text-primary-foreground shadow-sm"
                                                            } else {
                                                                "bg-muted/50 border border-border"
                                                            },
                                                            // Add min-height during streaming to prevent layout jump
                                                            if is_streaming { "min-h-[2.5rem]" } else { "" }
                                                        )>
                                                            <p class="text-sm whitespace-pre-wrap break-words leading-relaxed">
                                                                {msg.content.clone()}
                                                                {if is_streaming && !msg.content.is_empty() {
                                                                    // Pulsing cursor at end of streaming content
                                                                    view! {
                                                                        <span class="inline-block animate-pulse text-primary/70 ml-0.5">"▍"</span>
                                                                    }.into_any()
                                                                } else if is_streaming {
                                                                    // Spinner while waiting for first token
                                                                    view! {
                                                                        <span class="inline-flex items-center gap-1.5 text-muted-foreground">
                                                                            <Spinner/>
                                                                            <span class="text-xs">"Routing..."</span>
                                                                        </span>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <span></span> }.into_any()
                                                                }}
                                                            </p>
                                                        </div>
                                                        // Run/Receipt links for assistant messages (placeholder if trace unavailable)
                                                        {if !is_user && !is_streaming {
                                                            let latency = latency_ms.unwrap_or(0);
                                                            let trace = trace_id.clone();
                                                            let run_overview_url = trace.clone().map(|tid| format!("/runs/{}", tid));
                                                            let run_receipt_url = trace.clone().map(|tid| format!("/runs/{}?tab=receipt", tid));
                                                            Some(view! {
                                                                <div class="flex items-center gap-3 mt-1 px-1 flex-wrap" data-testid="chat-trace-links">
                                                                    {trace.clone().map(|tid| view! {
                                                                        <TraceButton
                                                                            trace_id=tid.clone()
                                                                            latency_ms=latency
                                                                            on_click=Callback::new(move |id: String| {
                                                                                active_trace.set(Some(id));
                                                                            })
                                                                            data_testid="chat-trace-link".to_string()
                                                                        />
                                                                    })}
                                                                    <div class="flex items-center gap-1">
                                                                        {run_overview_url.map(|url| view! {
                                                                            <a
                                                                                href=url
                                                                                class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                                                title="View Run Detail"
                                                                                data-testid="chat-run-link"
                                                                            >
                                                                                "Run"
                                                                            </a>
                                                                        }.into_any()).unwrap_or_else(|| view! {
                                                                            <span
                                                                                class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                                                title="Run detail unavailable"
                                                                                data-testid="chat-run-link"
                                                                            >
                                                                                "Run"
                                                                            </span>
                                                                        }.into_any())}
                                                                        <span class="text-muted-foreground/50">"·"</span>
                                                                        {run_receipt_url.map(|url| view! {
                                                                            <a
                                                                                href=url
                                                                                class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                                                title="View Receipt"
                                                                                data-testid="chat-receipt-link"
                                                                            >
                                                                                "Receipt"
                                                                            </a>
                                                                        }.into_any()).unwrap_or_else(|| view! {
                                                                            <span
                                                                                class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                                                title="Receipt unavailable"
                                                                                data-testid="chat-receipt-link"
                                                                            >
                                                                                "Receipt"
                                                                            </span>
                                                                        }.into_any())}
                                                                    </div>
                                                                    {token_count.map(|tc| {
                                                                        let display = format_token_display(tc, prompt_tokens, completion_tokens);
                                                                        view! {
                                                                            <span class="text-xs text-muted-foreground">
                                                                                {display}
                                                                            </span>
                                                                        }
                                                                    })}
                                                                </div>
                                                            })
                                                        } else {
                                                            None
                                                        }}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect::<Vec<_>>()}

                                    // Inline error indicator after messages (provides context)
                                    {move || {
                                        let state = chat_state.try_get().unwrap_or_default();
                                        let has_error = state.error.is_some();
                                        let notice = state.stream_notice.clone();
                                        let has_recovery = state.stream_recovery.is_some();

                                        if has_error {
                                            let display_msg = notice.as_ref()
                                                .map(|n| n.message.clone())
                                                .unwrap_or_else(|| "Something went wrong".to_string());

                                            let retryable = notice.as_ref()
                                                .map(|n| n.retryable)
                                                .unwrap_or(false);

                                            let is_warning = notice.as_ref()
                                                .map(|n| n.tone == StreamNoticeTone::Warning)
                                                .unwrap_or(false);

                                            let (icon_color, bg_color) = if is_warning {
                                                ("text-warning", "bg-warning/5 border-warning/20")
                                            } else {
                                                ("text-destructive", "bg-destructive/5 border-destructive/20")
                                            };

                                            // Contextual help based on error type
                                            let help_hint = notice.as_ref().and_then(|n| {
                                                match n.message.as_str() {
                                                    "Server is busy" => Some("The server is processing many requests. Retrying usually helps."),
                                                    "No workers available" => Some("All inference workers are busy. Try again in a moment."),
                                                    "Connection lost" => Some("Check your network connection and try again."),
                                                    "Request already in progress" => Some("Wait for the current request to finish."),
                                                    "Session expired" => Some("You need to log in again to continue."),
                                                    "Access denied" => Some("You don't have permission for this action."),
                                                    "Too many requests" => Some("Slow down and try again in a moment."),
                                                    "Service temporarily unavailable" => Some("The service is temporarily down. Retrying usually helps."),
                                                    _ => None,
                                                }
                                            });

                                            // Determine action hint: retryable needs recovery state to actually work
                                            let action_hint = if retryable && has_recovery {
                                                Some("Click Retry above to try again.")
                                            } else if !retryable {
                                                Some("Dismiss to send a new message.")
                                            } else {
                                                None
                                            };

                                            Some(view! {
                                                <div
                                                    class=format!("flex items-start gap-3 mt-3 p-3 rounded-lg border {}", bg_color)
                                                    data-testid="chat-inline-error"
                                                    role="status"
                                                    aria-live="polite"
                                                >
                                                    // Error icon
                                                    <svg
                                                        xmlns="http://www.w3.org/2000/svg"
                                                        class=format!("h-4 w-4 flex-shrink-0 mt-0.5 {}", icon_color)
                                                        fill="none"
                                                        viewBox="0 0 24 24"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        aria-hidden="true"
                                                    >
                                                        <path
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                                                        />
                                                    </svg>
                                                    <div class="flex-1 min-w-0 space-y-1">
                                                        <p class="text-sm font-medium text-foreground">
                                                            {display_msg}
                                                        </p>
                                                        {help_hint.map(|hint| view! {
                                                            <p class="text-xs text-muted-foreground">{hint}</p>
                                                        })}
                                                        {action_hint.map(|hint| view! {
                                                            <p class="text-xs text-muted-foreground/70">{hint}</p>
                                                        })}
                                                    </div>
                                                </div>
                                            })
                                        } else {
                                            None
                                        }
                                    }}
                                </div>
                            }.into_any()
                        }
                    }}
                </div>
            </div>

            // Trace panel (modal overlay)
            {move || {
                active_trace.try_get().flatten().map(|tid| {
                    view! {
                        <TracePanel
                            trace_id=tid.clone()
                            on_close=Callback::new(move |_| {
                                active_trace.set(None);
                            })
                        />
                    }
                })
            }}

            // Session error display (stale/missing session)
            {move || {
                session_error.try_get().flatten().map(|e| view! {
                    <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4">
                        <div class="flex items-center justify-between gap-2">
                            <p class="text-sm text-warning-foreground">{e}</p>
                            <a
                                href="/chat"
                                class="text-sm font-medium text-primary hover:underline"
                            >
                                "Start New Session"
                            </a>
                        </div>
                    </div>
                })
            }}

            // Error display with dismiss button
            // Uses stream_notice.message for human-readable copy, falls back to raw error
            // Retry button only appears when error is retryable AND recovery state exists
            {move || {
                let action = chat_action.clone();
                let state = chat_state.try_get().unwrap_or_default();
                let notice = state.stream_notice.clone();
                let raw_error = state.error.clone();
                let has_recovery = state.stream_recovery.is_some();

                // Only show if there's an error
                raw_error.map(|raw| {
                    // Use human-readable notice message if available, else raw error
                    let display_msg = notice.as_ref()
                        .map(|n| n.message.clone())
                        .unwrap_or_else(|| raw.clone());

                    let retryable = notice.as_ref()
                        .map(|n| n.retryable)
                        .unwrap_or(false);

                    // Only show retry when both retryable flag is true AND recovery state exists
                    // This prevents showing retry when the recovery context has been cleared
                    let show_retry = retryable && has_recovery;

                    let is_warning = notice.as_ref()
                        .map(|n| n.tone == StreamNoticeTone::Warning)
                        .unwrap_or(false);

                    // Style based on tone: Warning = amber, Error = red
                    let (border_class, bg_class, text_class) = if is_warning {
                        ("border-warning", "bg-warning/10", "text-warning-foreground")
                    } else {
                        ("border-destructive", "bg-destructive/10", "text-destructive")
                    };

                    // Contextual help text based on error type (aligned with inline error hints)
                    let help_text = notice.as_ref().and_then(|n| {
                        match n.message.as_str() {
                            "Server is busy" => Some("The server is processing many requests. Retrying usually helps."),
                            "No workers available" => Some("All inference workers are busy. Try again in a moment."),
                            "Connection lost" => Some("Check your network connection and try again."),
                            "Request already in progress" => Some("Wait for the current request to finish."),
                            "Session expired" => Some("You need to log in again to continue."),
                            "Access denied" => Some("You don't have permission for this action."),
                            "Too many requests" => Some("Slow down and try again in a moment."),
                            "Service temporarily unavailable" => Some("The service is temporarily down. Retrying usually helps."),
                            _ => None,
                        }
                    });

                    view! {
                        <div
                            class=format!("mb-4 rounded-md border {} {} p-3 text-sm", border_class, bg_class)
                            role="alert"
                            data-testid="chat-error-banner"
                        >
                            <div class="flex flex-col gap-2">
                                <div class="flex items-center justify-between gap-2">
                                    <div class="flex flex-col gap-0.5">
                                        <p class=format!("font-medium {}", text_class)>{display_msg}</p>
                                        {help_text.map(|ht| view! {
                                            <p class="text-xs text-muted-foreground">{ht}</p>
                                        })}
                                    </div>
                                    <div class="flex items-center gap-2 flex-shrink-0">
                                        {if show_retry {
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    disabled=retry_disabled
                                                    on_click=do_retry
                                                >
                                                    "Retry"
                                                </Button>
                                            }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }}
                                        <button
                                            class="text-sm font-medium text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-colors"
                                            on:click=move |_| action.clear_error()
                                            aria-label="Dismiss error"
                                        >
                                            "Dismiss"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    }
                })
            }}

            // Inference readiness banner
            {move || {
                match system_status.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Loaded(status) => {
                        if matches!(status.inference_ready, InferenceReadyState::True) {
                            view! {}.into_any()
                        } else {
                            let guidance = guidance_for(
                                status.inference_ready,
                                status.inference_blockers.first(),
                            );
                            let action = guidance.action;
                            view! {
                                <div class="rounded-md border border-warning/40 bg-warning/10 p-3 text-sm">
                                    <div class="flex flex-wrap items-start justify-between gap-3">
                                        <div>
                                            <p class="font-medium text-warning-foreground">"Inference isn't ready"</p>
                                            <p class="text-xs text-muted-foreground">
                                                {format!("{}.", guidance.reason)}
                                            </p>
                                        </div>
                                        <div class="flex items-center gap-2">
                                            <a href=action.href class="btn btn-outline btn-sm">
                                                {action.label}
                                            </a>
                                            {status_center.map(|ctx| view! {
                                                    <button
                                                        class="text-xs text-muted-foreground hover:text-foreground"
                                                        on:click=move |_| ctx.open()
                                                    >
                                                        "Why?"
                                                    </button>
                                                })}
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }
                    _ => view! {}.into_any(),
                }
            }}

            // Input
            <div class="border-t border-border pt-4">
                <form
                    class="flex items-end gap-3"
                    on:submit=move |ev: web_sys::SubmitEvent| {
                        ev.prevent_default();
                        if can_send.try_get().unwrap_or(false) {
                            do_send();
                        }
                    }
                >
                    <button
                        type="button"
                        class="btn btn-outline btn-sm"
                        on:click=move |_| show_attach_dialog.set(true)
                    >
                        "Attach data"
                    </button>
                    <Textarea
                        value=message
                        placeholder="Type your message...".to_string()
                        class="flex-1".to_string()
                        rows=2
                        aria_label="Chat message input".to_string()
                        data_testid="chat-input".to_string()
                        on_keydown=handle_keydown
                    />
                    {move || {
                        if is_streaming.try_get().unwrap_or(false) {
                            view! {
                                <Button
                                    on_click=do_cancel
                                    class="bg-destructive hover:bg-destructive/90".to_string()
                                    aria_label="Stop streaming".to_string()
                                >
                                    "Stop"
                                </Button>
                            }.into_any()
                        } else {
                            let disabled = !can_send.try_get().unwrap_or(false);
                            view! {
                                <Button
                                    loading=is_loading.try_get().unwrap_or(false)
                                    disabled=disabled
                                    aria_label=if disabled { "Send message (disabled)".to_string() } else { "Send message".to_string() }
                                    data_testid="chat-send".to_string()
                                >
                                    "Send"
                                </Button>
                            }.into_any()
                        }
                    }}
                </form>
            </div>

            <Dialog
                open=show_attach_dialog
                title="Attach data".to_string()
                description="Create a dataset draft from a file, pasted text, or this chat.".to_string()
            >
                <div class="space-y-4">
                    <div class="grid grid-cols-3 gap-2 text-xs">
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Upload {
                                    "rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Upload)
                        >
                            "Upload file"
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Paste {
                                    "rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Paste)
                        >
                            "Paste text"
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Chat {
                                    "rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Chat)
                        >
                            "Use this chat"
                        </button>
                    </div>

                    {move || match attach_mode.try_get().unwrap_or(AttachMode::Upload) {
                        AttachMode::Upload => view! {
                            <div class="space-y-2">
                                <label class="text-xs text-muted-foreground">"Select a file"</label>
                                <input
                                    type="file"
                                    class="block w-full text-xs text-muted-foreground file:mr-3 file:rounded-md file:border-0 file:bg-muted file:px-3 file:py-2 file:text-xs file:font-medium file:text-foreground hover:file:bg-muted/70"
                                    accept=".pdf,.txt,.md"
                                    on:change=move |ev| {
                                        let file = selected_file_from_event(&ev);
                                        let name = file.as_ref().map(|f| f.name());
                                        selected_file_name.set(name);
                                        selected_file.set_value(file);
                                    }
                                />
                                {move || selected_file_name.try_get().flatten().map(|name| view! {
                                    <div class="text-xs text-muted-foreground">
                                        {format!("Selected: {}", name)}
                                    </div>
                                })}
                            </div>
                        }.into_any(),
                        AttachMode::Paste => view! {
                            <div class="space-y-2">
                                <label class="text-xs text-muted-foreground">"Paste text"</label>
                                <Textarea
                                    value=pasted_text
                                    placeholder="Paste training examples or notes...".to_string()
                                    rows=5
                                    class="w-full".to_string()
                                    aria_label="Paste dataset text".to_string()
                                />
                            </div>
                        }.into_any(),
                        AttachMode::Chat => {
                            let messages = chat_state.try_get().unwrap_or_default().messages;
                            let msg_count = messages.len();
                            let selected_count = Memo::new(move |_| selected_msg_indices.try_get().unwrap_or_default().len());

                            // Quick select: last N messages
                            let select_last_n = move |n: usize| {
                                let msgs = chat_state.try_get().unwrap_or_default().messages;
                                let total = msgs.len();
                                let start = total.saturating_sub(n);
                                let indices: std::collections::HashSet<usize> = (start..total).collect();
                                selected_msg_indices.set(indices);
                            };

                            // Toggle all
                            let toggle_all = move |_| {
                                let current = selected_msg_indices.try_get().unwrap_or_default();
                                let total = chat_state.try_get().unwrap_or_default().messages.len();
                                if current.len() == total {
                                    selected_msg_indices.set(std::collections::HashSet::new());
                                } else {
                                    selected_msg_indices.set((0..total).collect());
                                }
                            };

                            view! {
                                <div class="space-y-3">
                                    <div class="flex items-center justify-between">
                                        <label class="text-xs text-muted-foreground">"Select messages"</label>
                                        <span class="text-xs text-muted-foreground">
                                            {move || format!("{} of {} selected", selected_count.try_get().unwrap_or(0), chat_state.try_get().unwrap_or_default().messages.len())}
                                        </span>
                                    </div>

                                    // Quick actions
                                    <div class="flex gap-2 flex-wrap">
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=toggle_all
                                        >
                                            {move || if selected_msg_indices.try_get().unwrap_or_default().len() == chat_state.try_get().unwrap_or_default().messages.len() && !chat_state.try_get().unwrap_or_default().messages.is_empty() {
                                                "Deselect all"
                                            } else {
                                                "Select all"
                                            }}
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_n(5)
                                        >
                                            "Last 5"
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_n(10)
                                        >
                                            "Last 10"
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_n(20)
                                        >
                                            "Last 20"
                                        </button>
                                    </div>

                                    // Message list with checkboxes
                                    {if msg_count == 0 {
                                        view! {
                                            <p class="text-xs text-muted-foreground py-4 text-center">
                                                "No messages in this chat session."
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="max-h-48 overflow-y-auto border border-border rounded-md">
                                                {messages.into_iter().enumerate().map(|(idx, msg)| {
                                                    let is_checked = Memo::new(move |_| selected_msg_indices.try_get().unwrap_or_default().contains(&idx));
                                                    let role_badge = if msg.role == "user" { "U" } else { "A" };
                                                    let content_preview: String = msg.content.chars().take(60).collect::<String>()
                                                        + if msg.content.len() > 60 { "..." } else { "" };
                                                    let toggle_msg = move |checked: bool| {
                                                        selected_msg_indices.update(|set| {
                                                            if checked {
                                                                set.insert(idx);
                                                            } else {
                                                                set.remove(&idx);
                                                            }
                                                        });
                                                    };
                                                    view! {
                                                        <div class="flex items-start gap-2 px-3 py-2 border-b border-border/50 last:border-b-0 hover:bg-muted/30">
                                                            <Checkbox
                                                                checked=Signal::derive(move || is_checked.try_get().unwrap_or(false))
                                                                on_change=Callback::new(toggle_msg)
                                                                aria_label=format!("Select message {}", idx + 1)
                                                            />
                                                            <span class=move || format!(
                                                                "shrink-0 w-5 h-5 rounded text-xs flex items-center justify-center {}",
                                                                if msg.role == "user" { "bg-primary/20 text-primary" } else { "bg-muted text-muted-foreground" }
                                                            )>
                                                                {role_badge}
                                                            </span>
                                                            <span class="text-xs text-foreground/80 line-clamp-2 flex-1">
                                                                {content_preview}
                                                            </span>
                                                        </div>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        },
                    }}

                    {move || attach_error.try_get().flatten().map(|msg| view! {
                        <div class="text-xs text-destructive">{msg}</div>
                    })}
                    {move || attach_status.try_get().flatten().map(|msg| view! {
                        <div class="text-xs text-muted-foreground">{msg}</div>
                    })}

                    <div class="flex justify-end gap-2 pt-2 border-t border-border">
                        <Button
                            variant=ButtonVariant::Outline
                            disabled=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            on_click=Callback::new(move |_| show_attach_dialog.set(false))
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            loading=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            disabled=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            on_click=create_draft
                        >
                            "Create draft"
                        </Button>
                    </div>
                </div>
            </Dialog>
        </div>
    }
}

/// Format token display with breakdown if available
fn format_token_display(
    total: u32,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
) -> String {
    match (prompt_tokens, completion_tokens) {
        (Some(prompt), Some(completion)) => {
            format!(
                "{} tokens ({} prompt, {} completion)",
                total, prompt, completion
            )
        }
        _ => format!("{} tokens", total),
    }
}

fn selected_file_from_event(ev: &web_sys::Event) -> Option<web_sys::File> {
    let target = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())?;
    let files = target.files()?;
    files.get(0)
}

/// Target options fetched from API for the chat target selector
#[derive(Debug, Clone, Default)]
struct TargetOptions {
    models: Vec<(String, String)>,   // (id, name)
    stacks: Vec<(String, String)>,   // (id, name)
    policies: Vec<(String, String)>, // (cpid, display_name)
    loading: bool,
    error: Option<String>,
}

/// Chat target selector component for choosing model, stack, or policy pack
#[component]
fn ChatTargetSelector() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let show_dropdown = RwSignal::new(false);
    let options = RwSignal::new(TargetOptions::default());
    let has_loaded = RwSignal::new(false);

    let toggle_dropdown = move |_| {
        show_dropdown.update(|v| *v = !*v);
    };

    let select_target = {
        let action = chat_action.clone();
        move |target: ChatTarget| {
            action.set_target(target);
            show_dropdown.set(false);
        }
    };

    // Reset has_loaded when dropdown closes to allow refresh on next open
    Effect::new(move |prev_open: Option<bool>| {
        let is_open = show_dropdown.try_get().unwrap_or(false);
        if let Some(was_open) = prev_open {
            if was_open && !is_open {
                has_loaded.set(false);
            }
        }
        is_open
    });

    // Fetch options when dropdown is first opened
    Effect::new(move || {
        if show_dropdown.try_get().unwrap_or(false) && !has_loaded.try_get().unwrap_or(false) {
            has_loaded.set(true);
            options.update(|o| {
                o.loading = true;
                o.error = None;
            });

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());

                // Fetch all in parallel
                let models_fut = client.list_models();
                let stacks_fut = client.list_stacks();
                let policies_fut = client.list_policies();

                let (models_res, stacks_res, policies_res) =
                    futures::join!(models_fut, stacks_fut, policies_fut);

                let mut errors: Vec<String> = Vec::new();

                options.update(|o| {
                    o.loading = false;

                    // Parse models
                    match models_res {
                        Ok(resp) => {
                            o.models = resp
                                .models
                                .into_iter()
                                .map(|m| (m.id.clone(), m.name.clone()))
                                .collect();
                        }
                        Err(e) => {
                            let msg = format!("Models: {}", e);
                            web_sys::console::warn_1(&msg.clone().into());
                            errors.push(msg);
                        }
                    }

                    // Parse stacks
                    match stacks_res {
                        Ok(stacks) => {
                            o.stacks = stacks
                                .into_iter()
                                .filter(|s| s.is_active)
                                .map(|s| (s.id.clone(), s.name.clone()))
                                .collect();
                        }
                        Err(e) => {
                            let msg = format!("Stacks: {}", e);
                            web_sys::console::warn_1(&msg.clone().into());
                            errors.push(msg);
                        }
                    }

                    // Parse policies - extract display name from CPID
                    match policies_res {
                        Ok(policies) => {
                            o.policies = policies
                                .into_iter()
                                .map(|p| {
                                    let display = p
                                        .cpid
                                        .replace('-', " ")
                                        .split_whitespace()
                                        .map(|w| {
                                            let mut chars = w.chars();
                                            match chars.next() {
                                                Some(first) => {
                                                    first.to_uppercase().chain(chars).collect()
                                                }
                                                None => String::new(),
                                            }
                                        })
                                        .collect::<Vec<String>>()
                                        .join(" ");
                                    (p.cpid, display)
                                })
                                .collect();
                        }
                        Err(e) => {
                            let msg = format!("Policies: {}", e);
                            web_sys::console::warn_1(&msg.clone().into());
                            errors.push(msg);
                        }
                    }

                    if !errors.is_empty() {
                        o.error = Some(format!("Failed to load: {}", errors.join(", ")));
                    }
                });
            });
        }
    });

    view! {
        <div class="relative">
            <button
                class="flex items-center gap-2 rounded-md border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted transition-colors"
                on:click=toggle_dropdown
                data-testid="chat-target-selector"
            >
                <span class="text-muted-foreground text-xs">"Target:"</span>
                <span class="font-medium truncate max-w-[150px]">{move || chat_state.try_get().unwrap_or_default().target.display_name()}</span>
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-4 w-4 text-muted-foreground flex-shrink-0"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
                </svg>
            </button>

            // Dropdown menu
            {move || {
                if show_dropdown.try_get().unwrap_or(false) {
                    let select = select_target.clone();
                    let opts = options.try_get().unwrap_or_default();

                    view! {
                        <div
                            class="absolute left-0 top-full z-50 mt-1 min-w-[200px] rounded-md border border-border bg-popover shadow-lg max-h-80 overflow-y-auto"
                            data-testid="chat-target-dropdown"
                        >
                            <div class="p-1">
                                <TargetOption
                                    target=ChatTarget::Default
                                    label="Default".to_string()
                                    on_select=select.clone()
                                />

                                // Error display
                                {opts.error.as_ref().map(|e| view! {
                                    <div class="px-2 py-2 text-xs text-destructive bg-destructive/10 rounded mx-1 my-1">
                                        {e.clone()}
                                    </div>
                                })}

                                // Loading indicator
                                {if opts.loading {
                                    Some(view! {
                                        <div class="px-2 py-3 text-center text-sm text-muted-foreground">
                                            <span class="animate-pulse">"Loading options..."</span>
                                        </div>
                                    })
                                } else {
                                    None
                                }}

                                // Models section
                                {if !opts.models.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t border-border"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Models"</div>
                                        {opts.models.iter().map(|(id, name)| {
                                            let target = ChatTarget::Model(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <TargetOption
                                                    target=target
                                                    label=label
                                                    on_select=select
                                                />
                                            }
                                        }).collect::<Vec<_>>()}
                                    })
                                } else {
                                    None
                                }}

                                // Stacks section
                                {if !opts.stacks.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t border-border"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Stacks"</div>
                                        {opts.stacks.iter().map(|(id, name)| {
                                            let target = ChatTarget::Stack(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <TargetOption
                                                    target=target
                                                    label=label
                                                    on_select=select
                                                />
                                            }
                                        }).collect::<Vec<_>>()}
                                    })
                                } else {
                                    None
                                }}

                                // Policy Packs section
                                {if !opts.policies.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t border-border"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Policy Packs"</div>
                                        {opts.policies.iter().map(|(id, name)| {
                                            let target = ChatTarget::PolicyPack(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <TargetOption
                                                    target=target
                                                    label=label
                                                    on_select=select
                                                />
                                            }
                                        }).collect::<Vec<_>>()}
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

/// Individual target option in the dropdown
#[component]
fn TargetOption<F>(target: ChatTarget, label: String, on_select: F) -> impl IntoView
where
    F: Fn(ChatTarget) + Clone + 'static,
{
    let target_clone = target.clone();
    let select = on_select.clone();

    view! {
        <button
            class="flex w-full items-center rounded-sm px-2 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground transition-colors text-left"
            on:click=move |_| {
                select(target_clone.clone());
            }
        >
            {label}
        </button>
    }
}

/// Simple setTimeout wrapper for debouncing
#[cfg(target_arch = "wasm32")]
fn set_timeout_simple<F: FnOnce() + 'static>(f: F, ms: i32) {
    use wasm_bindgen::prelude::*;

    if let Some(window) = web_sys::window() {
        let closure = Closure::once(f);
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            ms,
        );
        closure.forget();
    } else {
        tracing::error!("set_timeout_simple: no window object available");
    }
}

/// Non-WASM stub (for tests)
#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(f: F, _ms: i32) {
    // Non-WASM: run immediately; debounce disabled in tests/SSR.
    f();
}
