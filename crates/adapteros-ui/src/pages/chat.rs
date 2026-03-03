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

use crate::api::{api_base_url, report_error_with_toast, ApiClient, ApiError};
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::layout::nav_group_label_for_route;
use crate::components::status_center::use_status_center;
use crate::components::{
    use_is_tablet_or_smaller, AdapterHeat, AdapterMagnet, AlertBanner, Badge, BadgeVariant,
    BannerVariant, Button, ButtonLink, ButtonSize, ButtonType, ButtonVariant, ChatAdaptersRegion,
    Checkbox, ConfirmationDialog, ConfirmationSeverity, Dialog, IconX, Input, Markdown,
    MarkdownStream, PageBreadcrumbItem, PageScaffold, PageScaffoldStatus, Spinner,
    SuggestedAdapterView, Textarea, TraceButton, TracePanel,
};
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{
    use_chat, use_settings, use_ui_profile, ChatSessionMeta, ChatSessionsManager, ChatTarget,
    StreamNoticeTone,
};
#[cfg(target_arch = "wasm32")]
use crate::utils::status_display_with_raw;
use adapteros_api_types::inference::{
    AdapterAttachReason, AdapterAttachment, DegradedNotice, DegradedNoticeKind, DegradedNoticeLevel,
};
use adapteros_api_types::training::ChatMessageInput;
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
#[cfg(target_arch = "wasm32")]
use leptos_router::hooks::use_params_map;
use std::collections::BTreeSet;
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
const CHAT_SCROLL_BOTTOM_THRESHOLD_PX: i32 = 24;
#[cfg(target_arch = "wasm32")]
const SESSION_CONFIRM_NOT_FOUND_GRACE_MS: u32 = 1200;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AttachMode {
    #[default]
    Upload,
    Paste,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SessionConfirmationState {
    #[default]
    Confirmed,
    PendingConfirm,
    NotFound,
    TransientError,
}

fn map_session_confirmation_error(error: &ApiError) -> SessionConfirmationState {
    if error.is_not_found() {
        SessionConfirmationState::NotFound
    } else {
        SessionConfirmationState::TransientError
    }
}

/// Chat landing page - redirects to the most recent session or shows empty state.
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
                PageBreadcrumbItem::current("Chat"),
            ]
            full_width=true
        >
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>"Loading"</Badge>
            </PageScaffoldStatus>
            <div class="chat-loading-placeholder flex items-center justify-center h-full opacity-50" data-testid="chat-ssr-fallback">
                <Spinner />
            </div>
        </PageScaffold>
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn Chat() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
    let navigate = use_navigate();
    let sessions = ChatSessionsManager::load_sessions();
    let recent_session_id = sessions.first().map(|session| session.id.clone());
    let has_recent_session = recent_session_id.is_some();
    let (system_status, refetch_status) = use_system_status();
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
        <PageScaffold
            title="Chat"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/chat"),
                PageBreadcrumbItem::current("Chat"),
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
        </PageScaffold>
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
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn ChatSession() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/chat").unwrap_or("Chat");
    view! {
        <PageScaffold
            title="Chat Session"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/chat"),
                PageBreadcrumbItem::new("Chat", "/chat"),
                PageBreadcrumbItem::current("Session"),
            ]
            full_width=true
        >
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>"Session"</Badge>
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

    // Defer heavy component tree construction to next tick to break out of the
    // wasm-bindgen-futures task queue context and avoid RefCell re-entrancy.
    let mounted = RwSignal::new(false);
    gloo_timers::callback::Timeout::new(0, move || {
        mounted.set(true);
    })
    .forget();

    view! {
        <PageScaffold
            title="Chat Session"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/chat"),
                PageBreadcrumbItem::new("Chat", "/chat"),
                PageBreadcrumbItem::current("Session"),
            ]
            full_width=true
        >
            <PageScaffoldStatus slot>
                <Badge variant=BadgeVariant::Outline>"Session"</Badge>
            </PageScaffoldStatus>
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
        </PageScaffold>
    }
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
) -> impl IntoView {
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
        <div class="flex h-full min-h-0">
            // Desktop: persistent session list sidebar
            {move || {
                if !is_compact.try_get().unwrap_or(false) {
                    Some(view! {
                        <div class="chat-session-sidebar border-r border-border flex-shrink-0 flex flex-col h-full overflow-hidden">
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
                            session_index_epoch=session_index_epoch_signal
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
                                    class="btn btn-ghost btn-icon-sm p-1.5 rounded hover:bg-muted/50 text-muted-foreground"
                                    on:click=move |_| show_mobile_sessions.set(false)
                                    aria-label="Close session list"
                                >
                                    <IconX class="h-4 w-4"/>
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
        Callback::new(move |open_add_files: bool| {
            let navigate = navigate.clone();
            let action = action.clone();
            // Optimistic navigation: make the URL/session stable immediately, then swap to the
            // server-issued session id once created. This prevents "New Chat" from appearing
            // non-responsive during cold starts or transient backend delays.
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = if open_add_files {
                format!("/chat/{}?add_files=1", placeholder_id)
            } else {
                format!("/chat/{}", placeholder_id)
            };
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                // Create the session in the backend first; inference streaming requires
                // a server-issued session id.
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Conversation".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        // Clean up placeholder (if untouched) before switching to the real id.
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let path = if open_add_files {
                            format!("/chat/{}?add_files=1", session_id)
                        } else {
                            format!("/chat/{}", session_id)
                        };
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
    let start_chat = { Callback::new(move |_: ()| create_session.run(false)) };
    let add_files = { Callback::new(move |_: ()| create_session.run(true)) };
    let go_to_adapters = {
        Callback::new(move |_: ()| {
            navigate("/adapters", Default::default());
        })
    };

    view! {
        <div class="chat-empty-state-panel" data-testid="chat-empty-state">
            <div class="chat-empty-state-content">
                <div class="mx-auto w-14 h-14 shrink-0 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                    <svg xmlns="http://www.w3.org/2000/svg" class="text-primary shrink-0" width="28" height="28" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                    </svg>
                </div>
                <h3 class="heading-3">"Start Chat"</h3>
                <p class="text-sm text-muted-foreground leading-relaxed">
                    "Chat can help you build adapters and produce proof. Start a chat or add files to begin."
                </p>
                <div class="flex items-center justify-center gap-3">
                    <Button on_click=start_chat data_testid="chat-empty-new-chat".to_string()>
                        "Start Chat"
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=add_files
                        data_testid="chat-empty-add-files".to_string()
                    >
                        "Add Files"
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
                        "Browse Adapters (Library)"
                    </a>
                </p>
            </div>
        </div>
    }
}

#[allow(dead_code)]
#[component]
fn ChatUnavailableEntry(
    reason: String,
    action_label: String,
    action_href: String,
    on_retry: Callback<()>,
) -> impl IntoView {
    let status_center = use_status_center();
    let (chat_state, chat_action) = use_chat();
    let (system_status, _) = use_system_status();
    let queued_message = RwSignal::new(String::new());

    let queue_disabled = Signal::derive(move || {
        queued_message
            .try_get()
            .unwrap_or_default()
            .trim()
            .is_empty()
    });
    let queue_submit = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let content = queued_message.try_get().unwrap_or_default();
            if content.trim().is_empty() {
                return;
            }
            action.queue_message(content);
            queued_message.set(String::new());
        })
    };

    view! {
        <div class="flex h-full items-center justify-center p-6" data-testid="chat-unavailable-state">
            <div class="w-full max-w-2xl rounded-lg border border-warning/40 bg-warning/10 p-6">
                <div class="space-y-2">
                    <h2 class="heading-3">"Conversation unavailable"</h2>
                    <p class="text-sm text-muted-foreground" data-testid="chat-unavailable-reason">
                        {reason}"."
                    </p>
                    <p class="text-xs text-muted-foreground" data-testid="chat-unavailable-summary">
                        {move || match system_status.try_get().unwrap_or(LoadingState::Idle) {
                            LoadingState::Loaded(status) => {
                                let boot_phase = status.boot.as_ref()
                                    .map(|b| b.phase.as_str())
                                    .unwrap_or("unknown");
                                let workers = match status.readiness.checks.workers.status {
                                    adapteros_api_types::StatusIndicator::Ready => "ready",
                                    adapteros_api_types::StatusIndicator::NotReady => "not ready",
                                    adapteros_api_types::StatusIndicator::Unknown => "unknown",
                                };
                                let blocker = primary_blocker(&status.inference_blockers)
                                    .map(|b| format!("{:?}", b))
                                    .unwrap_or_else(|| "none".to_string());
                                format!("Boot: {} · Workers: {} · Primary blocker: {}", boot_phase, workers, blocker)
                            }
                            _ => "Boot: unknown · Workers: unknown · Primary blocker: status pending".to_string(),
                        }}
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
                            class="btn btn-link btn-xs text-xs text-muted-foreground hover:text-foreground"
                            on:click=move |_| ctx.open()
                        >
                            "Why?"
                        </button>
                    })}
                </div>
                <div class="mt-4 space-y-2">
                    <label class="text-xs uppercase tracking-wide text-muted-foreground">
                        "Queue a message for when inference is ready"
                    </label>
                    <Textarea
                        value=queued_message
                        placeholder="Ask your question now; it will send automatically once ready..."
                        rows=3
                    />
                    <div class="flex items-center justify-between gap-2">
                        <span class="text-xs text-muted-foreground">
                            {move || format!("Queued: {}", chat_state.try_get().unwrap_or_default().queued_count())}
                        </span>
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Sm
                            disabled=queue_disabled
                            on_click=queue_submit
                            data_testid="chat-unavailable-queue".to_string()
                        >
                            "Queue message"
                        </Button>
                    </div>
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
                    .create_backend_session(name, Some("New Conversation".to_string()))
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
                    .create_backend_session(name, Some("New Conversation".to_string()))
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
                    <h2 class="text-sm font-semibold">"Conversations"</h2>
                    <button
                        class="btn btn-primary btn-sm inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                        on:click=move |_| create_session.run(())
                        title="New conversation"
                        aria-label="New Conversation"
                        data-testid="chat-sidebar-new-session"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
                        </svg>
                        "New Conversation"
                    </button>
                </div>
                <div class="grid grid-cols-2 gap-1 rounded-lg bg-muted/40 p-1">
                    <button
                        class=move || format!(
                            "btn btn-ghost btn-sm px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if !show_archived.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(false); }
                        aria-label="Show active sessions"
                        data-testid="chat-sidebar-toggle-active"
                    >
                        {move || format!("Active ({})", active_count.try_get().unwrap_or(0))}
                    </button>
                    <button
                        class=move || format!(
                            "btn btn-ghost btn-sm px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if show_archived.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(true); }
                        aria-label="Show archived sessions"
                        data-testid="chat-sidebar-toggle-archived"
                    >
                        {move || format!("Archived ({})", archived_count.try_get().unwrap_or(0))}
                    </button>
                </div>
                <div class="space-y-1.5">
                    <button
                        class=move || format!(
                            "btn btn-outline btn-sm w-full inline-flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-semibold rounded-md border transition-colors {}",
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
                        title="Create an adapter from selected conversations"
                        aria-label="Create Adapter From Selected Conversations"
                        data-testid="chat-sidebar-learn"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v4m0 10v4M3 12h4m10 0h4M5.6 5.6l2.8 2.8m7.2 7.2 2.8 2.8m0-12.8-2.8 2.8m-7.2 7.2-2.8 2.8"/>
                        </svg>
                        {move || {
                            if creating_training_dataset.try_get().unwrap_or(false) {
                                "Preparing training data..."
                            } else {
                                "Create Adapter from Selection"
                            }
                        }}
                    </button>
                    <div class="flex items-center justify-between gap-2 text-2xs text-muted-foreground">
                        <span>
                            {move || {
                                let count = selected_training_count.try_get().unwrap_or(0);
                                if count == 0 {
                                    "Select conversations below to enable".to_string()
                                } else {
                                    format!("{} selected", count)
                                }
                            }}
                        </span>
                        <button
                            class="btn btn-link btn-xs underline decoration-dotted hover:text-foreground disabled:no-underline disabled:cursor-not-allowed"
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
                                    <p class="text-xs font-medium truncate">"Continue this draft"</p>
                                    <p class="text-2xs text-muted-foreground">
                                        {move || format!("{} messages", dock_message_count.try_get().unwrap_or(0))}
                                    </p>
                                </div>
                                <button
                                    class="btn btn-outline btn-sm shrink-0 px-2 py-1 text-xs font-medium rounded border border-primary/30 text-primary hover:bg-primary/10 transition-colors"
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
                    let showing_archived = show_archived.try_get().unwrap_or(false);
                    let list = filtered_sessions.try_get().unwrap_or_default();
                    if list.is_empty() {
                        let create_session_empty = create_session;
                        let has_search = !search_query.try_get().unwrap_or_default().is_empty();
                        let message = if has_search {
                            "No matching conversations"
                        } else if showing_archived {
                            "No archived conversations"
                        } else {
                            "No conversations yet"
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
                                            "New conversation"
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
    /// Whether this session is selected for "Create Adapter from Selection"
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
    let training_aria_label = format!("Select '{}' to create an adapter", session_title.clone());
    let archive_action = on_archive;
    let unarchive_action = on_unarchive;

    // Overflow menu state
    let show_overflow = RwSignal::new(false);
    let overflow_trigger_ref = NodeRef::<leptos::html::Button>::new();
    let overflow_menu_ref = NodeRef::<leptos::html::Div>::new();
    let show_permanent_delete_confirm = RwSignal::new(false);
    let hard_delete_session_id = id.clone();

    // Focus first menu item whenever overflow menu opens.
    {
        Effect::new(move |_| {
            if !show_overflow.try_get().unwrap_or(false) {
                return;
            }
            gloo_timers::callback::Timeout::new(0, move || {
                if let Some(menu) = overflow_menu_ref.get() {
                    if let Ok(Some(first_item)) = menu.query_selector(r#"[role="menuitem"]"#) {
                        if let Ok(first_item) = first_item.dyn_into::<web_sys::HtmlElement>() {
                            let _ = first_item.focus();
                        }
                    }
                }
            })
            .forget();
        });
    }

    let handle_overflow_keydown = Callback::new({
        #[allow(clippy::redundant_locals)]
        let overflow_menu_ref = overflow_menu_ref;
        #[allow(clippy::redundant_locals)]
        let overflow_trigger_ref = overflow_trigger_ref;
        move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                show_overflow.set(false);
                if let Some(trigger) = overflow_trigger_ref.get() {
                    let _ = trigger.focus();
                }
            }
            "ArrowDown" | "ArrowUp" => {
                ev.prevent_default();
                ev.stop_propagation();
                let Some(menu) = overflow_menu_ref.get() else {
                    return;
                };
                let Ok(items) = menu.query_selector_all(r#"[role="menuitem"]"#) else {
                    return;
                };
                let len = items.length();
                if len == 0 {
                    return;
                }
                let active = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .and_then(|el| el.dyn_into::<web_sys::Node>().ok());

                let mut current_idx: i32 = -1;
                for idx in 0..len {
                    let Some(node) = items.item(idx) else {
                        continue;
                    };
                    if active
                        .as_ref()
                        .is_some_and(|active_node| node.is_same_node(Some(active_node)))
                    {
                        current_idx = idx as i32;
                        break;
                    }
                }

                let next_idx = if ev.key() == "ArrowDown" {
                    if current_idx < 0 {
                        0
                    } else {
                        ((current_idx + 1) as u32) % len
                    }
                } else if current_idx < 0 {
                    len - 1
                } else {
                    ((current_idx - 1 + len as i32) as u32) % len
                };

                if let Some(node) = items.item(next_idx) {
                    if let Ok(item) = node.dyn_into::<web_sys::HtmlElement>() {
                        let _ = item.focus();
                    }
                }
            }
            _ => {}
        }
    });

    view! {
        <div
            class=move || format!(
                "chat-session-row {}",
                if selected.try_get().unwrap_or(false) {
                    "chat-session-row--active"
                } else {
                    ""
                }
            )
            data-testid="chat-session-row"
        >
            <div
                class="chat-session-row-checkbox"
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

            <a href=href class="chat-session-row-link">
                <h3 class="chat-session-row-title">{session_title}</h3>

                {if !session_preview.is_empty() {
                    let preview = session_preview.clone();
                    Some(view! {
                        <p class="chat-session-row-preview">{preview}</p>
                    })
                } else {
                    None
                }}

                <div class="chat-session-row-meta">
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
            </a>

            <div class="chat-session-row-actions">
                {archive_action.map(|archive| view! {
                    <button
                        class="btn btn-ghost btn-icon-sm chat-session-row-action"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            archive.run(());
                        }
                        title="Archive session"
                        aria-label="Archive session"
                        data-testid="chat-session-archive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 7h18M5 7v10a2 2 0 002 2h10a2 2 0 002-2V7M9 11h6"/>
                        </svg>
                    </button>
                })}
                {unarchive_action.map(|unarchive| view! {
                    <button
                        class="btn btn-ghost btn-icon-sm chat-session-row-action"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            unarchive.run(());
                        }
                        title="Restore session"
                        aria-label="Restore session"
                        data-testid="chat-session-unarchive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 12a8 8 0 118 8M4 12V8m0 4h4"/>
                        </svg>
                    </button>
                })}
                <button
                    class="btn btn-ghost btn-icon-sm chat-session-row-action chat-session-row-action--destructive"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        on_delete.run(());
                    }
                    title="Delete session"
                    aria-label="Delete session"
                    data-testid="chat-session-delete"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                    </svg>
                </button>

                <div class="relative">
                    <button
                        node_ref=overflow_trigger_ref
                        class="btn btn-ghost btn-icon-sm chat-session-row-action chat-session-row-action--muted"
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
                        <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 20 20">
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
                                on:click=move |_| {
                                    show_overflow.set(false);
                                    if let Some(trigger) = overflow_trigger_ref.get() {
                                        let _ = trigger.focus();
                                    }
                                }
                            />
                            <div
                                node_ref=overflow_menu_ref
                                class="absolute right-0 top-full z-50 mt-1 w-40 rounded-md border border-border bg-background shadow-lg py-1"
                                role="menu"
                                on:keydown=move |ev| handle_overflow_keydown.run(ev)
                            >
                                {is_archived.then(|| view! {
                                    <div class="border-t border-border my-1" role="separator" />
                                    <button
                                        class="btn btn-ghost btn-sm w-full text-left px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors"
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

use crate::utils::format_relative_time;

fn generate_readable_id(_prefix: &str, _slug_source: &str) -> String {
    adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string()
}

#[component]
fn ChatConversationMessageItem(
    msg_id: String,
    active_trace: RwSignal<Option<String>>,
) -> impl IntoView {
    let (chat_state, _) = use_chat();
    let compact_layout = use_is_tablet_or_smaller();
    let lookup_id = msg_id.clone();

    let message = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .find(|m| m.id == lookup_id)
            .cloned()
    });

    let streaming_content = Signal::derive(move || {
        message
            .try_get()
            .flatten()
            .filter(|m| m.role == "assistant" && m.is_streaming)
            .map(|m| m.content)
            .unwrap_or_default()
    });

    view! {
        {move || {
            message.try_get().flatten().map(|msg| {
                let is_user = msg.role == "user";
                let is_system = msg.role == "system";
                let is_streaming = msg.is_streaming;
                let trace_id = msg.trace_id.clone();
                let latency_ms = msg.latency_ms;
                let token_count = msg.token_count;
                let prompt_tokens = msg.prompt_tokens;
                let completion_tokens = msg.completion_tokens;
                let citations = msg.citations.clone().unwrap_or_default();
                let document_links = msg.document_links.clone().unwrap_or_default();
                let adapters_used = msg.adapters_used.clone().unwrap_or_default();
                let unavailable_pinned_adapters =
                    msg.unavailable_pinned_adapters.clone().unwrap_or_default();
                let pinned_routing_fallback = msg.pinned_routing_fallback.clone();
                let fallback_triggered = msg.fallback_triggered;
                let fallback_backend = msg.fallback_backend.clone();
                let adapter_attachments = msg.adapter_attachments.clone();
                let degraded_notices = msg.degraded_notices.clone();
                let citation_count = citations.len();
                let document_link_count = document_links.len();
                let has_trust_details = !adapter_attachments.is_empty()
                    || !degraded_notices.is_empty()
                    || !unavailable_pinned_adapters.is_empty()
                    || pinned_routing_fallback.is_some()
                    || fallback_triggered
                    || citation_count > 0
                    || document_link_count > 0
                    || !adapters_used.is_empty();
                let critical_notices: Vec<DegradedNotice> = degraded_notices
                    .iter()
                    .filter(|notice| {
                        notice.meaning_changed && notice.level == DegradedNoticeLevel::Critical
                    })
                    .cloned()
                    .collect();
                let trust_state = if citation_count > 0 || document_link_count > 0 {
                    "chat-provenance-cited"
                } else {
                    "chat-provenance-none"
                };
                let role_label = if is_user {
                    "You"
                } else if is_system {
                    "System"
                } else {
                    "Assistant"
                };

                view! {
                    <div class=format!(
                        "flex {}",
                        if is_user {
                            "justify-end"
                        } else if is_system {
                            "justify-center"
                        } else {
                            "justify-start"
                        }
                    )>
                        <div class=format!(
                            "flex flex-col gap-1.5 max-w-[80%] {}",
                            if is_user {
                                "items-end"
                            } else if is_system {
                                "items-center max-w-full"
                            } else {
                                "items-start"
                            }
                        )>
                            {if is_system {
                                None
                            } else {
                                Some(view! {
                                    <span class="text-2xs uppercase tracking-wider font-medium text-muted-foreground px-1">
                                        {role_label}
                                    </span>
                                })
                            }}
                            <div class=format!(
                                "rounded-lg {} {} {} {} chat-message-bubble {}",
                                if is_system && compact_layout.try_get().unwrap_or(false) {
                                    "px-0 py-0"
                                } else {
                                    "px-4 py-3"
                                },
                                if is_user {
                                    "bg-primary text-primary-foreground shadow-sm"
                                } else if is_system {
                                    "bg-transparent border-0 text-muted-foreground text-xs"
                                } else {
                                    "bg-muted/50 border border-border chat-message--assistant"
                                },
                                // Add min-height during streaming to prevent layout jump
                                if is_streaming { "min-h-[2.5rem]" } else { "" },
                                if is_system { "chat-message-system" } else { "" },
                                trust_state
                            )>
                                {if is_user {
                                    view! {
                                        <p class="text-sm whitespace-pre-wrap break-words leading-relaxed">
                                            {msg.content.clone()}
                                        </p>
                                    }.into_any()
                                } else if is_streaming {
                                    let has_content = !streaming_content.try_get().unwrap_or_default().is_empty();
                                    view! {
                                        <div class="text-sm break-words leading-relaxed">
                                            <MarkdownStream
                                                content=Signal::derive(move || streaming_content.try_get().unwrap_or_default())
                                            />
                                            {if has_content {
                                                view! {
                                                    <span class="inline-block animate-pulse text-primary/70 ml-0.5">"▍"</span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span class="inline-flex items-center gap-1.5 text-muted-foreground">
                                                        <Spinner/>
                                                        <span class="text-xs">"Preparing response..."</span>
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
                            {if !is_user && !is_system && !is_streaming {
                                let latency = latency_ms.unwrap_or(0);
                                let trace = trace_id.clone();
                                let run_overview_url = trace.clone().map(|tid| format!("/runs/{}", tid));
                                let run_receipt_url = trace.clone().map(|tid| format!("/runs/{}?tab=receipt", tid));
                                let run_replay_url = trace.clone().map(|tid| format!("/runs/{}?tab=replay", tid));
                                Some(view! {
                                    {if !critical_notices.is_empty() {
                                        let title = prominent_degraded_title(&critical_notices).to_string();
                                        let alert_messages: Vec<String> = critical_notices
                                            .iter()
                                            .map(|notice| notice.message.clone())
                                            .collect::<BTreeSet<_>>()
                                            .into_iter()
                                            .collect();
                                        Some(view! {
                                            <div
                                                class="w-full rounded-lg border-2 border-destructive/60 bg-destructive/10 px-3 py-2 mb-1"
                                                data-testid="chat-meaning-change-alert"
                                            >
                                                <p class="text-xs font-semibold uppercase tracking-wide text-destructive">
                                                    {title}
                                                </p>
                                                <div class="mt-1 space-y-0.5">
                                                    {alert_messages.into_iter().map(|message| {
                                                        view! {
                                                            <p class="text-xs text-foreground leading-relaxed">
                                                                {message}
                                                            </p>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }}
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
                                                    title="View Execution Record"
                                                    data-testid="chat-run-link"
                                                >
                                                    "Execution Record"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Execution record unavailable"
                                                    data-testid="chat-run-link"
                                                >
                                                    "Execution Record"
                                                </span>
                                            }.into_any())}
                                            <span class="text-muted-foreground/50">"·"</span>
                                            {run_receipt_url.map(|url| view! {
                                                <a
                                                    href=url
                                                    class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                    title="View Execution Receipt (signed log / proof)"
                                                    data-testid="chat-receipt-link"
                                                >
                                                    "Execution Receipt"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Execution receipt unavailable (signed log / proof unavailable)"
                                                    data-testid="chat-receipt-link"
                                                >
                                                    "Execution Receipt"
                                                </span>
                                            }.into_any())}
                                            <span class="text-muted-foreground/50">"·"</span>
                                            {run_replay_url.clone().map(|url| view! {
                                                <a
                                                    href=url
                                                    class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                    title="Replay this response exactly"
                                                    data-testid="chat-replay-link"
                                                >
                                                    "Replay Exactly"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Replay unavailable"
                                                    data-testid="chat-replay-link"
                                                >
                                                    "Replay Exactly"
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
                                        {if has_trust_details {
                                            let summary = trust_summary_label(
                                                citation_count,
                                                document_link_count,
                                                &adapter_attachments,
                                                &adapters_used,
                                                degraded_notices.len(),
                                            );
                                            Some(view! {
                                                <div class="flex items-start gap-1.5 w-full flex-col" data-testid="chat-adapter-chips">
                                                    <span class="text-2xs text-muted-foreground" data-testid="chat-citation-chips">
                                                        {summary}
                                                    </span>
                                                    <ChatTrustPanel
                                                        citations=citations
                                                        document_links=document_links
                                                        adapters_used=adapters_used
                                                        adapter_attachments=adapter_attachments
                                                        degraded_notices=degraded_notices
                                                        unavailable_pinned_adapters=unavailable_pinned_adapters
                                                        pinned_routing_fallback=pinned_routing_fallback
                                                        fallback_triggered=fallback_triggered
                                                        fallback_backend=fallback_backend
                                                    />
                                                </div>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                })
                            } else {
                                None
                            }}
                        </div>
                    </div>
                }
            })
        }}
    }
}

#[component]
fn ChatTrustPanel(
    citations: Vec<crate::signals::chat::ChatCitation>,
    document_links: Vec<crate::signals::chat::ChatDocumentLink>,
    adapters_used: Vec<String>,
    adapter_attachments: Vec<AdapterAttachment>,
    degraded_notices: Vec<DegradedNotice>,
    unavailable_pinned_adapters: Vec<String>,
    pinned_routing_fallback: Option<String>,
    fallback_triggered: bool,
    fallback_backend: Option<String>,
) -> impl IntoView {
    let dataset_versions: Vec<String> = document_links
        .iter()
        .filter_map(|link| link.dataset_version_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut effective_notices = degraded_notices;

    if !unavailable_pinned_adapters.is_empty()
        && !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::BlockedPins)
    {
        effective_notices.push(DegradedNotice {
            kind: DegradedNoticeKind::BlockedPins,
            level: DegradedNoticeLevel::Warning,
            message: format!(
                "{} pinned adapter(s) were unavailable.",
                unavailable_pinned_adapters.len()
            ),
            meaning_changed: true,
        });
    }

    if let Some(mode) = pinned_routing_fallback.clone() {
        if !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::RoutingOverride)
        {
            effective_notices.push(DegradedNotice {
                kind: DegradedNoticeKind::RoutingOverride,
                level: DegradedNoticeLevel::Warning,
                message: format!("Routing override applied with mode: {mode}."),
                meaning_changed: true,
            });
        }
    }

    if fallback_triggered
        && !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::WorkerSemanticFallback)
    {
        let backend_label = fallback_backend
            .clone()
            .unwrap_or_else(|| "another backend".to_string());
        effective_notices.push(DegradedNotice {
            kind: DegradedNoticeKind::WorkerSemanticFallback,
            level: DegradedNoticeLevel::Critical,
            message: format!("Worker fallback changed execution backend to {backend_label}."),
            meaning_changed: true,
        });
    }

    view! {
        <details class="w-full rounded-md border border-border/60 bg-muted/20 px-2.5 py-2 mt-0.5" data-testid="chat-trust-panel">
            <summary class="cursor-pointer select-none text-2xs font-medium text-muted-foreground">
                "Trust details"
            </summary>
            <div class="mt-2 space-y-3">
                {if !adapter_attachments.is_empty() || !adapters_used.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-adapters">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Why adapters were used"
                            </p>
                            {if !adapter_attachments.is_empty() {
                                adapter_attachments.into_iter().map(|attachment| {
                                    let display_name = attachment
                                        .adapter_label
                                        .clone()
                                        .unwrap_or_else(|| short_adapter_label(&attachment.adapter_id));
                                    let reason_label = attach_reason_label(&attachment.attach_reason);
                                    let reason_detail = attach_reason_detail(&attachment.attach_reason);
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs font-medium text-foreground">{display_name}</span>
                                                <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                    {reason_label}
                                                </span>
                                            </div>
                                            <p class="text-2xs text-muted-foreground leading-relaxed">
                                                {reason_detail}
                                            </p>
                                            <p class="text-[11px] text-muted-foreground mt-1">
                                                "Version: "
                                                <span class="font-mono">
                                                    {attachment
                                                        .adapter_version_id
                                                        .clone()
                                                        .unwrap_or_else(|| "not pinned".to_string())}
                                                </span>
                                            </p>
                                            <p class="text-[10px] text-muted-foreground/80 mt-0.5">
                                                "ID: "
                                                <span class="font-mono">{attachment.adapter_id.clone()}</span>
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                }).collect::<Vec<_>>()
                            } else {
                                adapters_used.into_iter().map(|adapter_id| {
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs font-medium text-foreground">
                                                    {short_adapter_label(&adapter_id)}
                                                </span>
                                                <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                    "used"
                                                </span>
                                            </div>
                                            <p class="text-[10px] text-muted-foreground/80 mt-0.5">
                                                "ID: "
                                                <span class="font-mono">{adapter_id}</span>
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                }).collect::<Vec<_>>()
                            }}
                        </div>
                    })
                } else {
                    None
                }}

                {if !effective_notices.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-degraded">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Degraded or failed states"
                            </p>
                            {effective_notices.into_iter().map(|notice| {
                                let level_class = degraded_level_class(&notice.level);
                                view! {
                                    <div class=format!(
                                        "rounded-md border px-2 py-1.5 {}",
                                        level_class
                                    )>
                                        <div class="flex items-center justify-between gap-2 flex-wrap">
                                            <span class="text-xs font-medium text-foreground">
                                                {degraded_kind_label(&notice.kind)}
                                            </span>
                                            <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                {degraded_level_label(&notice.level)}
                                            </span>
                                        </div>
                                        <p class="text-2xs text-muted-foreground leading-relaxed">
                                            {notice.message}
                                        </p>
                                        {if notice.meaning_changed {
                                            Some(view! {
                                                <p class="text-[11px] text-warning-foreground mt-1">
                                                    "Meaning changed from the requested path."
                                                </p>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })
                } else {
                    None
                }}

                {if !document_links.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-documents">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Source documents"
                            </p>
                            {if !dataset_versions.is_empty() {
                                Some(view! {
                                    <div class="flex flex-wrap items-center gap-1">
                                        {dataset_versions.into_iter().map(|dataset_version| {
                                            view! {
                                                <span class="text-2xs rounded bg-muted px-1.5 py-0.5 text-muted-foreground">
                                                    "Dataset version "
                                                    <span class="font-mono">{dataset_version}</span>
                                                </span>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                })
                            } else {
                                None
                            }}
                            <div class="flex flex-col gap-1" data-testid="chat-document-links">
                                {document_links.into_iter().map(|link| {
                                    let document_name = link.document_name.clone();
                                    let download_url = link.download_url.clone();
                                    let dataset_version = link.dataset_version_id.clone();
                                    let source_file = link.source_file.clone();
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <a
                                                href=download_url
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                class="text-xs text-primary hover:underline"
                                                title="Open source document"
                                            >
                                                {document_name}
                                            </a>
                                            <div class="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
                                                {dataset_version.map(|dataset_version| view! {
                                                    <span class="text-[11px] text-muted-foreground">
                                                        "Dataset version: "
                                                        <span class="font-mono">{dataset_version}</span>
                                                    </span>
                                                })}
                                                {source_file.map(|source_file| view! {
                                                    <span class="text-[11px] text-muted-foreground">
                                                        "Source file: "
                                                        <span>{source_file}</span>
                                                    </span>
                                                })}
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }}

                {if !citations.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-citations">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Citation spans"
                            </p>
                            <div class="flex flex-col gap-1">
                                {citations.into_iter().map(|citation| {
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs text-foreground">
                                                    {citation_page_span_label(&citation)}
                                                </span>
                                                {citation.rank.map(|rank| view! {
                                                    <span class="text-2xs text-muted-foreground">
                                                        "Rank "
                                                        {rank}
                                                    </span>
                                                })}
                                            </div>
                                            <p class="text-[11px] text-muted-foreground mt-0.5 truncate">
                                                {citation.file_path}
                                            </p>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }}
            </div>
        </details>
    }
}

/// Chat conversation panel - renders the full conversation experience for a session.
/// Used by both /chat and /chat/:session_id routes through the ChatWorkspace layout.
#[component]
fn ChatConversationPanel(
    /// Reactive session ID signal
    session_id_signal: Signal<String>,
    /// Monotonic epoch for local session index changes.
    session_index_epoch: Signal<u64>,
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
    let settings = use_settings();
    let (system_status, _refetch_status) = use_system_status();
    let status_center = use_status_center();
    let is_compact_view = use_is_tablet_or_smaller();
    let show_mobile_config_details = RwSignal::new(false);

    // Local state for input and trace panel
    let message = RwSignal::new(String::new());
    let active_trace = RwSignal::new(Option::<String>::None);
    let session_loaded = RwSignal::new(false);
    let current_session_id = RwSignal::new(String::new());
    let session_confirmation_state = RwSignal::new(SessionConfirmationState::Confirmed);
    let session_inline_notice = RwSignal::new(Option::<String>::None);
    let session_confirmation_nonce = RwSignal::new(0_u64);
    let session_confirmation_retry_epoch = RwSignal::new(0_u64);
    let session_confirmation_attempt = RwSignal::new(0_u64);
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
    let bit_identical_mode_blocked = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .bit_identical_mode_blocked
    });
    let bit_identical_mode_degraded = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .bit_identical_mode_degraded
    });
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

    // Load session from localStorage when session ID or local session index changes.
    {
        let action = chat_action.clone();
        Effect::new(move |prev_effect_key: Option<(String, u64, u64)>| {
            let id = session_id();
            let observe_session_epoch = matches!(
                session_confirmation_state
                    .try_get()
                    .unwrap_or(SessionConfirmationState::Confirmed),
                SessionConfirmationState::PendingConfirm
                    | SessionConfirmationState::TransientError
                    | SessionConfirmationState::NotFound
            );
            let session_epoch = if observe_session_epoch {
                session_index_epoch.try_get().unwrap_or(0)
            } else {
                0
            };
            let retry_epoch = session_confirmation_retry_epoch.try_get().unwrap_or(0);
            let effect_key = (id.clone(), session_epoch, retry_epoch);

            // Handle empty/invalid session ID - redirect to landing page
            if id.is_empty() {
                web_sys::console::warn_1(
                    &"[ChatSession] Empty session ID, redirecting to /chat".into(),
                );
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/chat");
                }
                return effect_key;
            }

            // Skip if both session and trigger epochs are unchanged.
            if prev_effect_key.as_ref() == Some(&effect_key) {
                return effect_key;
            }

            let session_changed = prev_effect_key
                .as_ref()
                .map(|(prev_id, _, _)| prev_id != &id)
                .unwrap_or(true);

            // Clear any existing messages from a different session before loading
            if let Some((ref prev, _, _)) = prev_effect_key {
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
                return effect_key;
            }

            current_session_id.set(id.clone());
            action.set_session_id(Some(id.clone()));
            if session_changed {
                session_loaded.set(false);
                session_inline_notice.set(None);
            }

            // Try to load session from localStorage
            if let Some(stored) = ChatSessionsManager::load_session(&id) {
                let msg_count = stored.messages.len();
                let is_stub = msg_count == 0 && !stored.placeholder;
                action.restore_session(stored);
                session_confirmation_state.set(SessionConfirmationState::Confirmed);
                session_inline_notice.set(None);
                session_confirmation_nonce.update(|nonce| *nonce = nonce.wrapping_add(1));
                crate::debug_log!("[Chat] Restored session {} with {} messages", id, msg_count);
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
                session_confirmation_state.set(SessionConfirmationState::PendingConfirm);
                session_inline_notice.set(None);
                let nonce = session_confirmation_nonce
                    .try_get_untracked()
                    .unwrap_or(0)
                    .wrapping_add(1);
                session_confirmation_nonce.set(nonce);
                let attempt = session_confirmation_attempt
                    .try_get_untracked()
                    .unwrap_or(0)
                    .wrapping_add(1);
                session_confirmation_attempt.set(attempt);
                crate::debug_log!(
                    "[ChatSessionConfirm] state=pending session={} attempt={} source=local_miss",
                    id,
                    attempt
                );

                let action = action.clone();
                let id = id.clone();
                let current_session_id = current_session_id;
                let session_confirmation_nonce = session_confirmation_nonce;
                let session_confirmation_state = session_confirmation_state;
                let session_inline_notice = session_inline_notice;
                wasm_bindgen_futures::spawn_local(async move {
                    #[cfg(target_arch = "wasm32")]
                    gloo_timers::future::TimeoutFuture::new(SESSION_CONFIRM_NOT_FOUND_GRACE_MS)
                        .await;

                    let still_current = current_session_id.try_get_untracked().unwrap_or_default()
                        == id
                        && session_confirmation_nonce.try_get_untracked().unwrap_or(0) == nonce;
                    if !still_current {
                        return;
                    }

                    if let Some(stored_after_grace) = ChatSessionsManager::load_session(&id) {
                        action.restore_session(stored_after_grace);
                        session_confirmation_state.set(SessionConfirmationState::Confirmed);
                        session_inline_notice.set(None);
                        crate::debug_log!(
                            "[ChatSessionConfirm] state=confirmed session={} attempt={} source=local_after_grace",
                            id,
                            attempt
                        );
                        return;
                    }

                    match action.get_backend_session(&id).await {
                        Ok(backend_session) => {
                            let still_current =
                                current_session_id.try_get_untracked().unwrap_or_default() == id
                                    && session_confirmation_nonce.try_get_untracked().unwrap_or(0)
                                        == nonce;
                            if !still_current {
                                return;
                            }
                            let _ = ChatSessionsManager::merge_backend_sessions(
                                std::slice::from_ref(&backend_session),
                            );
                            refresh_sessions.run(());
                            if let Some(restored) = ChatSessionsManager::load_session(&id) {
                                action.restore_session(restored);
                            }
                            session_confirmation_state.set(SessionConfirmationState::Confirmed);
                            session_inline_notice.set(None);
                            crate::debug_log!(
                                "[ChatSessionConfirm] state=confirmed session={} attempt={} source=backend_probe",
                                id,
                                attempt
                            );
                        }
                        Err(e) => {
                            let still_current =
                                current_session_id.try_get_untracked().unwrap_or_default() == id
                                    && session_confirmation_nonce.try_get_untracked().unwrap_or(0)
                                        == nonce;
                            if !still_current {
                                return;
                            }
                            let mapped = map_session_confirmation_error(&e);
                            session_confirmation_state.set(mapped);
                            session_inline_notice.set(None);
                            let outcome = match mapped {
                                SessionConfirmationState::NotFound => "not_found",
                                SessionConfirmationState::TransientError => "transient",
                                SessionConfirmationState::Confirmed => "confirmed",
                                SessionConfirmationState::PendingConfirm => "pending",
                            };
                            crate::debug_log!(
                                "[ChatSessionConfirm] state={} session={} attempt={} error={}",
                                outcome,
                                id,
                                attempt,
                                e
                            );
                        }
                    }
                });
            }

            // Check for ?prompt=, ?adapter=, and ?add_files=1 query parameters once per session ID.
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
                                        return effect_key.clone();
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
                                    session_inline_notice.set(Some(format!(
                                        "Prompt too long ({} characters). Maximum is {} characters.",
                                        decoded.len(),
                                        MAX_URL_PROMPT_LENGTH
                                    )));
                                    return effect_key;
                                }
                                if !decoded.is_empty() {
                                    action.send_message_streaming(decoded);
                                    consumed_any = true;
                                }
                            }

                            // Handle ?add_files=1 parameter
                            if let Some(add_files) = params.get("add_files") {
                                if add_files == "1" || add_files.eq_ignore_ascii_case("true") {
                                    show_attach_dialog.set(true);
                                    consumed_any = true;
                                }
                            }
                        }
                    }
                }
                if consumed_any {
                    query_params_consumed_for_session.set(Some(id.clone()));
                    // Drop one-shot params (`prompt`, `add_files`) from the URL to avoid accidental re-run
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
            effect_key
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
                        session_confirmation_state.set(SessionConfirmationState::Confirmed);
                        session_inline_notice.set(None);
                        session_confirmation_nonce.update(|nonce| *nonce = nonce.wrapping_add(1));
                        let session = ChatSessionsManager::session_from_state(&id, &state);
                        ChatSessionsManager::save_session(&session);
                        refresh_sessions.run(());
                        crate::debug_log!(
                            "[Chat] Auto-saved session {} ({} messages)",
                            id,
                            msg_count
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
    // TODO(chat-session-confirmation): Optionally gate send on `Confirmed` only if product
    // decides to enforce strict server-confirmed sessions. Kept permissive for now to avoid
    // behavior regressions in optimistic/new-session flows.
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
    let base_model_badge = Signal::derive(move || {
        format!(
            "Base model: {}",
            base_model_label.try_get().unwrap_or_default()
        )
    });
    let context_model_label = Signal::derive(move || {
        let model = base_model_label
            .try_get()
            .unwrap_or_else(|| "Auto".to_string());
        if model.chars().count() > 20 {
            format!("{}…", model.chars().take(20).collect::<String>())
        } else {
            model
        }
    });
    let context_adapter_label = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        let mut pinned = state.pinned_adapters.clone();
        for id in &state.session_pinned_adapters {
            if !pinned.contains(id) {
                pinned.push(id.clone());
            }
        }

        let primary = if state.verified_mode {
            pinned.first().cloned()
        } else {
            state
                .selected_adapter
                .clone()
                .or_else(|| pinned.first().cloned())
        };
        let compact = primary.map(|value| {
            if value.chars().count() > 18 {
                format!("{}…", value.chars().take(18).collect::<String>())
            } else {
                value
            }
        });

        if state.verified_mode {
            match compact {
                Some(label) => format!("{label} (pinned)"),
                None => "No pinned adapter".to_string(),
            }
        } else {
            compact.unwrap_or_else(|| "Auto".to_string())
        }
    });
    let context_mode_label = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        if !state.verified_mode {
            "Best-Effort".to_string()
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            "Strict-Replayable".to_string()
        } else {
            "Bit-Identical".to_string()
        }
    });
    let context_mode_variant = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        if !state.verified_mode {
            BadgeVariant::Secondary
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Success
        }
    });

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

    // Message log scroll management
    let message_log_ref = NodeRef::<leptos::html::Div>::new();
    let is_at_bottom = RwSignal::new(true);

    // Keyed message IDs for efficient message list updates.
    let message_ids = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    });

    let latest_trace_id = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .rev()
            .find_map(|msg| msg.trace_id.clone())
    });

    let latest_replay_url = Signal::derive(move || {
        latest_trace_id
            .try_get()
            .flatten()
            .map(|trace_id| format!("/runs/{}?tab=replay", trace_id))
    });

    let latest_signed_log_url = Signal::derive(move || {
        latest_trace_id
            .try_get()
            .flatten()
            .map(|trace_id| format!("/runs/{}?tab=receipt", trace_id))
    });

    // Track tail updates so auto-scroll follows streaming token appends.
    let message_tail_signature = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        let tail = state
            .messages
            .last()
            .map(|m| (m.id.clone(), m.content.len(), m.is_streaming));
        (state.messages.len(), tail)
    });

    let scroll_to_latest = Callback::new(move |_: ()| {
        if let Some(el) = message_log_ref.get() {
            el.set_scroll_top(el.scroll_height());
            let _ = is_at_bottom.try_set(true);
        }
    });

    {
        Effect::new(move |_| {
            let _ = message_tail_signature.try_get();
            if !is_at_bottom.try_get().unwrap_or(true) {
                return;
            }
            if let Some(el) = message_log_ref.get() {
                let el_clone = el.clone();
                gloo_timers::callback::Timeout::new(10, move || {
                    el_clone.set_scroll_top(el_clone.scroll_height());
                    let _ = is_at_bottom.try_set(true);
                })
                .forget();
            }
        });
    }

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
    let retry_session_confirmation = {
        Callback::new(move |_: ()| {
            let id = current_session_id.try_get_untracked().unwrap_or_default();
            if id.is_empty() {
                return;
            }
            session_confirmation_state.set(SessionConfirmationState::PendingConfirm);
            session_inline_notice.set(None);
            session_confirmation_retry_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
            crate::debug_log!(
                "[ChatSessionConfirm] state=pending session={} source=manual_retry",
                id
            );
        })
    };

    // Keep persistent knowledge collection from user settings in chat state.
    {
        let action = chat_action.clone();
        Effect::new(move || {
            let knowledge = settings
                .try_get()
                .and_then(|s| s.knowledge_collection_id.clone());
            action.set_knowledge_collection_id(knowledge);
        });
    }

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
        #[cfg(target_arch = "wasm32")]
        let chat_action = chat_action.clone();
        Callback::new(move |_: ()| {
            attach_error.set(None);
            let mode = attach_mode.try_get().unwrap_or(AttachMode::Upload);
            #[cfg(target_arch = "wasm32")]
            let knowledge_collection_id = chat_state
                .try_get()
                .and_then(|s| s.knowledge_collection_id.clone());
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
                        let chat_action = chat_action.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let _base_model_param = base_model_param.clone();
                        let knowledge_collection_id = knowledge_collection_id.clone();

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
                                                    attach_status.set(Some(
                                                        "Creating chat collection...".to_string(),
                                                    ));
                                                    let collection_name =
                                                        if knowledge_collection_id.is_some() {
                                                            format!("Chat: {} (merged)", file_name)
                                                        } else {
                                                            format!("Chat: {}", file_name)
                                                        };
                                                    match client
                                                        .create_collection(&crate::api::types::CreateCollectionRequest {
                                                            name: collection_name,
                                                            description: Some("Auto-created from chat attachment".to_string()),
                                                        })
                                                        .await
                                                    {
                                                        Ok(collection) => {
                                                            if let Some(knowledge_id) = &knowledge_collection_id {
                                                                attach_status.set(Some("Merging knowledge sources...".to_string()));
                                                                if let Ok(knowledge) = client.get_collection(knowledge_id).await {
                                                                    for source_doc in knowledge.documents {
                                                                        let _ = client
                                                                            .add_document_to_collection(
                                                                                &collection.collection_id,
                                                                                &source_doc.document_id,
                                                                            )
                                                                            .await;
                                                                    }
                                                                }
                                                            }
                                                            if let Err(e) = client
                                                                .add_document_to_collection(&collection.collection_id, &doc_id)
                                                                .await
                                                            {
                                                                attach_error.set(Some(format!(
                                                                    "Collection created but failed to attach document: {}",
                                                                    e
                                                                )));
                                                                attach_busy.set(false);
                                                                attach_status.set(None);
                                                                return;
                                                            }
                                                            chat_action.set_active_collection_id(Some(collection.collection_id.clone()));
                                                            let system_message = crate::signals::ChatMessage {
                                                                id: format!("sys-{}", uuid::Uuid::new_v4().simple()),
                                                                role: "system".to_string(),
                                                                content: format!(
                                                                    "📎 {} added ({} chunks). I can now answer questions about this document.",
                                                                    file_name, chunk_count
                                                                ),
                                                                timestamp: crate::utils::now_utc(),
                                                                is_streaming: false,
                                                                status: crate::signals::MessageStatus::Complete,
                                                                queued_at: None,
                                                                pending_phase: crate::signals::PendingPhase::Calm,
                                                                pending_reason: None,
                                                                trace_id: None,
                                                                latency_ms: None,
                                                                token_count: None,
                                                                prompt_tokens: None,
                                                                completion_tokens: None,
                                                                backend_used: None,
                                                                citations: None,
                                                                document_links: None,
                                                                has_citations: false,
                                                                adapters_used: None,
                                                                unavailable_pinned_adapters: None,
                                                                pinned_routing_fallback: None,
                                                                fallback_triggered: false,
                                                                fallback_backend: None,
                                                                adapter_attachments: Vec::new(),
                                                                degraded_notices: Vec::new(),
                                                                replay_status: None,
                                                                policy_warnings: Vec::new(),
                                                            };
                                                            chat_action.append_message(system_message);
                                                        }
                                                        Err(e) => {
                                                            attach_error.set(Some(format!(
                                                                "Failed to create collection for chat RAG: {}",
                                                                e
                                                            )));
                                                            attach_busy.set(false);
                                                            attach_status.set(None);
                                                            return;
                                                        }
                                                    }
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
                                                        status_display_with_raw(&status.status)
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
                    attach_status.set(Some("Preparing your text...".to_string()));

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
                                        "/training?open_wizard=1&dataset_id={}{}",
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
                                        .set(Some(format!("Couldn't prepare your text: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        attach_error.set(Some(
                            "Text processing is only available in the web UI.".to_string(),
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
                    attach_status.set(Some("Preparing selected messages...".to_string()));

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
                                        "/training?open_wizard=1&dataset_id={}{}",
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
                                    attach_error.set(Some(format!(
                                        "Couldn't prepare selected messages: {}",
                                        e
                                    )));
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
                            "Chat processing is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
            }
        })
    };

    view! {
        <div class="p-4 flex h-full min-h-0 flex-col gap-3">
            // Header
            <div
                class="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-3"
                data-testid="chat-header"
            >
                <div class="flex items-center gap-2 text-xs text-muted-foreground">
                    <span class="uppercase tracking-wider text-2xs font-medium">"Session"</span>
                    <span
                        class="font-mono bg-muted/30 px-1.5 py-0.5 rounded text-2xs"
                        data-testid="chat-session-id-label"
                    >
                        {session_label}
                    </span>
                </div>
                <div class="chat-header-controls">
                    // Target selector for choosing model, stack, or policy pack
                    <div class="chat-header-target">
                        <ChatTargetSelector inline=true/>
                    </div>
                    <Badge variant=BadgeVariant::Outline class="chat-header-base-model">
                        <span
                            class="chat-header-base-model-label"
                            title=move || base_model_badge.try_get().unwrap_or_default()
                        >
                            {move || base_model_badge.try_get().unwrap_or_default()}
                        </span>
                    </Badge>
                    <div class="chat-header-mode-toggle flex items-center rounded-full border border-border bg-muted/30 p-0.5 text-xs">
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        } else {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(false)
                                    type="button"
                                >
                                    "Best-Effort"
                                </button>
                            }
                        }
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            if bit_identical_mode_blocked.try_get().unwrap_or(false) {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-destructive/15 text-destructive shadow-sm".to_string()
                                            } else if bit_identical_mode_degraded.try_get().unwrap_or(false) {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-warning/15 text-warning-foreground shadow-sm".to_string()
                                            } else {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                            }
                                        } else {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(true)
                                    type="button"
                                >
                                    {move || {
                                        if bit_identical_mode_blocked.try_get().unwrap_or(false)
                                            || bit_identical_mode_degraded
                                                .try_get()
                                                .unwrap_or(false)
                                        {
                                            "Strict-Replayable"
                                        } else {
                                            "Bit-Identical"
                                        }
                                    }}
                                </button>
                            }
                        }
                    </div>
                    // Status badge
                    <div class="chat-header-status" data-testid="chat-status-badge">
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
            </div>
            <div
                class="flex flex-wrap items-center gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2"
                data-testid="chat-context-strip"
            >
                <span class="text-[11px] uppercase tracking-wide text-muted-foreground">
                    "Current context"
                </span>
                <Badge variant=BadgeVariant::Outline>
                    <span class="text-xs">
                        "Model: "
                        {move || context_model_label.try_get().unwrap_or_else(|| "Auto".to_string())}
                    </span>
                </Badge>
                <Badge variant=BadgeVariant::Outline>
                    <span class="text-xs">
                        "Adapter: "
                        {move || context_adapter_label.try_get().unwrap_or_else(|| "Auto".to_string())}
                    </span>
                </Badge>
                {move || {
                    let variant = context_mode_variant
                        .try_get()
                        .unwrap_or(BadgeVariant::Secondary);
                    let label = context_mode_label
                        .try_get()
                        .unwrap_or_else(|| "Best-Effort".to_string());
                    view! {
                        <Badge variant=variant>
                            <span class="text-xs">{label}</span>
                        </Badge>
                    }
                }}
            </div>

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
                        {if is_warning {
                            view! {
                                <AlertBanner
                                    title="Stream warning"
                                    message=message.clone()
                                    variant=BannerVariant::Warning
                                />
                            }.into_any()
                        } else {
                            view! {
                                <Badge variant=variant>{message}</Badge>
                            }.into_any()
                        }}
                    </div>
                })
            }}

            // Pause notice + navigation to review flow
            {move || {
                let state = chat_state.try_get().unwrap_or_default();
                let _pause = state.paused_inference.clone()?;
                let message = state
                    .stream_notice
                    .clone()
                    .map(|n| n.message)
                    .unwrap_or_else(|| "Paused: Awaiting review".to_string());
                Some(view! {
                    <div class="flex items-center gap-3 text-xs" data-testid="chat-paused-notice">
                        <Badge variant=BadgeVariant::Warning>"Paused"</Badge>
                        <span class="text-muted-foreground truncate">{message}</span>
                    </div>
                })
            }}

            // Pending-adapter badge: rendered outside the collapsed <details>
            // so it's always visible when adapter_selection_pending is true.
            {move || adapter_selection_pending.try_get().unwrap_or(false).then(|| view! {
                <span
                    class="chat-adapters-pending-badge"
                    role="status"
                    aria-label="Adapter changes pending confirmation"
                >
                    "Pending next message"
                </span>
            })}

            <details class="rounded-lg border border-border/60 bg-card/60 px-3 py-2" data-testid="chat-advanced-adapter-controls">
                <summary class="cursor-pointer text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    "Advanced adapter controls"
                </summary>
                <div class="mt-3">
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
                </div>
            </details>

            {move || {
                latest_replay_url.try_get().flatten().map(|replay_href| {
                    let signed_log_href = latest_signed_log_url
                        .try_get()
                        .flatten()
                        .unwrap_or_else(|| "/runs".to_string());
                    view! {
                        <div
                            class="rounded-lg border border-primary/25 bg-primary/5 px-4 py-3"
                            data-testid="chat-replay-proof-banner"
                        >
                            <div class="flex flex-wrap items-center justify-between gap-3">
                                <div class="space-y-1">
                                    <p class="text-sm font-medium">"Replay + execution receipt ready"</p>
                                    <p class="text-xs text-muted-foreground">
                                        "You can replay the latest response with locked output and review its execution receipt."
                                    </p>
                                </div>
                                <div class="flex items-center gap-2">
                                    <ButtonLink
                                        href=replay_href
                                        variant=ButtonVariant::Primary
                                        size=ButtonSize::Sm
                                    >
                                        "Replay Exact Response"
                                    </ButtonLink>
                                    <ButtonLink
                                        href=signed_log_href
                                        variant=ButtonVariant::Outline
                                        size=ButtonSize::Sm
                                    >
                                        "View Execution Receipt"
                                    </ButtonLink>
                                </div>
                            </div>
                        </div>
                    }
                })
            }}

            // Messages
            <div class="relative flex-1 min-h-0">
                <div
                    node_ref=message_log_ref
                    class="flex-1 h-full overflow-y-auto rounded-lg border border-border bg-card"
                    role="log"
                    aria-live="polite"
                    aria-label="Chat messages"
                    on:scroll=move |_| {
                        if let Some(el) = message_log_ref.get() {
                            let distance = el.scroll_height() - el.scroll_top() - el.client_height();
                            let _ = is_at_bottom.try_set(distance <= CHAT_SCROLL_BOTTOM_THRESHOLD_PX);
                        }
                    }
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
                                        class="btn btn-ghost btn-icon-sm chat-overflow-notice-dismiss"
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
                <div class="p-4">
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
                                        <div class="mx-auto w-14 h-14 shrink-0 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="text-primary shrink-0"
                                                width="28"
                                                height="28"
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
                                            <h3 class="heading-4 text-foreground">"Start Chat"</h3>
                                            <p class="text-sm text-muted-foreground leading-relaxed">
                                                "Ask your first question, add files for context, or browse adapters."
                                            </p>
                                        </div>
                                        <div class="flex flex-wrap justify-center gap-2 pt-2">
                                            <Button
                                                size=ButtonSize::Sm
                                                on_click=Callback::new(move |_| {
                                                    #[cfg(target_arch = "wasm32")]
                                                    {
                                                        if let Some(window) = web_sys::window() {
                                                            if let Some(document) = window.document() {
                                                                if let Ok(Some(element)) = document.query_selector(
                                                                    "[data-testid='chat-input']",
                                                                ) {
                                                                    if let Some(input) =
                                                                        element.dyn_ref::<web_sys::HtmlElement>()
                                                                    {
                                                                        let _ = input.focus();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                })
                                                data_testid="chat-conversation-start-chat".to_string()
                                            >
                                                "Start Chat"
                                            </Button>
                                            <Button
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                                on_click=Callback::new(move |_| show_attach_dialog.set(true))
                                                data_testid="chat-conversation-add-files".to_string()
                                            >
                                                "Add Files"
                                            </Button>
                                            <a
                                                href="/adapters"
                                                class="btn btn-ghost btn-sm"
                                                data-testid="chat-conversation-browse-adapters"
                                            >
                                                "Browse Adapters (Library)"
                                            </a>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-5">
                                    <For
                                        each=move || message_ids.try_get().unwrap_or_default()
                                        key=|id| id.clone()
                                        children={
                                            let active_trace = active_trace;
                                            move |msg_id| {
                                                view! {
                                                    <ChatConversationMessageItem
                                                        msg_id=msg_id
                                                        active_trace=active_trace
                                                    />
                                                }
                                            }
                                        }
                                    />

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
                                                ("text-status-warning", "bg-warning/5 border-warning/20")
                                            } else {
                                                ("text-destructive", "bg-destructive/5 border-destructive/20")
                                            };

                                            // Contextual help based on error type
                                            let help_hint = notice.as_ref().and_then(|n| {
                                                match n.message.as_str() {
                                                    "Server is busy" => Some("The server is processing many requests. Retrying usually helps."),
                                                    "No workers available" => Some("All inference engines are busy. Try again in a moment."),
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
                {move || {
                    let has_messages = !message_ids.try_get().unwrap_or_default().is_empty();
                    if has_messages && !is_at_bottom.try_get().unwrap_or(true) {
                        Some(view! {
                            <button
                                type="button"
                                class="btn btn-outline btn-sm absolute bottom-4 right-4 inline-flex items-center gap-1.5 rounded-full border border-border bg-background/95 px-3 py-1.5 text-xs font-medium text-foreground shadow-sm hover:bg-muted/80 transition-colors"
                                on:click=move |_| scroll_to_latest.run(())
                                data-testid="chat-jump-to-latest"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 5v14m0 0l6-6m-6 6l-6-6"/>
                                </svg>
                                "Jump to latest"
                            </button>
                        })
                    } else {
                        None
                    }
                }}
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

            // Session inline notice (query param validation, etc.)
            {move || {
                session_inline_notice.try_get().flatten().map(|msg| view! {
                    <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-inline-notice">
                        <p class="text-sm text-warning-foreground">{msg}</p>
                    </div>
                })
            }}

            // Session confirmation state display
            {move || {
                let state = session_confirmation_state
                    .try_get()
                    .unwrap_or(SessionConfirmationState::Confirmed);
                match state {
                    SessionConfirmationState::Confirmed => None,
                    SessionConfirmationState::PendingConfirm => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-pending">
                                    "Local draft session (not confirmed by server yet)."
                                </p>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-primary hover:underline"
                                        on:click=move |_| retry_session_confirmation.run(())
                                        data-testid="chat-session-confirm-retry"
                                    >
                                        "Retry confirmation"
                                    </button>
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                    SessionConfirmationState::NotFound => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-not-found">
                                    "Session not found on server; link may be stale."
                                </p>
                                <div class="flex items-center gap-3">
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                    SessionConfirmationState::TransientError => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-transient">
                                    "Could not confirm session due to a temporary error."
                                </p>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-primary hover:underline"
                                        on:click=move |_| retry_session_confirmation.run(())
                                        data-testid="chat-session-confirm-retry"
                                    >
                                        "Retry confirmation"
                                    </button>
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                }
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
                            "No workers available" => Some("All inference engines are busy. Try again in a moment."),
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
                                            class="btn btn-ghost btn-sm text-sm font-medium text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-colors"
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
                                                        class="btn btn-link btn-xs text-xs text-muted-foreground hover:text-foreground"
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
                <div class="mb-2 text-xs text-muted-foreground" data-testid="chat-active-config-line">
                    {move || {
                        let state = chat_state.try_get().unwrap_or_default();
                        let model = base_model_label.try_get().unwrap_or_else(|| "Auto".to_string());
                        let adapters = state
                            .pinned_adapters
                            .iter()
                            .take(2)
                            .cloned()
                            .collect::<Vec<_>>();
                        let adapter_segment = if adapters.is_empty() {
                            "no adapters pinned".to_string()
                        } else {
                            adapters.join(", ")
                        };
                        let rag_segment = state
                            .active_collection_id
                            .as_ref()
                            .map(|id| format!("RAG: {}", id))
                            .unwrap_or_else(|| "RAG: off".to_string());
                        let verify_segment = if state.verified_mode {
                            if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded
                            {
                                "Strict-Replayable"
                            } else {
                                "Bit-Identical"
                            }
                        } else {
                            "Best-Effort"
                        };
                        let details = format!(
                            "{} · {} · {} · {}",
                            model, adapter_segment, rag_segment, verify_segment
                        );
                        if is_compact_view.try_get().unwrap_or(false) {
                            let expanded = show_mobile_config_details.try_get().unwrap_or(false);
                            let button_text = if expanded {
                                details
                            } else {
                                model
                            };
                            view! {
                                <button
                                    type="button"
                                    class="btn btn-ghost btn-xs h-auto px-1 text-xs text-muted-foreground"
                                    on:click=move |_| show_mobile_config_details.update(|v| *v = !*v)
                                >
                                    {button_text}
                                </button>
                            }
                            .into_any()
                        } else {
                            view! { <span>{details}</span> }.into_any()
                        }
                    }}
                </div>
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
                description="Create training material from a file, pasted text, or this chat.".to_string()
            >
                <div class="space-y-4">
                    <div class="grid grid-cols-3 gap-2 text-xs">
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Upload {
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
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
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
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
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
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
                                <label for="chat-attach-upload-file" class="text-xs text-muted-foreground">
                                    "Select a file"
                                </label>
                                <input
                                    id="chat-attach-upload-file"
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
                                <label for="chat-attach-paste-text" class="text-xs text-muted-foreground">
                                    "Paste text"
                                </label>
                                <Textarea
                                    id="chat-attach-paste-text".to_string()
                                    value=pasted_text
                                    placeholder="Paste training examples or notes...".to_string()
                                    rows=5
                                    class="w-full".to_string()
                                    aria_label="Paste training text".to_string()
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
                                        <p class="text-xs text-muted-foreground">"Select messages"</p>
                                        <span class="text-xs text-muted-foreground">
                                            {move || format!("{} of {} selected", selected_count.try_get().unwrap_or(0), chat_state.try_get().unwrap_or_default().messages.len())}
                                        </span>
                                    </div>

                                    // Quick actions
                                    <div class="flex gap-2 flex-wrap">
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
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
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_5.run(5)
                                        >
                                            "Last 5"
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_10.run(10)
                                        >
                                            "Last 10"
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
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

fn trust_summary_label(
    citation_count: usize,
    document_link_count: usize,
    adapter_attachments: &[AdapterAttachment],
    adapters_used: &[String],
    degraded_count: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if citation_count > 0 {
        parts.push(format!(
            "{} source{}",
            citation_count,
            plural_suffix(citation_count)
        ));
    }

    if document_link_count > 0 {
        parts.push(format!(
            "{} document{}",
            document_link_count,
            plural_suffix(document_link_count)
        ));
    }

    if let Some(first_attachment) = adapter_attachments.first() {
        let label = first_attachment
            .adapter_label
            .clone()
            .unwrap_or_else(|| short_adapter_label(&first_attachment.adapter_id));
        let extra = adapter_attachments.len().saturating_sub(1);
        if extra > 0 {
            parts.push(format!("{label} +{extra} adapter{}", plural_suffix(extra)));
        } else {
            parts.push(label);
        }
    } else if let Some(first_adapter) = adapters_used.first() {
        let extra = adapters_used.len().saturating_sub(1);
        if extra > 0 {
            parts.push(format!(
                "{} +{} adapter{}",
                short_adapter_label(first_adapter),
                extra,
                plural_suffix(extra)
            ));
        } else {
            parts.push(short_adapter_label(first_adapter));
        }
    }

    if degraded_count > 0 {
        parts.push(format!(
            "{} degraded notice{}",
            degraded_count,
            plural_suffix(degraded_count)
        ));
    }

    if parts.is_empty() {
        "Open trust details".to_string()
    } else {
        parts.join(" · ")
    }
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn short_adapter_label(adapter_id: &str) -> String {
    adapter_id
        .strip_prefix("adp_")
        .or_else(|| adapter_id.strip_prefix("adp-"))
        .unwrap_or(adapter_id)
        .to_string()
}

fn attach_reason_label(reason: &AdapterAttachReason) -> &'static str {
    match reason {
        AdapterAttachReason::Requested => "requested",
        AdapterAttachReason::Pinned => "pinned",
        AdapterAttachReason::StackRouting => "stack routing",
        AdapterAttachReason::FallbackRouting => "fallback routing",
        AdapterAttachReason::Unknown => "automatic",
    }
}

fn attach_reason_detail(reason: &AdapterAttachReason) -> &'static str {
    match reason {
        AdapterAttachReason::Requested => "Added because you requested this adapter directly.",
        AdapterAttachReason::Pinned => "Added because this adapter is pinned in the current chat.",
        AdapterAttachReason::StackRouting => "Added by the active stack routing policy.",
        AdapterAttachReason::FallbackRouting => {
            "Added during fallback after part of the requested route degraded."
        }
        AdapterAttachReason::Unknown => "Added by automatic routing.",
    }
}

fn degraded_kind_label(kind: &DegradedNoticeKind) -> &'static str {
    match kind {
        DegradedNoticeKind::AttachFailure => "Attach failure",
        DegradedNoticeKind::WorkerSemanticFallback => "Semantic fallback",
        DegradedNoticeKind::RoutingOverride => "Routing override",
        DegradedNoticeKind::BlockedPins => "Blocked pins",
        DegradedNoticeKind::WorkerUnavailable => "Worker unavailable",
        DegradedNoticeKind::FfiAttachFailure => "Low-level attach failure",
    }
}

fn degraded_level_label(level: &DegradedNoticeLevel) -> &'static str {
    match level {
        DegradedNoticeLevel::Info => "info",
        DegradedNoticeLevel::Warning => "warning",
        DegradedNoticeLevel::Critical => "critical",
    }
}

fn degraded_level_class(level: &DegradedNoticeLevel) -> &'static str {
    match level {
        DegradedNoticeLevel::Info => "border-info/30 bg-info/5",
        DegradedNoticeLevel::Warning => "border-warning/30 bg-warning/10",
        DegradedNoticeLevel::Critical => "border-destructive/40 bg-destructive/10",
    }
}

fn prominent_degraded_title(notices: &[DegradedNotice]) -> &'static str {
    if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::FfiAttachFailure)
    {
        "Meaning changed: low-level adapter attach failed"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::WorkerSemanticFallback)
    {
        "Meaning changed: fallback worker path used"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::WorkerUnavailable)
    {
        "Response path failed: worker unavailable"
    } else if notices
        .iter()
        .any(|notice| notice.kind == DegradedNoticeKind::AttachFailure)
    {
        "Meaning changed: adapter attach failed"
    } else {
        "Meaning changed during execution"
    }
}

fn citation_page_span_label(citation: &crate::signals::chat::ChatCitation) -> String {
    if let Some(page) = citation.page_number {
        format!(
            "Page {} · chars {}-{}",
            page, citation.offset_start, citation.offset_end
        )
    } else {
        format!("Chars {}-{}", citation.offset_start, citation.offset_end)
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

// ---------------------------------------------------------------------------
// Chat target selector (moved from chat_dock.rs, stacks removed)
// ---------------------------------------------------------------------------

/// Target options fetched from API
#[derive(Debug, Clone, Default)]
struct TargetOptions {
    models: Vec<(String, String)>,   // (id, name)
    policies: Vec<(String, String)>, // (cpid, display_name)
    loading: bool,
    error: Option<String>,
}

/// Target selector dropdown with dynamic data from API
#[component]
fn ChatTargetSelector(#[prop(optional)] inline: bool) -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let show_dropdown = RwSignal::new(false);
    let options = RwSignal::new(TargetOptions::default());
    let has_loaded = RwSignal::new(false);

    let (system_status, _) = use_system_status();
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

    Effect::new(move |prev_open: Option<bool>| {
        let Some(is_open) = show_dropdown.try_get() else {
            return prev_open.unwrap_or(false);
        };
        if let Some(was_open) = prev_open {
            if was_open && !is_open {
                let _ = has_loaded.try_set(false);
            }
        }
        is_open
    });

    Effect::new(move || {
        if show_dropdown.try_get().unwrap_or(false) && !has_loaded.try_get().unwrap_or(true) {
            has_loaded.set(true);
            options.update(|o| {
                o.loading = true;
                o.error = None;
            });

            wasm_bindgen_futures::spawn_local(async move {
                let client = crate::api::ApiClient::with_base_url(crate::api::api_base_url());

                let models_fut = client.list_models();
                let policies_fut = client.list_policies();

                let (models_res, policies_res) = futures::join!(models_fut, policies_fut);

                let mut errors: Vec<String> = Vec::new();

                let _ = options.try_update(|o| {
                    o.loading = false;

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

    let container_class = if inline {
        "relative"
    } else {
        "relative border-b px-4 py-2"
    };
    let button_class = if inline {
        "flex items-center gap-2 rounded-md border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted transition-colors"
    } else {
        "flex w-full items-center justify-between rounded-md border bg-background px-3 py-2 text-sm hover:bg-muted transition-colors"
    };
    let dropdown_class = if inline {
        "absolute left-0 top-full z-50 mt-1 min-w-[200px] rounded-md border border-border bg-popover shadow-lg max-h-80 overflow-y-auto"
    } else {
        "absolute left-4 right-4 top-full z-50 mt-1 rounded-md border bg-popover shadow-lg max-h-80 overflow-y-auto"
    };

    view! {
        <div class=container_class>
            <button
                class=button_class
                on:click=toggle_dropdown
                data-testid=move || if inline { Some("chat-target-selector".to_string()) } else { None }
            >
                {move || {
                    let model = active_model_name.try_get().flatten();
                    let label = chat_state.get().target.display_name_with_model(model.as_deref());
                    if inline {
                        view! {
                            <>
                                <span class="text-muted-foreground text-xs">"Target:"</span>
                                <span class="font-medium truncate min-w-[140px] max-w-[220px]">{label}</span>
                            </>
                        }
                        .into_any()
                    } else {
                        view! { <span class="truncate">{label}</span> }.into_any()
                    }
                }}
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class=if inline {
                        "h-4 w-4 text-muted-foreground flex-shrink-0"
                    } else {
                        "h-4 w-4 text-muted-foreground"
                    }
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
                </svg>
            </button>

            {move || {
                if inline && show_dropdown.get() {
                    Some(view! {
                        <div
                            class="fixed inset-0 z-40"
                            on:click=move |_| show_dropdown.set(false)
                        />
                    })
                } else {
                    None
                }
            }}

            {move || {
                if show_dropdown.get() {
                    let select = select_target.clone();
                    let opts = options.get();

                    view! {
                        <div
                            class=dropdown_class
                            data-testid=move || if inline { Some("chat-target-dropdown".to_string()) } else { None }
                        >
                            <div class="p-1">
                                <ChatTargetOption
                                    target=ChatTarget::Default
                                    label="Auto".to_string()
                                    on_select=select.clone()
                                />

                                {opts.error.as_ref().map(|e| view! {
                                    <div class="px-2 py-2 text-xs text-destructive bg-destructive/10 rounded mx-1 my-1">
                                        {e.clone()}
                                    </div>
                                })}

                                {if opts.loading {
                                    Some(view! {
                                        <div class="px-2 py-3 text-center text-sm text-muted-foreground">
                                            <span class="animate-pulse">"Loading options..."</span>
                                        </div>
                                    })
                                } else {
                                    None
                                }}

                                {if !opts.models.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Models"</div>
                                        {opts.models.iter().map(|(id, name)| {
                                            let target = ChatTarget::Model(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <ChatTargetOption
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

                                {if !opts.policies.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Policy Packs"</div>
                                        {opts.policies.iter().map(|(id, name)| {
                                            let target = ChatTarget::PolicyPack(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <ChatTargetOption
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

#[component]
fn ChatTargetOption<F>(target: ChatTarget, label: String, on_select: F) -> impl IntoView
where
    F: Fn(ChatTarget) + Clone + 'static,
{
    let target_clone = target.clone();
    let select = on_select.clone();

    view! {
        <button
            class="flex w-full items-center rounded-sm px-2 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground transition-colors"
            on:click=move |_| {
                select(target_clone.clone());
            }
        >
            {label}
        </button>
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
