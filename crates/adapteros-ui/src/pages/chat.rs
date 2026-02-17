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

use crate::api::types::{
    AssignTagsRequest, CreateChatTagRequest, ForkSessionRequest, ShareSessionRequest,
};
use crate::api::{api_base_url, report_error_with_toast, ApiClient};
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::status_center::use_status_center;
use crate::components::{
    use_is_tablet_or_smaller, AdapterHeat, AdapterMagnet, Badge, BadgeVariant, Button, ButtonLink,
    ButtonSize, ButtonType, ButtonVariant, ChatAdaptersRegion, Checkbox, ConfirmationDialog,
    ConfirmationSeverity, Dialog, Input, Markdown, Spinner, SuggestedAdapterView, Textarea,
    TraceButton, TracePanel,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{
    use_chat, use_settings, ChatSessionMeta, ChatSessionsManager, ChatTarget, StreamNoticeTone,
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
const DOCUMENT_UPLOAD_MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
const DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".markdown"];
const MAX_CHAT_DATASET_MESSAGES: usize = 10_000;

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
    let recent_session_id = sessions.first().map(|session| session.id.clone());
    let has_recent_session = recent_session_id.is_some();
    let (system_status, refetch_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });
    let refetch_status_signal = StoredValue::new(refetch_status);
    let redirected_to_recent = RwSignal::new(false);
    let retry_status = Callback::new(move |_: ()| {
        let _ = refetch_status_signal.try_with_value(|f| f.run(()));
    });

    // Redirect to the most recent session only when inference is ready.
    {
        let navigate = navigate.clone();
        let recent_session_id = recent_session_id.clone();
        Effect::new(move |_| {
            if redirected_to_recent.try_get().unwrap_or(false) {
                return;
            }
            let status = system_status.try_get().unwrap_or(LoadingState::Idle);
            let ready = matches!(status, LoadingState::Loaded(ref s) if matches!(s.inference_ready, InferenceReadyState::True));
            if !ready {
                return;
            }
            let Some(recent_id) = recent_session_id.clone() else {
                return;
            };

            // Preserve query params (?prompt=, ?adapter=) across the redirect.
            let search = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .unwrap_or_default();
            let path = format!("/chat/{}{}", recent_id, search);
            let navigate_now = navigate.clone();

            redirected_to_recent.set(true);
            // Defer navigate to avoid RefCell re-entrancy: component creation runs
            // inside the wasm-bindgen-futures task queue, and navigate() internally
            // uses spawn_local, causing a double-borrow panic.
            gloo_timers::callback::Timeout::new(0, move || {
                navigate_now(
                    &path,
                    leptos_router::NavigateOptions {
                        replace: true,
                        ..Default::default()
                    },
                );
            })
            .forget();
        });
    }

    // Defer mounting to avoid wasm-bindgen-futures re-entrancy.
    let selected_signal = Signal::derive(|| None);
    let mounted = RwSignal::new(false);
    gloo_timers::callback::Timeout::new(0, move || {
        mounted.set(true);
    })
    .forget();

    view! {
        <>
            <h1 class="sr-only">"Chat"</h1>
            <Show when=move || mounted.try_get().unwrap_or(false) fallback=move || view! {
                <div
                    class="chat-loading-placeholder flex items-center justify-center h-full opacity-50"
                    data-testid="chat-loading-state"
                >
                    <Spinner />
                </div>
            }>
                {move || {
                    match system_status.try_get().unwrap_or(LoadingState::Idle) {
                        LoadingState::Loaded(status) => {
                            if matches!(status.inference_ready, InferenceReadyState::True) {
                                if has_recent_session {
                                    // During redirect to /chat/:session_id, keep view lightweight.
                                    view! { <div class="chat-redirect" /> }.into_any()
                                } else {
                                    view! { <ChatWorkspace selected_session_id=selected_signal /> }.into_any()
                                }
                            } else {
                                let guidance =
                                    guidance_for(status.inference_ready, primary_blocker(&status.inference_blockers));
                                view! {
                                    <ChatUnavailableEntry
                                        reason=guidance.reason.to_string()
                                        action_label=guidance.action.label.to_string()
                                        action_href=guidance.action.href.to_string()
                                        on_retry=retry_status
                                    />
                                }.into_any()
                            }
                        }
                        LoadingState::Error(_) => {
                            view! {
                                <ChatUnavailableEntry
                                    reason="System status unavailable".to_string()
                                    action_label="View system status".to_string()
                                    action_href="/system".to_string()
                                    on_retry=retry_status
                                />
                            }.into_any()
                        }
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <div
                                class="chat-loading-placeholder flex items-center justify-center h-full opacity-50"
                                data-testid="chat-loading-state"
                            >
                                <Spinner />
                            </div>
                        }.into_any(),
                    }
                }}
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
        let id = params
            .try_get()
            .unwrap_or_default()
            .get("session_id")
            .unwrap_or_default();
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
            <Show when=move || mounted.try_get().unwrap_or(false) fallback=move || view! {
                <div
                    class="chat-loading-placeholder flex items-center justify-center h-full opacity-50"
                    data-testid="chat-loading-state"
                >
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
    let show_archived = RwSignal::new(false);
    let navigate = use_navigate();
    let (_, chat_action) = use_chat();
    let sessions = RwSignal::new(ChatSessionsManager::load_sessions());
    let archived_sessions = RwSignal::new(ChatSessionsManager::load_archived_sessions());
    let refresh_sessions = {
        Callback::new(move |_: ()| {
            sessions.set(ChatSessionsManager::load_sessions());
            archived_sessions.set(ChatSessionsManager::load_archived_sessions());
        })
    };

    // Hydrate sessions from the backend — recovers sessions lost from localStorage.
    {
        let chat_action = chat_action.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match chat_action.list_backend_sessions().await {
                Ok(backend_sessions) => {
                    if ChatSessionsManager::merge_backend_sessions(&backend_sessions) {
                        sessions.set(ChatSessionsManager::load_sessions());
                        archived_sessions.set(ChatSessionsManager::load_archived_sessions());
                    }
                }
                Err(e) => {
                    // Non-fatal: localStorage sessions still work; log for debugging.
                    web_sys::console::warn_1(
                        &format!("[Chat] Failed to hydrate sessions from backend: {}", e).into(),
                    );
                }
            }
        });
    }

    // Derive a non-optional session ID for the conversation panel
    let session_id_for_panel =
        Signal::derive(move || selected_session_id.try_get().flatten().unwrap_or_default());
    let has_selection = Signal::derive(move || {
        selected_session_id
            .try_get()
            .flatten()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    });

    // Refresh sessions list when selection changes (picks up auto-saved sessions)
    {
        Effect::new(move |_| {
            let selected = selected_session_id.try_get().flatten();
            if let Some(id) = selected.clone() {
                show_archived.set(ChatSessionsManager::is_session_archived(&id));
            }
            sessions.set(ChatSessionsManager::load_sessions());
            archived_sessions.set(ChatSessionsManager::load_archived_sessions());
        });
    }

    // Handle session deletion
    let on_delete_session = {
        let navigate = navigate.clone();
        Callback::new(move |deleted_id: String| {
            ChatSessionsManager::delete_session(&deleted_id);
            sessions.set(ChatSessionsManager::load_sessions());
            archived_sessions.set(ChatSessionsManager::load_archived_sessions());
            // If deleted session was selected, go to /chat to auto-select next
            if selected_session_id.get_untracked().as_deref() == Some(deleted_id.as_str()) {
                navigate("/chat", Default::default());
            }
        })
    };

    // Archive selected/visible session
    let on_archive_session = {
        let navigate = navigate.clone();
        let chat_action = chat_action.clone();
        Callback::new(move |session_id: String| {
            let navigate = navigate.clone();
            let chat_action = chat_action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut apply_local = true;
                if let Err(e) = chat_action
                    .archive_backend_session(&session_id, Some("user_archive".to_string()))
                    .await
                {
                    if e.is_not_found() {
                        web_sys::console::warn_1(
                            &format!(
                                "[Chat] Backend session not found during archive; keeping local archive state for {}",
                                session_id
                            )
                            .into(),
                        );
                    } else {
                        apply_local = false;
                        report_error_with_toast(
                            &e,
                            "Failed to archive chat session",
                            Some("/chat"),
                            false,
                        );
                    }
                }

                if apply_local {
                    ChatSessionsManager::archive_session(&session_id);
                    sessions.set(ChatSessionsManager::load_sessions());
                    archived_sessions.set(ChatSessionsManager::load_archived_sessions());
                    if selected_session_id.get_untracked().as_deref() == Some(session_id.as_str()) {
                        navigate("/chat", Default::default());
                    }
                }
            });
        })
    };

    // Restore session from archive
    let on_unarchive_session = {
        let navigate = navigate.clone();
        let chat_action = chat_action.clone();
        Callback::new(move |session_id: String| {
            let navigate = navigate.clone();
            let chat_action = chat_action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut apply_local = true;
                if let Err(e) = chat_action.restore_backend_session(&session_id).await {
                    if e.is_not_found() {
                        web_sys::console::warn_1(
                            &format!(
                                "[Chat] Backend session not found during restore; keeping local unarchive state for {}",
                                session_id
                            )
                            .into(),
                        );
                    } else {
                        apply_local = false;
                        report_error_with_toast(
                            &e,
                            "Failed to restore chat session",
                            Some("/chat"),
                            false,
                        );
                    }
                }

                if apply_local {
                    ChatSessionsManager::unarchive_session(&session_id);
                    sessions.set(ChatSessionsManager::load_sessions());
                    archived_sessions.set(ChatSessionsManager::load_archived_sessions());
                    if selected_session_id.get_untracked().as_deref() == Some(session_id.as_str()) {
                        navigate("/chat", Default::default());
                    }
                }
            });
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
                                archived_sessions=archived_sessions
                                show_archived=show_archived
                                on_archive=on_archive_session
                                on_unarchive=on_unarchive_session
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
                // Mobile: back-nav breadcrumb + sessions toggle
                {move || {
                    if is_compact.try_get().unwrap_or(false) {
                        Some(view! {
                            <div class="flex items-center gap-2 px-4 py-2 border-b border-border bg-background/80 shrink-0">
                                <a
                                    href="/chat"
                                    class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                                    aria-label="Back to chat sessions"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/>
                                    </svg>
                                    "Chat"
                                </a>
                                <span class="text-xs text-muted-foreground/50">"/"</span>
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
                        fallback=move || view! { <ChatEmptyWorkspace/> }
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
                                archived_sessions=archived_sessions
                                show_archived=show_archived
                                on_archive=on_archive_session
                                on_unarchive=on_unarchive_session
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
                        report_error_with_toast(
                            &e,
                            "Failed to create chat session",
                            Some("/chat"),
                            false,
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
        <div class="flex h-full items-center justify-center p-6" data-testid="chat-empty-state">
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
                    <Button on_click=create_session data_testid="chat-empty-new-chat".to_string()>
                        "New Chat"
                    </Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=go_to_training
                        data_testid="chat-empty-create-adapter".to_string()
                    >
                        "Create Adapter"
                    </Button>
                </div>
                <p class="text-xs text-muted-foreground">
                    "or "
                    <a
                        href="/adapters"
                        class="underline hover:text-foreground transition-colors"
                        data-testid="chat-empty-browse-adapters"
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

#[component]
fn ChatUnavailableEntry(
    reason: String,
    action_label: String,
    action_href: String,
    on_retry: Callback<()>,
) -> impl IntoView {
    let status_center = use_status_center();

    view! {
        <div class="flex h-full items-center justify-center p-6" data-testid="chat-unavailable-state">
            <div class="w-full max-w-2xl rounded-lg border border-warning/40 bg-warning/10 p-6">
                <div class="space-y-2">
                    <h2 class="heading-3">"Chat unavailable"</h2>
                    <p class="text-sm text-muted-foreground" data-testid="chat-unavailable-reason">
                        {reason}"."
                    </p>
                </div>
                <div class="mt-5 flex flex-wrap items-center gap-2">
                    <ButtonLink
                        href=action_href
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        data_testid="chat-unavailable-action"
                    >
                        {action_label}
                    </ButtonLink>
                    <Button
                        variant=ButtonVariant::Secondary
                        size=ButtonSize::Sm
                        on_click=on_retry
                        data_testid="chat-unavailable-retry".to_string()
                    >
                        "Retry status"
                    </Button>
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
    /// Active sessions (reactive)
    #[prop(into)]
    sessions: Signal<Vec<ChatSessionMeta>>,
    /// Archived sessions (reactive)
    #[prop(into)]
    archived_sessions: Signal<Vec<ChatSessionMeta>>,
    /// Sidebar mode toggle (active vs archived)
    show_archived: RwSignal<bool>,
    /// Callback when a session is archived (passes session ID)
    on_archive: Callback<String>,
    /// Callback when a session is restored from archive (passes session ID)
    on_unarchive: Callback<String>,
    /// Callback when a session is deleted (passes deleted session ID)
    on_delete: Callback<String>,
) -> impl IntoView {
    let navigate = use_navigate();
    let search_query = RwSignal::new(String::new());
    let (chat_state, chat_action) = use_chat();

    // Shared tab state
    let show_shared = RwSignal::new(false);
    let shared_sessions = RwSignal::new(Vec::<crate::api::types::SharedSessionInfo>::new());
    let shared_loading = RwSignal::new(false);

    // Fetch shared sessions when the shared tab becomes active
    {
        Effect::new(move |_| {
            if !show_shared.try_get().unwrap_or(false) {
                return;
            }
            shared_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                match client.get_shared_with_me().await {
                    Ok(resp) => {
                        shared_sessions.set(resp.sessions);
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to load shared sessions",
                            Some("/chat"),
                            false,
                        );
                    }
                }
                shared_loading.set(false);
            });
        });
    }

    // Filtered sessions based on search
    let filtered_sessions = Memo::new(move |_| {
        let query = search_query.try_get().unwrap_or_default().to_lowercase();
        let all = if show_archived.try_get().unwrap_or(false) {
            archived_sessions.try_get().unwrap_or_default()
        } else {
            sessions.try_get().unwrap_or_default()
        };
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
                        report_error_with_toast(
                            &e,
                            "Failed to create chat session",
                            Some("/chat"),
                            false,
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
                        report_error_with_toast(
                            &e,
                            "Failed to save dock messages to session",
                            Some("/chat"),
                            false,
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

    // Multi-select state for chat-to-training flow
    let selected_training_session_ids = RwSignal::new(Vec::<String>::new());
    let creating_training_dataset = RwSignal::new(false);
    let training_dataset_error = RwSignal::new(Option::<String>::None);
    let selected_training_count = Signal::derive(move || {
        selected_training_session_ids
            .try_get()
            .unwrap_or_default()
            .len()
    });

    let toggle_training_session = Callback::new(move |(session_id, checked): (String, bool)| {
        selected_training_session_ids.update(|ids| {
            if checked {
                if !ids.contains(&session_id) {
                    ids.push(session_id);
                }
            } else {
                ids.retain(|id| id != &session_id);
            }
        });
        training_dataset_error.set(None);
    });

    let clear_training_selection = Callback::new(move |_: ()| {
        selected_training_session_ids.set(Vec::new());
        training_dataset_error.set(None);
    });

    let learn_and_generate_adapter = {
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            if creating_training_dataset.try_get().unwrap_or(false) {
                return;
            }
            let selected_ids = selected_training_session_ids.try_get().unwrap_or_default();
            if selected_ids.is_empty() {
                training_dataset_error
                    .set(Some("Select one or more chat sessions first.".to_string()));
                return;
            }

            creating_training_dataset.set(true);
            training_dataset_error.set(None);
            let navigate = navigate.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let mut combined_messages: Vec<ChatMessageInput> = selected_ids
                    .iter()
                    .filter_map(|id| ChatSessionsManager::load_session(id))
                    .flat_map(|session| {
                        session
                            .messages
                            .into_iter()
                            .filter_map(|msg| {
                                let content = msg.content.trim().to_string();
                                if content.is_empty() {
                                    None
                                } else {
                                    Some(ChatMessageInput {
                                        role: msg.role,
                                        content,
                                        timestamp: Some(msg.timestamp),
                                    })
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();

                if combined_messages.is_empty() {
                    training_dataset_error.set(Some(
                        "No messages found in the selected sessions.".to_string(),
                    ));
                    creating_training_dataset.set(false);
                    return;
                }

                if combined_messages.len() > MAX_CHAT_DATASET_MESSAGES {
                    training_dataset_error.set(Some(format!(
                        "Selected chats exceed {} messages. Choose fewer chats and try again.",
                        MAX_CHAT_DATASET_MESSAGES
                    )));
                    creating_training_dataset.set(false);
                    return;
                }

                combined_messages.sort_by(|a, b| {
                    a.timestamp
                        .as_deref()
                        .unwrap_or_default()
                        .cmp(b.timestamp.as_deref().unwrap_or_default())
                });

                let dataset_name = format!(
                    "chat-learning-{}",
                    crate::utils::now_utc().format("%Y%m%d_%H%M%S")
                );
                let provenance_session_id = if selected_ids.len() == 1 {
                    selected_ids.first().cloned()
                } else {
                    None
                };
                let client = ApiClient::with_base_url(api_base_url());

                match client
                    .create_dataset_from_chat(
                        combined_messages,
                        Some(dataset_name),
                        provenance_session_id,
                    )
                    .await
                {
                    Ok(resp) => {
                        selected_training_session_ids.set(Vec::new());
                        creating_training_dataset.set(false);
                        let path = format!(
                            "/training?open_wizard=1&dataset_id={}&return_to=/chat",
                            resp.dataset_id
                        );
                        navigate(&path, Default::default());
                    }
                    Err(e) => {
                        creating_training_dataset.set(false);
                        training_dataset_error.set(Some(e.user_message()));
                        report_error_with_toast(
                            &e,
                            "Failed to prepare training data from selected chats",
                            Some("/chat"),
                            false,
                        );
                    }
                }
            });
        })
    };

    // Delete confirmation state
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let show_delete_confirm = RwSignal::new(false);
    let active_count = Memo::new(move |_| sessions.try_get().unwrap_or_default().len());
    let archived_count = Memo::new(move |_| archived_sessions.try_get().unwrap_or_default().len());
    let pending_delete_title = Memo::new(move |_| {
        let id = pending_delete_id.try_get().flatten().unwrap_or_default();
        if id.is_empty() {
            return String::new();
        }
        sessions
            .try_get()
            .unwrap_or_default()
            .into_iter()
            .chain(archived_sessions.try_get().unwrap_or_default())
            .find(|s| s.id == id)
            .map(|s| s.title)
            .unwrap_or_default()
    });

    let request_delete = Callback::new(move |id: String| {
        pending_delete_id.set(Some(id));
        show_delete_confirm.set(true);
    });

    let confirm_delete = move |_| {
        if let Some(id) = pending_delete_id.try_get().flatten() {
            selected_training_session_ids.update(|ids| ids.retain(|sid| sid != &id));
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
                        aria-label="New Session"
                        data-testid="chat-sidebar-new-session"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
                        </svg>
                        "New Session"
                    </button>
                </div>
                <div class="grid grid-cols-3 gap-1 rounded-lg bg-muted/40 p-1">
                    <button
                        class=move || format!(
                            "px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if !show_archived.try_get().unwrap_or(false) && !show_shared.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(false); show_shared.set(false); }
                        aria-label="Show active sessions"
                        data-testid="chat-sidebar-toggle-active"
                    >
                        {move || format!("Active ({})", active_count.try_get().unwrap_or(0))}
                    </button>
                    <button
                        class=move || format!(
                            "px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if show_archived.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(true); show_shared.set(false); }
                        aria-label="Show archived sessions"
                        data-testid="chat-sidebar-toggle-archived"
                    >
                        {move || format!("Archived ({})", archived_count.try_get().unwrap_or(0))}
                    </button>
                    <button
                        class=move || format!(
                            "px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if show_shared.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_shared.set(true); show_archived.set(false); }
                        aria-label="Show shared sessions"
                        data-testid="chat-sidebar-toggle-shared"
                    >
                        "Shared"
                    </button>
                </div>
                <div class="space-y-1.5">
                    <button
                        class=move || format!(
                            "w-full inline-flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-semibold rounded-md border transition-colors {}",
                            if creating_training_dataset.try_get().unwrap_or(false)
                                || selected_training_count.try_get().unwrap_or(0) == 0
                            {
                                "border-border text-muted-foreground bg-muted/30 cursor-not-allowed"
                            } else {
                                "border-primary/30 text-primary bg-primary/5 hover:bg-primary/10"
                            }
                        )
                        disabled=move || {
                            creating_training_dataset.try_get().unwrap_or(false)
                                || selected_training_count.try_get().unwrap_or(0) == 0
                        }
                        on:click=move |_| learn_and_generate_adapter.run(())
                        title="Create training data from selected chats and open training"
                        aria-label="Learn and Generate Adapter"
                        data-testid="chat-sidebar-learn"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v4m0 10v4M3 12h4m10 0h4M5.6 5.6l2.8 2.8m7.2 7.2 2.8 2.8m0-12.8-2.8 2.8m-7.2 7.2-2.8 2.8"/>
                        </svg>
                        {move || {
                            if creating_training_dataset.try_get().unwrap_or(false) {
                                "Preparing training data..."
                            } else {
                                "Learn & Generate Adapter"
                            }
                        }}
                    </button>
                    <div class="flex items-center justify-between gap-2 text-2xs text-muted-foreground">
                        <span>
                            {move || format!(
                                "{} selected",
                                selected_training_count.try_get().unwrap_or(0)
                            )}
                        </span>
                        <button
                            class="underline decoration-dotted hover:text-foreground disabled:no-underline disabled:cursor-not-allowed"
                            disabled=move || selected_training_count.try_get().unwrap_or(0) == 0
                            on:click=move |_| clear_training_selection.run(())
                            data-testid="chat-sidebar-learn-clear"
                        >
                            "Clear"
                        </button>
                    </div>
                    {move || {
                        training_dataset_error
                            .try_get()
                            .flatten()
                            .map(|err| {
                                view! {
                                    <p class="text-2xs text-destructive">{err}</p>
                                }
                            })
                    }}
                </div>
                <div data-testid="chat-sidebar-search">
                    <Input
                        value=search_query
                        placeholder="Search sessions...".to_string()
                    />
                </div>
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
                                    data-testid="chat-sidebar-continue"
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
                    // Shared sessions tab
                    if show_shared.try_get().unwrap_or(false) {
                        let is_loading = shared_loading.try_get().unwrap_or(false);
                        let items = shared_sessions.try_get().unwrap_or_default();
                        if is_loading {
                            return view! {
                                <div class="p-6 flex items-center justify-center">
                                    <Spinner />
                                </div>
                            }.into_any();
                        }
                        if items.is_empty() {
                            let create_session_shared = create_session;
                            return view! {
                                <div class="p-6 text-center space-y-2">
                                    <p class="text-xs text-muted-foreground">"No sessions shared with you yet"</p>
                                    <div class="flex items-center justify-center gap-2">
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| {
                                                show_shared.set(false);
                                                show_archived.set(false);
                                            })
                                        >
                                            "View active sessions"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Secondary
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| create_session_shared.run(()))
                                        >
                                            "New session"
                                        </Button>
                                    </div>
                                </div>
                            }.into_any();
                        }
                        return view! {
                            <div class="divide-y divide-border">
                                {items.into_iter().map(|shared| {
                                    let href = format!("/chat/{}", shared.session_id);
                                    view! {
                                        <a href=href class="block px-3 py-2 hover:bg-muted/50 transition-colors">
                                            <h3 class="text-sm font-medium truncate">{shared.name}</h3>
                                            <div class="flex items-center gap-2 mt-0.5 text-2xs text-muted-foreground">
                                                <span>{format!("from {}", shared.shared_by)}</span>
                                                <span>"·"</span>
                                                <Badge variant=BadgeVariant::Outline>{shared.permission.clone()}</Badge>
                                            </div>
                                        </a>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any();
                    }

                    let showing_archived = show_archived.try_get().unwrap_or(false);
                    let list = filtered_sessions.try_get().unwrap_or_default();
                    if list.is_empty() {
                        let create_session_empty = create_session;
                        let has_search = !search_query.try_get().unwrap_or_default().is_empty();
                        let message = if has_search {
                            "No matching sessions"
                        } else if showing_archived {
                            "No archived sessions"
                        } else {
                            "No sessions yet"
                        };
                        view! {
                            <div class="p-6 text-center space-y-2">
                                <p class="text-xs text-muted-foreground">{message}</p>
                                {if has_search {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| search_query.set(String::new()))
                                        >
                                            "Clear search"
                                        </Button>
                                    }.into_any()
                                } else if showing_archived {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| {
                                                show_archived.set(false);
                                                show_shared.set(false);
                                            })
                                        >
                                            "View active sessions"
                                        </Button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Secondary
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| create_session_empty.run(()))
                                        >
                                            "New session"
                                        </Button>
                                    }.into_any()
                                }}
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
                                    let training_selected = {
                                        let id = id.clone();
                                        Signal::derive(move || {
                                            selected_training_session_ids
                                                .try_get()
                                                .unwrap_or_default()
                                                .contains(&id)
                                        })
                                    };
                                    let delete_handler = request_delete;
                                    let archive_handler = on_archive;
                                    let unarchive_handler = on_unarchive;
                                    let training_toggle_handler = toggle_training_session;
                                    let archive_id = id.clone();
                                    let unarchive_id = id.clone();
                                    let delete_id = id.clone();
                                    let training_id = id.clone();
                                    let archive_callback = if showing_archived {
                                        None
                                    } else {
                                        Some(Callback::new(move |_: ()| {
                                            archive_handler.run(archive_id.clone());
                                        }))
                                    };
                                    let unarchive_callback = if showing_archived {
                                        Some(Callback::new(move |_: ()| {
                                            unarchive_handler.run(unarchive_id.clone());
                                        }))
                                    } else {
                                        None
                                    };
                                    view! {
                                        <SessionListItem
                                            session=session
                                            selected=is_selected
                                            training_selected=training_selected
                                            on_training_select_change=Callback::new(move |checked| {
                                                training_toggle_handler.run((training_id.clone(), checked));
                                            })
                                            on_archive=archive_callback
                                            on_unarchive=unarchive_callback
                                            on_delete=Callback::new(move |_: ()| {
                                                delete_handler.run(delete_id.clone());
                                            })
                                            is_archived=showing_archived
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Delete confirmation dialog
            {move || {
                let title = pending_delete_title.try_get().unwrap_or_default();
                let description = format!(
                    "Are you sure you want to delete '{}'? This action cannot be undone.",
                    title,
                );
                view! {
                    <ConfirmationDialog
                        open=show_delete_confirm
                        title="Delete Session"
                        description=description
                        severity=ConfirmationSeverity::Destructive
                        confirm_text="Delete"
                        typed_confirmation=title
                        on_confirm=Callback::new(confirm_delete)
                        on_cancel=Callback::new(cancel_delete)
                    />
                }
            }}
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
    /// Whether this session is selected for "Learn & Generate Adapter"
    #[prop(into)]
    training_selected: Signal<bool>,
    /// Callback for selecting this session as training input
    on_training_select_change: Callback<bool>,
    on_archive: Option<Callback<()>>,
    on_unarchive: Option<Callback<()>>,
    on_delete: Callback<()>,
    /// Whether this item is in the archived list (controls overflow menu options)
    #[prop(default = false)]
    is_archived: bool,
) -> impl IntoView {
    let settings = use_settings();
    let id = session.id.clone();
    let href = format!("/chat/{}", id);
    let updated_at = session.updated_at.clone();
    let message_count = session.message_count;
    let session_title = session.title.clone();
    let session_preview = session.preview.clone();
    let training_aria_label = format!("Select '{}' for adapter learning", session_title.clone());
    let archive_action = on_archive;
    let unarchive_action = on_unarchive;

    // Overflow menu state
    let show_overflow = RwSignal::new(false);
    let show_share_dialog = RwSignal::new(false);
    let show_tags_dialog = RwSignal::new(false);
    let show_fork_dialog = RwSignal::new(false);
    let show_provenance_panel = RwSignal::new(false);
    let show_permanent_delete_confirm = RwSignal::new(false);
    let share_session_id = id.clone();
    let tags_session_id = id.clone();
    let fork_session_id = id.clone();
    let provenance_session_id = id.clone();
    let hard_delete_session_id = id.clone();

    let navigate = use_navigate();

    view! {
        <div
            class=move || format!(
                "group relative flex items-start gap-2 py-2 px-3 transition-colors {}",
                if selected.try_get().unwrap_or(false) {
                    "bg-primary/10 border-l-2 border-l-primary"
                } else {
                    "hover:bg-muted/50 border-l-2 border-l-transparent"
                }
            )
            data-testid="chat-session-row"
        >
            <div
                class="pt-0.5 shrink-0"
                data-testid="chat-session-training-checkbox"
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            >
                <Checkbox
                    checked=training_selected
                    on_change=Callback::new(move |checked| on_training_select_change.run(checked))
                    aria_label=training_aria_label
                />
            </div>

            <a href=href class="flex-1 min-w-0">
                <div class="min-w-0">
                    // Title
                    <h3 class="text-sm font-medium truncate">{session_title}</h3>

                    // Preview
                    {if !session_preview.is_empty() {
                        let preview = session_preview.clone();
                        Some(view! {
                            <p class="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                                {preview}
                            </p>
                        })
                    } else {
                        None
                    }}

                    // Metadata
                    <div class="flex items-center gap-2 mt-0.5 text-2xs text-muted-foreground">
                        {move || {
                            let show_timestamps = settings
                                .try_get()
                                .map(|s| s.show_timestamps)
                                .unwrap_or(true);
                            if show_timestamps {
                                view! {
                                    <>
                                        <span>{format_relative_time(&updated_at)}</span>
                                        <span>"·"</span>
                                    </>
                                }
                                .into_any()
                            } else {
                                view! {}.into_any()
                            }
                        }}
                        <span>{format!("{} msgs", message_count)}</span>
                    </div>
                </div>
            </a>

            <div class="flex items-center gap-1 shrink-0">
                {archive_action.map(|archive| view! {
                    <button
                        class="p-1 rounded hover:bg-primary/10 text-muted-foreground hover:text-primary transition-colors"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            archive.run(());
                        }
                        title="Archive session"
                        aria-label="Archive session"
                        data-testid="chat-session-archive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 7h18M5 7v10a2 2 0 002 2h10a2 2 0 002-2V7M9 11h6"/>
                        </svg>
                    </button>
                })}
                {unarchive_action.map(|unarchive| view! {
                    <button
                        class="p-1 rounded hover:bg-primary/10 text-muted-foreground hover:text-primary transition-colors"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            unarchive.run(());
                        }
                        title="Restore session"
                        aria-label="Restore session"
                        data-testid="chat-session-unarchive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 12a8 8 0 118 8M4 12V8m0 4h4"/>
                        </svg>
                    </button>
                })}
                <button
                    class="p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        on_delete.run(());
                    }
                    title="Delete session"
                    aria-label="Delete session"
                    data-testid="chat-session-delete"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                    </svg>
                </button>

                // Overflow menu (kebab)
                <div class="relative">
                    <button
                        class="p-1 rounded hover:bg-muted/50 text-muted-foreground hover:text-foreground transition-colors"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            show_overflow.update(|v| *v = !*v);
                        }
                        title="More actions"
                        aria-label="More actions"
                        aria-haspopup="menu"
                        attr:aria-expanded=move || show_overflow.try_get().unwrap_or(false).to_string()
                        data-testid="chat-session-overflow"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="currentColor" viewBox="0 0 20 20">
                            <circle cx="10" cy="4" r="1.5"/>
                            <circle cx="10" cy="10" r="1.5"/>
                            <circle cx="10" cy="16" r="1.5"/>
                        </svg>
                    </button>
                    {move || {
                        if !show_overflow.try_get().unwrap_or(false) {
                            return view! {}.into_any();
                        }
                        view! {
                            <div
                                class="fixed inset-0 z-40"
                                on:click=move |_| show_overflow.set(false)
                            />
                            <div
                                class="absolute right-0 top-full z-50 mt-1 w-40 rounded-md border border-border bg-background shadow-lg py-1"
                                role="menu"
                            >
                                <button
                                    class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
                                    role="menuitem"
                                    on:click=move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        show_overflow.set(false);
                                        show_share_dialog.set(true);
                                    }
                                >
                                    "Share"
                                </button>
                                <button
                                    class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
                                    role="menuitem"
                                    on:click=move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        show_overflow.set(false);
                                        show_tags_dialog.set(true);
                                    }
                                >
                                    "Tags"
                                </button>
                                <button
                                    class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
                                    role="menuitem"
                                    on:click=move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        show_overflow.set(false);
                                        show_fork_dialog.set(true);
                                    }
                                >
                                    "Fork"
                                </button>
                                <button
                                    class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
                                    role="menuitem"
                                    on:click=move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        show_overflow.set(false);
                                        show_provenance_panel.set(true);
                                    }
                                >
                                    "Provenance"
                                </button>
                                {is_archived.then(|| view! {
                                    <div class="border-t border-border my-1" role="separator" />
                                    <button
                                        class="w-full text-left px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors"
                                        role="menuitem"
                                        on:click=move |ev: web_sys::MouseEvent| {
                                            ev.stop_propagation();
                                            show_overflow.set(false);
                                            show_permanent_delete_confirm.set(true);
                                        }
                                    >
                                        "Permanent Delete"
                                    </button>
                                })}
                            </div>
                        }.into_any()
                    }}
                </div>
            </div>

            // Collaboration dialogs (rendered inside the item for correct session context)
            {move || {
                if show_share_dialog.try_get().unwrap_or(false) {
                    Some(view! {
                        <ShareSessionDialog
                            session_id=share_session_id.clone()
                            on_close=Callback::new(move |_| show_share_dialog.set(false))
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_tags_dialog.try_get().unwrap_or(false) {
                    Some(view! {
                        <TagsDialog
                            session_id=tags_session_id.clone()
                            on_close=Callback::new(move |_| show_tags_dialog.set(false))
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_fork_dialog.try_get().unwrap_or(false) {
                    let navigate = navigate.clone();
                    Some(view! {
                        <ForkSessionDialog
                            session_id=fork_session_id.clone()
                            on_close=Callback::new(move |_| show_fork_dialog.set(false))
                            on_forked=Callback::new(move |new_id: String| {
                                show_fork_dialog.set(false);
                                let path = format!("/chat/{}", new_id);
                                navigate(&path, Default::default());
                            })
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_provenance_panel.try_get().unwrap_or(false) {
                    Some(view! {
                        <ProvenancePanel
                            session_id=provenance_session_id.clone()
                            on_close=Callback::new(move |_| show_provenance_panel.set(false))
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_permanent_delete_confirm.try_get().unwrap_or(false) {
                    let delete_id = hard_delete_session_id.clone();
                    Some(view! {
                        <ConfirmationDialog
                            open=show_permanent_delete_confirm
                            title="Permanently Delete Session"
                            description="This will permanently remove the session and all its data. This cannot be undone."
                            severity=ConfirmationSeverity::Destructive
                            confirm_text="Permanently Delete"
                            on_confirm=Callback::new(move |_| {
                                let id = delete_id.clone();
                                let on_delete = on_delete;
                                show_permanent_delete_confirm.set(false);
                                wasm_bindgen_futures::spawn_local(async move {
                                    let client = ApiClient::with_base_url(api_base_url());
                                    match client.hard_delete_session(&id).await {
                                        Ok(_) => on_delete.run(()),
                                        Err(e) => {
                                            report_error_with_toast(&e, "Failed to permanently delete session", Some("/chat"), false);
                                        }
                                    }
                                });
                            })
                            on_cancel=Callback::new(move |_| show_permanent_delete_confirm.set(false))
                        />
                    })
                } else { None }
            }}
        </div>
    }
}

use crate::utils::{format_relative_time, humanize};

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
                let is_stub = msg_count == 0 && !stored.placeholder;
                action.restore_session(stored);
                session_error.set(None);
                web_sys::console::log_1(
                    &format!("[Chat] Restored session {} with {} messages", id, msg_count).into(),
                );
                // If this is a server-recovered stub with no local messages,
                // fetch messages from the backend and restore them.
                if is_stub {
                    let action = action.clone();
                    let id = id.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match action.fetch_session_messages(&id).await {
                            Ok(messages) if !messages.is_empty() => {
                                if let Some(updated) =
                                    ChatSessionsManager::backfill_session_messages(&id, &messages)
                                {
                                    action.restore_session(updated);
                                    refresh_sessions.run(());
                                }
                            }
                            Ok(_) => {} // No messages on server either
                            Err(e) => {
                                web_sys::console::warn_1(
                                    &format!(
                                        "[Chat] Failed to backfill messages for {}: {}",
                                        id, e
                                    )
                                    .into(),
                                );
                            }
                        }
                    });
                }
            } else {
                // Session not found locally; create a local draft but do not persist it yet.
                // This keeps URL navigation stable without creating phantom sessions.
                let placeholder = ChatSessionsManager::create_placeholder_session(&id);
                action.restore_session(placeholder);
                session_error.set(Some(
                    "Local draft session (not confirmed by server yet).".to_string(),
                ));
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
                        session_error.set(None);
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
                let _ = upload_cancelled.try_set(true);
                let _ = attach_mode.try_set(AttachMode::Upload);
                let _ = selected_file_name.try_set(None);
                selected_file.set_value(None);
                let _ = attach_status.try_set(None);
                let _ = attach_error.try_set(None);
                let _ = attach_busy.try_set(false);
                let _ = pasted_text.try_set(String::new());
                let _ = selected_msg_indices.try_set(std::collections::HashSet::new());
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
    // Extract the active model name from system status for resolving "Auto" targets.
    let active_model_name =
        Signal::derive(
            move || match system_status.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(ref status) => status
                    .kernel
                    .as_ref()
                    .and_then(|k| k.model.as_ref())
                    .and_then(|m| m.model_id.clone()),
                _ => None,
            },
        );
    let base_model_label =
        Signal::derive(
            move || match chat_state.try_get().unwrap_or_default().target.clone() {
                ChatTarget::Model(name) => name,
                _ => active_model_name
                    .try_get()
                    .flatten()
                    .unwrap_or_else(|| "Auto".to_string()),
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
                    if let Err(validation_error) = validate_attach_upload_file(&file) {
                        attach_error.set(Some(validation_error));
                        selected_file_name.set(None);
                        selected_file.set_value(None);
                        return;
                    }

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

    // Collaboration UI state
    let show_header_share = RwSignal::new(false);
    let show_header_tags = RwSignal::new(false);
    let show_header_fork = RwSignal::new(false);
    let show_header_provenance = RwSignal::new(false);

    // Tags loaded for badge display in header
    let session_tags = RwSignal::new(Vec::<crate::api::types::ChatTagResponse>::new());
    {
        Effect::new(move |_| {
            let id = session_id();
            if id.is_empty() {
                return;
            }
            let id = id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                match client.get_session_tags(&id).await {
                    Ok(resp) => session_tags.set(resp.tags),
                    Err(_) => session_tags.set(Vec::new()),
                }
            });
        });
    }

    view! {
        <div class="p-6 flex h-full min-h-0 flex-col gap-4">
            // Header
            <div
                class="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-4"
                data-testid="chat-header"
            >
                <div class="space-y-1">
                    <h2 class="heading-3">"Chat Session"</h2>
                    <div class="flex items-center gap-2 text-xs text-muted-foreground">
                        <span class="uppercase tracking-wider text-2xs font-medium">"Session"</span>
                        <span
                            class="font-mono bg-muted/30 px-1.5 py-0.5 rounded text-2xs"
                            data-testid="chat-session-id-label"
                        >
                            {session_label}
                        </span>
                    </div>
                </div>
                <div class="flex items-center gap-3">
                    // Target selector for choosing model, stack, or policy pack
                    <crate::components::chat_dock::ChatTargetSelector inline=true/>
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
                    <div data-testid="chat-status-badge">
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
                    // Collaboration actions
                    <div class="flex items-center gap-1">
                        <button
                            class="px-2 py-1 text-xs rounded-md border border-border hover:bg-muted/50 transition-colors"
                            on:click=move |_| show_header_share.set(true)
                            title="Share session"
                            data-testid="chat-header-share"
                        >
                            "Share"
                        </button>
                        <button
                            class="px-2 py-1 text-xs rounded-md border border-border hover:bg-muted/50 transition-colors"
                            on:click=move |_| show_header_fork.set(true)
                            title="Fork session"
                            data-testid="chat-header-fork"
                        >
                            "Fork"
                        </button>
                        <button
                            class="px-2 py-1 text-xs rounded-md border border-border hover:bg-muted/50 transition-colors"
                            on:click=move |_| show_header_provenance.set(true)
                            title="View provenance"
                            data-testid="chat-header-provenance"
                        >
                            "Provenance"
                        </button>
                    </div>
                </div>
            </div>

            // Tags badge row
            <div class="flex items-center gap-2 flex-wrap" data-testid="chat-header-tags">
                {move || {
                    let tags = session_tags.try_get().unwrap_or_default();
                    if tags.is_empty() {
                        None
                    } else {
                        Some(view! {
                            <>
                                {tags.into_iter().map(|tag| {
                                    let style = tag.color.as_deref()
                                        .map(|c| format!("border-color: {}; color: {}", c, c))
                                        .unwrap_or_default();
                                    view! {
                                        <span
                                            class="inline-flex items-center px-2 py-0.5 text-2xs font-medium rounded-full border border-border"
                                            style=style
                                        >
                                            {tag.name}
                                        </span>
                                    }
                                }).collect::<Vec<_>>()}
                            </>
                        })
                    }
                }}
                <button
                    class="inline-flex items-center px-1.5 py-0.5 text-2xs text-muted-foreground hover:text-foreground rounded border border-dashed border-border hover:border-foreground/30 transition-colors"
                    on:click=move |_| show_header_tags.set(true)
                    data-testid="chat-header-edit-tags"
                >
                    "+ Tag"
                </button>
            </div>

            // Collaboration dialogs (header-level)
            {move || {
                if show_header_share.try_get().unwrap_or(false) {
                    Some(view! {
                        <ShareSessionDialog
                            session_id=session_id()
                            on_close=Callback::new(move |_| show_header_share.set(false))
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_header_tags.try_get().unwrap_or(false) {
                    let refresh_tags = Callback::new(move |_: ()| {
                        let id = session_id();
                        wasm_bindgen_futures::spawn_local(async move {
                            let client = ApiClient::with_base_url(api_base_url());
                            if let Ok(resp) = client.get_session_tags(&id).await {
                                session_tags.set(resp.tags);
                            }
                        });
                    });
                    Some(view! {
                        <TagsDialog
                            session_id=session_id()
                            on_close=Callback::new(move |_| {
                                show_header_tags.set(false);
                                refresh_tags.run(());
                            })
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_header_fork.try_get().unwrap_or(false) {
                    let navigate = use_navigate();
                    Some(view! {
                        <ForkSessionDialog
                            session_id=session_id()
                            on_close=Callback::new(move |_| show_header_fork.set(false))
                            on_forked=Callback::new(move |new_id: String| {
                                show_header_fork.set(false);
                                let path = format!("/chat/{}", new_id);
                                navigate(&path, Default::default());
                            })
                        />
                    })
                } else { None }
            }}
            {move || {
                if show_header_provenance.try_get().unwrap_or(false) {
                    Some(view! {
                        <ProvenancePanel
                            session_id=session_id()
                            on_close=Callback::new(move |_| show_header_provenance.set(false))
                        />
                    })
                } else { None }
            }}

            // Stream status notice (progressive latency + TTFT feedback)
            // Error-tone notices with state.error are shown in the error banner below.
            // Info + Warning (without error) are shown here as inline status.
            {move || {
                let state = chat_state.try_get().unwrap_or_default();
                let notice = state.stream_notice.clone()?;
                // Error-tone notices paired with an actual error use the error banner
                if notice.tone == StreamNoticeTone::Error && state.error.is_some() {
                    return None;
                }
                // Paused notices have their own dedicated section
                if notice.tone == StreamNoticeTone::Paused {
                    return None;
                }
                let message = notice.message.clone();
                let is_ttft = message.ends_with("to first token");
                let is_warning = notice.tone == StreamNoticeTone::Warning;
                let variant = if is_warning {
                    BadgeVariant::Warning
                } else {
                    BadgeVariant::Secondary
                };
                let css_class = if is_ttft {
                    "latency-status latency-status--ttft"
                } else if is_warning {
                    "latency-status latency-status--slow"
                } else {
                    "latency-status"
                };
                Some(view! {
                    <div class=css_class data-testid="chat-stream-status">
                        <Badge variant=variant>{message}</Badge>
                    </div>
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
                // Context overflow indicator
                {
                    let dismiss_action = chat_action.clone();
                    move || {
                        let notice = chat_state.try_get().unwrap_or_default().overflow_notice();
                        notice.map(|msg| {
                            let dismiss = dismiss_action.clone();
                            let evicted = chat_state.try_get().unwrap_or_default().total_messages_evicted > 0;
                            let severity_class = if evicted {
                                "chat-overflow-notice chat-overflow-notice--evicted"
                            } else {
                                "chat-overflow-notice chat-overflow-notice--warning"
                            };
                            view! {
                                <div
                                    class=severity_class
                                    role="status"
                                    aria-live="polite"
                                    data-testid="chat-overflow-notice"
                                >
                                    <span class="chat-overflow-notice-text">{msg}</span>
                                    <button
                                        class="chat-overflow-notice-dismiss"
                                        type="button"
                                        title="Dismiss"
                                        aria-label="Dismiss overflow notice"
                                        on:click=move |_| dismiss.dismiss_overflow_notice()
                                    >
                                        {"\u{00d7}"}
                                    </button>
                                </div>
                            }
                        })
                    }
                }
                <div class="p-5">
                    {move || {
                        let msgs = chat_state.try_get().unwrap_or_default().messages;
                        if msgs.is_empty() {
                            view! {
                                <div
                                    class="flex h-full min-h-[200px] items-center justify-center py-12"
                                    data-testid="chat-conversation-empty"
                                >
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
                                        // Suggestion chips (clickable to pre-fill)
                                        <div class="flex flex-wrap justify-center gap-2 pt-2">
                                            {["Summarize a document", "Explain a concept", "Review code"].into_iter().map(|prompt| {
                                                let prompt_text = prompt.to_string();
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="text-xs px-3 py-1.5 rounded-full bg-muted text-muted-foreground hover:bg-primary/10 hover:text-primary transition-colors cursor-pointer"
                                                        data-testid="chat-conversation-suggestion"
                                                        on:click=move |_| {
                                                            message.set(prompt_text.clone());
                                                        }
                                                    >
                                                        {prompt}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
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
                                                            {if is_user {
                                                                view! {
                                                                    <p class="text-sm whitespace-pre-wrap break-words leading-relaxed">
                                                                        {msg.content.clone()}
                                                                    </p>
                                                                }.into_any()
                                                            } else if is_streaming {
                                                                let content = msg.content.clone();
                                                                view! {
                                                                    <div class="text-sm break-words leading-relaxed">
                                                                        <Markdown content=content.clone() />
                                                                        {if !content.is_empty() {
                                                                            view! {
                                                                                <span class="inline-block animate-pulse text-primary/70 ml-0.5">"▍"</span>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! {
                                                                                <span class="inline-flex items-center gap-1.5 text-muted-foreground">
                                                                                    <Spinner/>
                                                                                    <span class="text-xs">"Routing..."</span>
                                                                                </span>
                                                                            }.into_any()
                                                                        }}
                                                                    </div>
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <div class="text-sm break-words leading-relaxed">
                                                                        <Markdown content=msg.content.clone() />
                                                                    </div>
                                                                }.into_any()
                                                            }}
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
                                            let fallback_error = state
                                                .error
                                                .as_deref()
                                                .map(str::trim)
                                                .filter(|msg| !msg.is_empty() && !msg.eq_ignore_ascii_case("error"))
                                                .map(|msg| msg.to_string());

                                            let display_msg = notice.as_ref()
                                                .map(|n| n.message.clone())
                                                .or(fallback_error)
                                                .unwrap_or_else(|| "Request failed. Retry in a moment.".to_string());

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
                    <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                        <div class="flex items-center justify-between gap-2">
                            <p class="text-sm text-warning-foreground">{e}</p>
                            <a
                                href="/chat"
                                class="text-sm font-medium text-primary hover:underline"
                                data-testid="chat-session-error-link"
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
                                                    data_testid="chat-error-retry".to_string()
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
                                            data-testid="chat-error-dismiss"
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
                                crate::components::inference_guidance::primary_blocker(&status.inference_blockers),
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
                                            <ButtonLink
                                                href=action.href
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                            >
                                                {action.label}
                                            </ButtonLink>
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
                    <Button
                        button_type=ButtonType::Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        on_click=Callback::new(move |_| show_attach_dialog.set(true))
                        data_testid="chat-attach".to_string()
                    >
                        "Attach data"
                    </Button>
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
                                    data_testid="chat-stop".to_string()
                                >
                                    "Stop"
                                </Button>
                            }.into_any()
                        } else {
                            let disabled = !can_send.try_get().unwrap_or(false);
                            view! {
                                <Button
                                    button_type=ButtonType::Submit
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
                                    accept=".pdf,.txt,.md,.markdown"
                                    on:change=move |ev| {
                                        match selected_file_from_event(&ev) {
                                            Some(file) => match validate_attach_upload_file(&file) {
                                                Ok(()) => {
                                                    selected_file_name.set(Some(file.name()));
                                                    selected_file.set_value(Some(file));
                                                    attach_error.set(None);
                                                }
                                                Err(validation_error) => {
                                                    selected_file_name.set(None);
                                                    selected_file.set_value(None);
                                                    attach_error.set(Some(validation_error));
                                                }
                                            },
                                            None => {
                                                selected_file_name.set(None);
                                                selected_file.set_value(None);
                                            }
                                        }
                                        reset_file_input_value(&ev);
                                    }
                                />
                                <p class="text-xs text-muted-foreground">
                                    {format!(
                                        "Supported: PDF, TXT, Markdown · Max {} MB",
                                        DOCUMENT_UPLOAD_MAX_FILE_SIZE / 1024 / 1024
                                    )}
                                </p>
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
                            let chat_state_for_quick_select = chat_state;
                            let selected_msg_indices_for_quick_select = selected_msg_indices;
                            let select_last_n = Callback::new(move |n: usize| {
                                let msgs = chat_state_for_quick_select
                                    .try_get()
                                    .unwrap_or_default()
                                    .messages;
                                let total = msgs.len();
                                let start = total.saturating_sub(n);
                                let indices: std::collections::HashSet<usize> =
                                    (start..total).collect();
                                selected_msg_indices_for_quick_select.set(indices);
                            });
                            let select_last_5 = select_last_n;
                            let select_last_10 = select_last_n;
                            let select_last_20 = select_last_n;

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
                                            on:click=move |_| select_last_5.run(5)
                                        >
                                            "Last 5"
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_10.run(10)
                                        >
                                            "Last 10"
                                        </button>
                                        <button
                                            type="button"
                                            class="px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_20.run(20)
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

fn validate_attach_upload_file(file: &web_sys::File) -> Result<(), String> {
    let size = file.size() as u64;
    if size > DOCUMENT_UPLOAD_MAX_FILE_SIZE {
        return Err(format!(
            "File too large. Maximum size is {} MB.",
            DOCUMENT_UPLOAD_MAX_FILE_SIZE / 1024 / 1024
        ));
    }

    let file_name = file.name().to_lowercase();
    let supported = DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS
        .iter()
        .any(|ext| file_name.ends_with(ext));
    if !supported {
        return Err(format!(
            "Unsupported file type. Supported: {}",
            DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS.join(", ")
        ));
    }

    Ok(())
}

fn reset_file_input_value(ev: &web_sys::Event) {
    if let Some(input) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
    {
        input.set_value("");
    }
}

// ---------------------------------------------------------------------------
// Collaboration Dialogs (PASS 2)
// ---------------------------------------------------------------------------

/// Dialog for sharing a chat session with other users
#[component]
fn ShareSessionDialog(session_id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let user_ids_input = RwSignal::new(String::new());
    let permission = RwSignal::new("read".to_string());
    let sharing = RwSignal::new(false);
    let share_error = RwSignal::new(Option::<String>::None);
    let share_success = RwSignal::new(false);

    // Load existing shares
    let existing_shares = RwSignal::new(Vec::<crate::api::types::SessionShareInfo>::new());
    {
        let sid = session_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::with_base_url(api_base_url());
            match client.get_session_shares(&sid).await {
                Ok(resp) => existing_shares.set(resp.shares),
                Err(e) => {
                    report_error_with_toast(
                        &e,
                        "Failed to load session shares",
                        Some("/chat"),
                        false,
                    );
                }
            }
        });
    }

    let do_share = {
        let sid = session_id.clone();
        Callback::new(move |_: ()| {
            let raw = user_ids_input.try_get().unwrap_or_default();
            let ids: Vec<String> = raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if ids.is_empty() {
                share_error.set(Some("Enter at least one user ID.".to_string()));
                return;
            }
            sharing.set(true);
            share_error.set(None);
            let perm = permission.try_get().unwrap_or_else(|| "read".to_string());
            let sid = sid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                let req = ShareSessionRequest {
                    user_ids: Some(ids),
                    workspace_id: None,
                    permission: perm,
                    expires_at: None,
                };
                match client.share_session(&sid, &req).await {
                    Ok(resp) => {
                        existing_shares.set(resp.shares);
                        share_success.set(true);
                        user_ids_input.set(String::new());
                    }
                    Err(e) => {
                        share_error.set(Some(e.user_message()));
                        report_error_with_toast(
                            &e,
                            "Failed to share session",
                            Some("/chat"),
                            false,
                        );
                    }
                }
                sharing.set(false);
            });
        })
    };

    let do_revoke = {
        let sid = session_id.clone();
        Callback::new(move |share_id: String| {
            let sid = sid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                match client.revoke_session_share(&sid, &share_id).await {
                    Ok(()) => {
                        existing_shares.update(|shares| shares.retain(|s| s.share_id != share_id));
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to revoke share", Some("/chat"), false);
                    }
                }
            });
        })
    };

    let open = RwSignal::new(true);
    let close_cb = on_close;
    Effect::new(move |_| {
        if !open.try_get().unwrap_or(true) {
            close_cb.run(());
        }
    });

    view! {
        <Dialog open=open title="Share Session">
            <div class="space-y-4">
                <div class="space-y-2">
                    <label class="text-xs font-medium">"User IDs (comma-separated)"</label>
                    <Input
                        value=user_ids_input
                        placeholder="user-1, user-2".to_string()
                    />
                </div>
                <div class="space-y-2">
                    <label class="text-xs font-medium">"Permission"</label>
                    <div class="flex items-center gap-2">
                        <button
                            class=move || format!(
                                "px-3 py-1 text-xs rounded-md border transition-colors {}",
                                if permission.try_get().unwrap_or_default() == "read" {
                                    "border-primary bg-primary/10 text-primary"
                                } else {
                                    "border-border text-muted-foreground hover:text-foreground"
                                }
                            )
                            on:click=move |_| permission.set("read".to_string())
                        >
                            "Read"
                        </button>
                        <button
                            class=move || format!(
                                "px-3 py-1 text-xs rounded-md border transition-colors {}",
                                if permission.try_get().unwrap_or_default() == "write" {
                                    "border-primary bg-primary/10 text-primary"
                                } else {
                                    "border-border text-muted-foreground hover:text-foreground"
                                }
                            )
                            on:click=move |_| permission.set("write".to_string())
                        >
                            "Write"
                        </button>
                    </div>
                </div>
                {move || share_error.try_get().flatten().map(|err| view! {
                    <p class="text-xs text-destructive">{err}</p>
                })}
                {move || share_success.try_get().unwrap_or(false).then(|| view! {
                    <p class="text-xs text-success">"Session shared."</p>
                })}
                <div class="flex items-center justify-end gap-2">
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(move |_| open.set(false))>
                        "Close"
                    </Button>
                    <Button
                        size=ButtonSize::Sm
                        on_click=do_share
                        disabled=Signal::derive(move || sharing.try_get().unwrap_or(false))
                    >
                        {move || if sharing.try_get().unwrap_or(false) { "Sharing..." } else { "Share" }}
                    </Button>
                </div>

                // Existing shares
                {move || {
                    let shares = existing_shares.try_get().unwrap_or_default();
                    if shares.is_empty() {
                        None
                    } else {
                        Some(view! {
                            <div class="border-t border-border pt-3 space-y-2">
                                <h4 class="text-xs font-medium">"Current Shares"</h4>
                                {shares.into_iter().map(|share| {
                                    let revoke = do_revoke;
                                    let sid = share.share_id.clone();
                                    view! {
                                        <div class="flex items-center justify-between text-xs">
                                            <div>
                                                <span class="font-mono">{share.user_id}</span>
                                                <span class="text-muted-foreground ml-2">{format!("({})", share.permission)}</span>
                                            </div>
                                            <button
                                                class="text-destructive hover:underline text-2xs"
                                                on:click=move |_| revoke.run(sid.clone())
                                            >
                                                "Revoke"
                                            </button>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        })
                    }
                }}
            </div>
        </Dialog>
    }
}

/// Dialog for managing tags on a chat session
#[component]
fn TagsDialog(session_id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let session_tags = RwSignal::new(Vec::<crate::api::types::ChatTagResponse>::new());
    let all_tags = RwSignal::new(Vec::<crate::api::types::ChatTagResponse>::new());
    let loading = RwSignal::new(true);
    let new_tag_name = RwSignal::new(String::new());
    let new_tag_color = RwSignal::new(String::new());

    // Load session tags and all available tags
    {
        let sid = session_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::with_base_url(api_base_url());
            let (session_result, all_result) = (
                client.get_session_tags(&sid).await,
                client.list_chat_tags().await,
            );
            match session_result {
                Ok(resp) => session_tags.set(resp.tags),
                Err(e) => {
                    report_error_with_toast(
                        &e,
                        "Failed to load session tags",
                        Some("/chat"),
                        false,
                    );
                }
            }
            match all_result {
                Ok(tags) => all_tags.set(tags),
                Err(e) => {
                    report_error_with_toast(&e, "Failed to load tags", Some("/chat"), false);
                }
            }
            loading.set(false);
        });
    }

    let assign_tag = {
        let sid = session_id.clone();
        Callback::new(move |tag_id: String| {
            let sid = sid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                let mut current_ids: Vec<String> = session_tags
                    .try_get()
                    .unwrap_or_default()
                    .iter()
                    .map(|t| t.id.clone())
                    .collect();
                if !current_ids.contains(&tag_id) {
                    current_ids.push(tag_id);
                }
                let req = AssignTagsRequest {
                    tag_ids: current_ids,
                };
                match client.assign_tags(&sid, &req).await {
                    Ok(()) => {
                        if let Ok(resp) = client.get_session_tags(&sid).await {
                            session_tags.set(resp.tags);
                        }
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to assign tag", Some("/chat"), false);
                    }
                }
            });
        })
    };

    let remove_tag = {
        let sid = session_id.clone();
        Callback::new(move |tag_id: String| {
            let sid = sid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                match client.remove_tag(&sid, &tag_id).await {
                    Ok(()) => {
                        session_tags.update(|tags| tags.retain(|t| t.id != tag_id));
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to remove tag", Some("/chat"), false);
                    }
                }
            });
        })
    };

    let create_tag = Callback::new(move |_: ()| {
        let name = new_tag_name.try_get().unwrap_or_default();
        if name.trim().is_empty() {
            return;
        }
        let color_raw = new_tag_color.try_get().unwrap_or_default();
        let color = if color_raw.trim().is_empty() {
            None
        } else {
            Some(color_raw)
        };
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::with_base_url(api_base_url());
            let req = CreateChatTagRequest { name, color };
            match client.create_chat_tag(&req).await {
                Ok(tag) => {
                    all_tags.update(|tags| tags.push(tag));
                    new_tag_name.set(String::new());
                    new_tag_color.set(String::new());
                }
                Err(e) => {
                    report_error_with_toast(&e, "Failed to create tag", Some("/chat"), false);
                }
            }
        });
    });

    let open = RwSignal::new(true);
    let close_cb = on_close;
    Effect::new(move |_| {
        if !open.try_get().unwrap_or(true) {
            close_cb.run(());
        }
    });

    view! {
        <Dialog open=open title="Manage Tags">
            <div class="space-y-4">
                {move || {
                    if loading.try_get().unwrap_or(true) {
                        return view! { <Spinner /> }.into_any();
                    }

                    let current = session_tags.try_get().unwrap_or_default();
                    let available = all_tags.try_get().unwrap_or_default();
                    let current_ids: Vec<String> = current.iter().map(|t| t.id.clone()).collect();

                    view! {
                        <div class="space-y-3">
                            // Current tags
                            <div>
                                <h4 class="text-xs font-medium mb-1">"Assigned Tags"</h4>
                                {if current.is_empty() {
                                    view! { <p class="text-2xs text-muted-foreground">"No tags assigned"</p> }.into_any()
                                } else {
                                    view! {
                                        <div class="flex flex-wrap gap-1">
                                            {current.into_iter().map(|tag| {
                                                let remove = remove_tag;
                                                let tid = tag.id.clone();
                                                let style = tag.color.as_deref()
                                                    .map(|c| format!("border-color: {}; color: {}", c, c))
                                                    .unwrap_or_default();
                                                view! {
                                                    <span
                                                        class="inline-flex items-center gap-1 px-2 py-0.5 text-2xs rounded-full border border-border"
                                                        style=style
                                                    >
                                                        {tag.name}
                                                        <button
                                                            class="hover:text-destructive"
                                                            on:click=move |_| remove.run(tid.clone())
                                                            title="Remove tag"
                                                        >
                                                            "x"
                                                        </button>
                                                    </span>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>

                            // Available tags to assign
                            <div>
                                <h4 class="text-xs font-medium mb-1">"Available Tags"</h4>
                                {
                                    let unassigned: Vec<_> = available.into_iter()
                                        .filter(|t| !current_ids.contains(&t.id))
                                        .collect();
                                    if unassigned.is_empty() {
                                        view! { <p class="text-2xs text-muted-foreground">"All tags assigned"</p> }.into_any()
                                    } else {
                                        view! {
                                            <div class="flex flex-wrap gap-1">
                                                {unassigned.into_iter().map(|tag| {
                                                    let assign = assign_tag;
                                                    let tid = tag.id.clone();
                                                    let style = tag.color.as_deref()
                                                        .map(|c| format!("border-color: {}; color: {}", c, c))
                                                        .unwrap_or_default();
                                                    view! {
                                                        <button
                                                            class="inline-flex items-center px-2 py-0.5 text-2xs rounded-full border border-dashed border-border hover:border-primary transition-colors"
                                                            style=style
                                                            on:click=move |_| assign.run(tid.clone())
                                                            title="Assign tag"
                                                        >
                                                            {"+ "}{tag.name}
                                                        </button>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                            </div>

                            // Create new tag
                            <div class="border-t border-border pt-3">
                                <h4 class="text-xs font-medium mb-2">"Create New Tag"</h4>
                                <div class="flex items-end gap-2">
                                    <div class="flex-1">
                                        <Input
                                            value=new_tag_name
                                            placeholder="Tag name".to_string()
                                        />
                                    </div>
                                    <div class="w-24">
                                        <Input
                                            value=new_tag_color
                                            placeholder="#color".to_string()
                                        />
                                    </div>
                                    <Button size=ButtonSize::Sm on_click=create_tag>
                                        "Create"
                                    </Button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}

                <div class="flex justify-end">
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(move |_| open.set(false))>
                        "Close"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}

/// Dialog for forking a chat session
#[component]
fn ForkSessionDialog(
    session_id: String,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_forked: Callback<String>,
) -> impl IntoView {
    let fork_name = RwSignal::new(String::new());
    let include_messages = RwSignal::new(true);
    let forking = RwSignal::new(false);
    let fork_error = RwSignal::new(Option::<String>::None);

    let do_fork = {
        let sid = session_id.clone();
        Callback::new(move |_: ()| {
            forking.set(true);
            fork_error.set(None);
            let name_val = fork_name.try_get().unwrap_or_default();
            let name = if name_val.trim().is_empty() {
                None
            } else {
                Some(name_val)
            };
            let msgs = include_messages.try_get().unwrap_or(true);
            let sid = sid.clone();
            let on_forked = on_forked;
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(api_base_url());
                let req = ForkSessionRequest {
                    name,
                    include_messages: msgs,
                };
                match client.fork_session(&sid, &req).await {
                    Ok(resp) => {
                        on_forked.run(resp.session_id);
                    }
                    Err(e) => {
                        fork_error.set(Some(e.user_message()));
                        report_error_with_toast(&e, "Failed to fork session", Some("/chat"), false);
                    }
                }
                forking.set(false);
            });
        })
    };

    let open = RwSignal::new(true);
    let close_cb = on_close;
    Effect::new(move |_| {
        if !open.try_get().unwrap_or(true) {
            close_cb.run(());
        }
    });

    view! {
        <Dialog open=open title="Fork Session">
            <div class="space-y-4">
                <div class="space-y-2">
                    <label class="text-xs font-medium">"New session name (optional)"</label>
                    <Input
                        value=fork_name
                        placeholder="Forked session name".to_string()
                    />
                </div>
                <div class="flex items-center gap-2">
                    <Checkbox
                        checked=Signal::derive(move || include_messages.try_get().unwrap_or(true))
                        on_change=Callback::new(move |v| include_messages.set(v))
                        aria_label="Include messages in fork"
                    />
                    <span class="text-xs">"Include existing messages"</span>
                </div>
                {move || fork_error.try_get().flatten().map(|err| view! {
                    <p class="text-xs text-destructive">{err}</p>
                })}
                <div class="flex items-center justify-end gap-2">
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(move |_| open.set(false))>
                        "Cancel"
                    </Button>
                    <Button
                        size=ButtonSize::Sm
                        on_click=do_fork
                        disabled=Signal::derive(move || forking.try_get().unwrap_or(false))
                    >
                        {move || if forking.try_get().unwrap_or(false) { "Forking..." } else { "Fork" }}
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}

/// Panel showing provenance chain for a chat session
#[component]
fn ProvenancePanel(session_id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let provenance =
        RwSignal::new(Option::<adapteros_api_types::provenance::ChatProvenanceResponse>::None);
    let loading = RwSignal::new(true);
    let load_error = RwSignal::new(Option::<String>::None);

    {
        let sid = session_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::with_base_url(api_base_url());
            match client.get_chat_provenance(&sid).await {
                Ok(resp) => provenance.set(Some(resp)),
                Err(e) => {
                    report_error_with_toast(&e, "Failed to load provenance", Some("/chat"), false);
                    load_error.set(Some(e.user_message()));
                }
            }
            loading.set(false);
        });
    }

    let open = RwSignal::new(true);
    let close_cb = on_close;
    Effect::new(move |_| {
        if !open.try_get().unwrap_or(true) {
            close_cb.run(());
        }
    });

    view! {
        <Dialog open=open title="Session Provenance">
            <div class="space-y-4 max-h-96 overflow-y-auto">
                {move || {
                    if loading.try_get().unwrap_or(true) {
                        return view! { <Spinner /> }.into_any();
                    }
                    if let Some(err) = load_error.try_get().flatten() {
                        return view! {
                            <p class="text-xs text-destructive">{err}</p>
                        }.into_any();
                    }
                    let Some(prov) = provenance.try_get().flatten() else {
                        return view! {
                            <p class="text-xs text-muted-foreground">"No provenance data available."</p>
                        }.into_any();
                    };

                    view! {
                        <div class="space-y-4">
                            // Session info
                            <div class="space-y-1">
                                <h4 class="text-xs font-semibold">"Session"</h4>
                                <div class="text-2xs text-muted-foreground space-y-0.5">
                                    <p>{format!("Name: {}", prov.session.name)}</p>
                                    <p>{format!("ID: {}", prov.session.id)}</p>
                                    <p>{format!("Messages: {}", prov.session.message_count)}</p>
                                    <p>{format!("Created: {}", prov.session.created_at)}</p>
                                </div>
                            </div>

                            // Stack info
                            {prov.stack.map(|stack| view! {
                                <div class="space-y-1">
                                    <h4 class="text-xs font-semibold">"Stack"</h4>
                                    <div class="text-2xs text-muted-foreground space-y-0.5">
                                        <p>{format!("Name: {}", stack.name)}</p>
                                        <p>{format!("ID: {}", stack.id)}</p>
                                        {stack.description.map(|d| view! { <p>{format!("Description: {}", d)}</p> })}
                                        <p>{format!("Adapters: {}", stack.adapter_ids.join(", "))}</p>
                                    </div>
                                </div>
                            })}

                            // Base model
                            {prov.base_model.map(|model| view! {
                                <div class="space-y-1">
                                    <h4 class="text-xs font-semibold">"Base Model"</h4>
                                    <div class="text-2xs text-muted-foreground space-y-0.5">
                                        <p>{format!("{} ({})", model.name, model.id)}</p>
                                        <p class="font-mono">{format!("Hash: {}", model.hash_b3)}</p>
                                    </div>
                                </div>
                            })}

                            // Adapters
                            {if prov.adapters.is_empty() {
                                None
                            } else {
                                Some(view! {
                                    <div class="space-y-2">
                                        <h4 class="text-xs font-semibold">"Adapters"</h4>
                                        {prov.adapters.into_iter().map(|adapter| {
                                            view! {
                                                <div class="rounded border border-border p-2 space-y-1">
                                                    <p class="text-xs font-medium">{adapter.name.clone()}</p>
                                                    <div class="text-2xs text-muted-foreground space-y-0.5">
                                                        <p>{format!("ID: {}", adapter.id)}</p>
                                                        <p>{format!("Tier: {}", adapter.tier)}</p>
                                                        <p class="font-mono">{format!("Hash: {}", adapter.hash_b3)}</p>
                                                        {adapter.training_job.map(|job| view! {
                                                            <div class="mt-1 pl-2 border-l-2 border-primary/30 space-y-0.5">
                                                                <p class="font-medium">"Training Job"</p>
                                                                <p>{format!("Status: {}", humanize(&job.status))}</p>
                                                                <p>{format!("Started: {}", job.started_at)}</p>
                                                                {job.completed_at.map(|c| view! { <p>{format!("Completed: {}", c)}</p> })}
                                                                {job.dataset.map(|ds| view! {
                                                                    <div class="mt-0.5">
                                                                        <p class="font-medium">"Dataset"</p>
                                                                        <p>{format!("{} ({})", ds.name, ds.format.to_uppercase())}</p>
                                                                        <p>{format!("Files: {}, Size: {} bytes", ds.file_count, ds.total_size_bytes)}</p>
                                                                    </div>
                                                                })}
                                                            </div>
                                                        })}
                                                    </div>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                })
                            }}

                            // Timeline
                            {prov.timeline.map(|events| {
                                if events.is_empty() {
                                    return view! {}.into_any();
                                }
                                view! {
                                    <div class="space-y-2">
                                        <h4 class="text-xs font-semibold">"Timeline"</h4>
                                        <div class="space-y-1">
                                            {events.into_iter().map(|event| view! {
                                                <div class="flex items-start gap-2 text-2xs">
                                                    <span class="text-muted-foreground shrink-0 w-32 font-mono">{event.timestamp.clone()}</span>
                                                    <span>{event.description}</span>
                                                </div>
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </div>
                                }.into_any()
                            })}

                            // Provenance hash
                            <div class="border-t border-border pt-2">
                                <p class="text-2xs text-muted-foreground font-mono">
                                    {format!("Provenance hash: {}", prov.provenance_hash)}
                                </p>
                                <p class="text-2xs text-muted-foreground">
                                    {format!("Computed: {}", prov.computed_at)}
                                </p>
                            </div>
                        </div>
                    }.into_any()
                }}

                <div class="flex justify-end pt-2">
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(move |_| open.set(false))>
                        "Close"
                    </Button>
                </div>
            </div>
        </Dialog>
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
