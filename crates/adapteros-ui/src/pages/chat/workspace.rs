//! Chat page with SSE streaming support
//!
//! This module provides the chat interface. The full chat page uses
//! the global chat state from signals/chat.rs for unified state management
//! with the dock panel.
//!
//! Page status: PRIMARY — default landing surface.
//!
//! ## Test-contracted IDs (do not rename without updating Playwright)
//! `chat-input`, `chat-send`, `chat-loading-state`, `chat-empty-state`,
//! `chat-unavailable-state`, `chat-unavailable-reason`, `chat-unavailable-action`,
//! `chat-run-link`, `chat-receipt-link`, `chat-replay-link`,
//! `chat-adapter-chips`, `chat-citation-chips`, `chat-trace-links`,
//! `chat-header`, `chat-conversation-empty`, `chat-stream-status`,
//! `chat-session-state-pending`, `chat-session-state-not-found`,
//! `chat-session-state-transient`, `chat-session-confirm-retry`.
//!
//! ## Performance Characteristics
//!
//! Streaming updates flow through:
//! 1. SSE token -> `stream_inference_to_state` (signals/chat.rs)
//! 2. Token appended via `push_str` (O(1) amortized)
//! 3. Signal update triggers reactive subscribers
//! 4. Message list updates use keyed iteration for efficient per-message diffs.
//!
//! Both the dock (`chat_dock.rs`) and full chat page use `<For>` with keyed
//! iteration so unchanged messages stay stable during streaming.
//!
//! Enable `show_telemetry_overlay` in settings for perf timing.

use super::conversation::ChatConversationPanel;
#[cfg(target_arch = "wasm32")]
use super::session_list::generate_readable_id;
use super::session_list::{ChatEmptyWorkspace, SessionListPanel};
use super::ChatSurfaceMode;
use crate::api::{report_error_with_toast, ApiError};
#[cfg(target_arch = "wasm32")]
use crate::components::inference_guidance::guidance_for;
#[cfg(target_arch = "wasm32")]
use crate::components::inference_guidance::primary_blocker;
use crate::components::layout::nav_group_label_for_route;
use crate::components::{
    use_is_tablet_or_smaller, Badge, BadgeVariant, ChatSessionListShell, ChatWorkspaceLayout,
    IconX, PageBreadcrumbItem, PageScaffold, PageScaffoldStatus, Spinner,
};
#[cfg(target_arch = "wasm32")]
use crate::components::{ChatQuickStartCard, ChatUnavailableEntry};
#[cfg(target_arch = "wasm32")]
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{use_chat, use_ui_profile, ChatSessionsManager};
#[cfg(target_arch = "wasm32")]
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use leptos_router::hooks::use_location;
use leptos_router::hooks::use_navigate;
#[cfg(target_arch = "wasm32")]
use leptos_router::hooks::use_params_map;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Maximum prompt length for URL-embedded prompts (bytes).
/// This prevents DoS attacks from extremely long URLs that could:
/// 1. Exceed browser URL limits (typically 2KB-8KB)
/// 2. Exhaust memory when decoded
/// 3. Overwhelm the inference endpoint
pub(super) const MAX_URL_PROMPT_LENGTH: usize = 2000;
pub(super) const DOCUMENT_UPLOAD_MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
pub(super) const DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS: &[&str] =
    &[".pdf", ".txt", ".md", ".markdown"];
pub(super) const MAX_CHAT_DATASET_MESSAGES: usize = 10_000;
pub(super) const CHAT_SCROLL_BOTTOM_THRESHOLD_PX: i32 = 24;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum AttachMode {
    #[default]
    Upload,
    Paste,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SessionConfirmationState {
    #[default]
    Confirmed,
    PendingConfirm,
    NotFound,
    TransientError,
}

pub(super) fn map_session_confirmation_error(error: &ApiError) -> SessionConfirmationState {
    if error.is_not_found() {
        SessionConfirmationState::NotFound
    } else {
        SessionConfirmationState::TransientError
    }
}

/// Chat landing page (quickstart composer-first).
/// Route: /chat
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn Chat() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
    view! {
        <PageScaffold
            title="Chat"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/chat"),
                PageBreadcrumbItem::current("Quick Start"),
            ]
            full_width=true
        >
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>"Quick Start"</Badge>
            </PageScaffoldStatus>
            <div class="mx-auto max-w-2xl px-4 py-8 space-y-4">
                <h2 class="heading-3">"Prompt to first answer"</h2>
                <p class="text-sm text-muted-foreground">
                    "Open the web UI to start a conversation. This server-side render is a static fallback."
                </p>
            </div>
        </PageScaffold>
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn Chat() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
    let (_, chat_action) = use_chat();
    let navigate = use_navigate();
    let (system_status, refetch_status) = use_system_status();
    let refetch_status_signal = StoredValue::new(refetch_status);
    let prompt = RwSignal::new(String::new());
    let query_adapter = RwSignal::new(Option::<String>::None);
    let creating_session = RwSignal::new(false);

    {
        if let Some(window) = web_sys::window() {
            if let Ok(search) = window.location().search() {
                if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                    if let Some(adapter) = params.get("adapter") {
                        let decoded = js_sys::decode_uri_component(&adapter)
                            .map(|s| s.as_string().unwrap_or_default())
                            .unwrap_or(adapter);
                        if !decoded.trim().is_empty() {
                            query_adapter.set(Some(decoded));
                        }
                    }
                }
            }
        }
    }

    // Focus composer on mount for quickstart flow.
    {
        gloo_timers::callback::Timeout::new(0, move || {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Ok(Some(element)) = document.query_selector("[data-testid='chat-input']")
                    {
                        if let Some(input) = element.dyn_ref::<web_sys::HtmlElement>() {
                            let _ = input.focus();
                        }
                    }
                }
            }
        })
        .forget();
    }

    let retry_status = Callback::new(move |_: ()| {
        let _ = refetch_status_signal.try_with_value(|f| f.run(()));
    });

    let can_submit = Signal::derive(move || {
        if creating_session.try_get().unwrap_or(false) {
            return false;
        }
        let has_prompt = !prompt.try_get().unwrap_or_default().trim().is_empty();
        let has_adapter = query_adapter
            .try_get()
            .flatten()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        has_prompt || has_adapter
    });
    let submit_prompt = {
        let action = chat_action.clone();
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            if creating_session.try_get().unwrap_or(false) {
                return;
            }
            let initial_prompt = prompt.try_get().unwrap_or_default();
            let adapter = query_adapter.try_get().flatten();
            if initial_prompt.trim().is_empty() && adapter.as_deref().is_none_or(str::is_empty) {
                return;
            }
            creating_session.set(true);
            if !initial_prompt.trim().is_empty() {
                prompt.set(String::new());
            }
            let action = action.clone();
            let navigate = navigate.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = generate_readable_id("session", "quickstart");
                match action
                    .create_backend_session(name, Some("Quick Start Conversation".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        let mut query_parts = Vec::new();
                        if !initial_prompt.trim().is_empty() {
                            let encoded_prompt =
                                js_sys::encode_uri_component(initial_prompt.trim())
                                    .as_string()
                                    .unwrap_or(initial_prompt);
                            query_parts.push(format!("prompt={}", encoded_prompt));
                        }
                        if let Some(adapter_id) = adapter.filter(|s| !s.trim().is_empty()) {
                            let encoded_adapter = js_sys::encode_uri_component(adapter_id.trim())
                                .as_string()
                                .unwrap_or(adapter_id);
                            query_parts.push(format!("adapter={}", encoded_adapter));
                        }
                        let query = if query_parts.is_empty() {
                            String::new()
                        } else {
                            format!("?{}", query_parts.join("&"))
                        };
                        let path = format!("/chat/s/{}{}", session_id, query);
                        navigate(&path, Default::default());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to create chat session",
                            Some("/chat"),
                            false,
                        );
                    }
                }
                creating_session.set(false);
            });
        })
    };
    let submit_on_enter = {
        let submit_prompt = submit_prompt;
        Callback::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Enter" && !ev.shift_key() && can_submit.try_get().unwrap_or(false) {
                ev.prevent_default();
                submit_prompt.run(());
            }
        })
    };

    view! {
        <PageScaffold
            title="Chat"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/chat"),
                PageBreadcrumbItem::current("Quick Start"),
            ]
            full_width=true
        >
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>
                    {move || match system_status.try_get().unwrap_or(LoadingState::Idle) {
                        LoadingState::Loaded(status) if matches!(status.inference_ready, InferenceReadyState::True) => "Ready".to_string(),
                        LoadingState::Loaded(_) => "Blocked".to_string(),
                        LoadingState::Error(_) => "Unavailable".to_string(),
                        LoadingState::Idle | LoadingState::Loading => "Checking".to_string(),
                    }}
                </Badge>
            </PageScaffoldStatus>
            {move || {
                match system_status.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Loaded(status) => {
                        if matches!(status.inference_ready, InferenceReadyState::True) {
                            view! {
                                <ChatQuickStartCard
                                    prompt=prompt
                                    creating_session=creating_session.read_only()
                                    can_submit=can_submit
                                    on_submit=submit_prompt
                                    on_submit_on_enter=submit_on_enter
                                    pinned_adapter=Signal::derive(move || query_adapter.try_get().flatten())
                                />
                            }.into_any()
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
        </PageScaffold>
    }
    .into_any()
}

/// Chat workspace route component used by both `/chat/history` and `/chat/s/:session_id`.
///
/// Uses deferred mounting: renders a lightweight placeholder first, then mounts
/// the full ChatWorkspace on the next tick. This avoids a wasm-bindgen-futures
/// RefCell re-entrancy panic (#2562) that occurs when Leptos builds the complex
/// ChatWorkspace component tree inside the task queue during SPA navigation.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn ChatSession() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
    let is_history_surface = use_location().pathname.get_untracked() == "/chat/history";
    let title = if is_history_surface {
        "Chat History"
    } else {
        "Chat Session"
    };
    let status_badge = if is_history_surface {
        "History"
    } else {
        "Session"
    };
    let breadcrumbs = if is_history_surface {
        vec![
            PageBreadcrumbItem::new(nav_label, "/chat"),
            PageBreadcrumbItem::current("History"),
        ]
    } else {
        vec![
            PageBreadcrumbItem::new(nav_label, "/chat"),
            PageBreadcrumbItem::new("History", "/chat/history"),
            PageBreadcrumbItem::current("Session"),
        ]
    };

    view! {
        <PageScaffold title=title breadcrumbs=breadcrumbs full_width=true>
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>{status_badge}</Badge>
            </PageScaffoldStatus>
            <div class="chat-loading-placeholder flex items-center justify-center h-full opacity-50" data-testid="chat-session-ssr-fallback">
                <Spinner />
            </div>
        </PageScaffold>
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn ChatSession() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
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
    let is_history_surface = selected_id.get_untracked().is_none();
    let title = if is_history_surface {
        "Chat History"
    } else {
        "Chat Session"
    };
    let status_badge = if is_history_surface {
        "History"
    } else {
        "Session"
    };
    let breadcrumbs = if is_history_surface {
        vec![
            PageBreadcrumbItem::new(nav_label, "/chat"),
            PageBreadcrumbItem::current("History"),
        ]
    } else {
        vec![
            PageBreadcrumbItem::new(nav_label, "/chat"),
            PageBreadcrumbItem::new("History", "/chat/history"),
            PageBreadcrumbItem::current("Session"),
        ]
    };
    let surface_mode = if is_history_surface {
        ChatSurfaceMode::History
    } else {
        ChatSurfaceMode::SessionDetail
    };

    // Defer heavy component tree construction to next tick to break out of the
    // wasm-bindgen-futures task queue context and avoid RefCell re-entrancy.
    let mounted = RwSignal::new(false);
    gloo_timers::callback::Timeout::new(0, move || {
        mounted.set(true);
    })
    .forget();

    view! {
        <PageScaffold title=title breadcrumbs=breadcrumbs full_width=true>
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>{status_badge}</Badge>
            </PageScaffoldStatus>
            <Show when=move || mounted.try_get().unwrap_or(false) fallback=move || view! {
                <div
                    class="chat-loading-placeholder flex items-center justify-center h-full opacity-50"
                    data-testid="chat-loading-state"
                >
                    <Spinner />
                </div>
            }>
                <ChatWorkspace
                    selected_session_id=selected_id
                    handle_query_params=true
                    surface_mode=surface_mode
                />
            </Show>
        </PageScaffold>
    }
}

#[component]
pub fn ChatHistory() -> impl IntoView {
    view! { <ChatSession/> }
}

#[component]
pub fn ChatSessionEquivalent() -> impl IntoView {
    view! { <ChatSession/> }
}

// ---------------------------------------------------------------------------
// ChatWorkspace - two-column layout with session list + conversation
// ---------------------------------------------------------------------------

/// Chat workspace with session list sidebar (desktop) and conversation panel.
/// On mobile, session list is available via a slide-out overlay.
#[allow(dead_code)]
#[component]
fn ChatWorkspace(
    /// The currently selected session ID. None means no session selected.
    selected_session_id: Signal<Option<String>>,
    /// Whether to handle ?prompt= and ?adapter= query parameters
    #[prop(default = false)]
    handle_query_params: bool,
    /// Route-level surface mode (quickstart/history/session detail)
    #[prop(default = ChatSurfaceMode::SessionDetail)]
    surface_mode: ChatSurfaceMode,
) -> impl IntoView {
    let is_history_surface = matches!(surface_mode, ChatSurfaceMode::History);
    let is_compact = use_is_tablet_or_smaller();
    let show_mobile_sessions = RwSignal::new(false);
    let show_archived = RwSignal::new(false);
    let navigate = use_navigate();
    let (_, chat_action) = use_chat();
    let sessions = RwSignal::new(ChatSessionsManager::load_sessions());
    let archived_sessions = RwSignal::new(ChatSessionsManager::load_archived_sessions());
    let session_index_epoch = RwSignal::new(0_u64);
    let refresh_sessions = {
        Callback::new(move |_: ()| {
            sessions.set(ChatSessionsManager::load_sessions());
            archived_sessions.set(ChatSessionsManager::load_archived_sessions());
            session_index_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
        })
    };

    // Hydrate sessions from the backend — recovers sessions lost from localStorage.
    {
        let chat_action = chat_action.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match chat_action.list_backend_sessions().await {
                Ok(backend_sessions) => {
                    if ChatSessionsManager::merge_backend_sessions(&backend_sessions) {
                        refresh_sessions.run(());
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
    let session_index_epoch_signal =
        Signal::derive(move || session_index_epoch.try_get().unwrap_or(0));
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
            refresh_sessions.run(());
        });
    }

    // Handle session deletion
    let on_delete_session = {
        let navigate = navigate.clone();
        Callback::new(move |deleted_id: String| {
            ChatSessionsManager::delete_session(&deleted_id);
            refresh_sessions.run(());
            // If deleted session was selected, go back to explicit history.
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
                    refresh_sessions.run(());
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
                    refresh_sessions.run(());
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
        <ChatWorkspaceLayout>
            // Desktop: persistent session list sidebar
            {move || {
                if !is_compact.try_get().unwrap_or(false) && is_history_surface {
                    Some(view! {
                        <ChatSessionListShell>
                            <SessionListPanel
                                selected_id=selected_session_id
                                sessions=sessions
                                archived_sessions=archived_sessions
                                show_archived=show_archived
                                on_archive=on_archive_session
                                on_unarchive=on_unarchive_session
                                on_delete=on_delete_session
                            />
                        </ChatSessionListShell>
                    })
                } else {
                    None
                }
            }}

            // Conversation area
            <div class="flex-1 min-w-0 flex flex-col h-full">
                // Mobile: back-nav breadcrumb + sessions toggle
                {move || {
                    if is_compact.try_get().unwrap_or(false) && is_history_surface {
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
                                    class="btn btn-outline btn-sm inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-border hover:bg-muted/50 transition-colors"
                                    on:click=move |_| show_mobile_sessions.update(|v| *v = !*v)
                                    aria-label="Toggle session list"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16"/>
                                    </svg>
                                    "Sessions"
                                </button>
                            </div>
                        }.into_any())
                    } else if is_compact.try_get().unwrap_or(false) {
                        Some(view! {
                            <div class="flex items-center gap-2 px-4 py-2 border-b border-border bg-background/80 shrink-0">
                                <a
                                    href="/chat"
                                    class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                                    aria-label="Back to chat history"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/>
                                    </svg>
                                    "History"
                                </a>
                            </div>
                        }.into_any())
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
                            session_index_epoch=session_index_epoch_signal
                            handle_query_params=handle_query_params
                            refresh_sessions=refresh_sessions
                        />
                    </Show>
                </div>
            </div>

            // Mobile: session list overlay (slide-in from left)
            {move || {
                if is_compact.try_get().unwrap_or(false)
                    && is_history_surface
                    && show_mobile_sessions.try_get().unwrap_or(false)
                {
                    Some(view! {
                        <div
                            class="fixed inset-0 z-40 bg-background/80 backdrop-blur-sm"
                            on:click=move |_| show_mobile_sessions.set(false)
                        />
                        <div class="fixed inset-y-0 left-0 z-50 w-80 bg-background border-r border-border shadow-xl flex flex-col overflow-hidden">
                            <div class="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
                                <h2 class="text-sm font-semibold">"Sessions"</h2>
                                <button
                                    class="btn btn-ghost btn-icon-sm p-1.5 rounded hover:bg-muted/50 text-muted-foreground"
                                    on:click=move |_| show_mobile_sessions.set(false)
                                    aria-label="Close session list"
                                >
                                    <IconX class="h-4 w-4"/>
                                </button>
                            </div>
                            <ChatSessionListShell>
                                <SessionListPanel
                                    selected_id=selected_session_id
                                    sessions=sessions
                                    archived_sessions=archived_sessions
                                    show_archived=show_archived
                                    on_archive=on_archive_session
                                    on_unarchive=on_unarchive_session
                                    on_delete=on_delete_session
                                />
                            </ChatSessionListShell>
                        </div>
                    })
                } else {
                    None
                }
            }}
        </ChatWorkspaceLayout>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_confirmation_maps_not_found_errors_to_not_found_state() {
        let not_found = ApiError::NotFound("missing".to_string());
        assert_eq!(
            map_session_confirmation_error(&not_found),
            SessionConfirmationState::NotFound
        );

        let structured = ApiError::Structured {
            error: "missing".to_string(),
            code: "NOT_FOUND".to_string(),
            failure_code: None,
            hint: None,
            details: Box::new(None),
            request_id: None,
            error_id: None,
            fingerprint: None,
            session_id: None,
            diag_trace_id: None,
            otel_trace_id: None,
        };
        assert_eq!(
            map_session_confirmation_error(&structured),
            SessionConfirmationState::NotFound
        );
    }

    #[test]
    fn session_confirmation_maps_non_not_found_errors_to_transient_state() {
        let transient_errors = [
            ApiError::Network("connection reset".to_string()),
            ApiError::Server("upstream failure".to_string()),
            ApiError::RateLimited {
                retry_after: Some(2000),
            },
        ];
        for err in transient_errors {
            assert_eq!(
                map_session_confirmation_error(&err),
                SessionConfirmationState::TransientError
            );
        }
    }
}
