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

use crate::api::ApiClient;
use crate::components::inference_guidance::guidance_for;
use crate::components::status_center::use_status_center;
use crate::components::{
    AdapterBar, AdapterHeat, AdapterMagnet, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant,
    Card, ConfirmationDialog, ConfirmationSeverity, EmptyState, EmptyStateVariant, Spinner,
    SuggestedAdapterView, SuggestedAdaptersBar, Textarea, TraceButton, TracePanel,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{use_chat, ChatSessionMeta, ChatSessionsManager, StreamNoticeTone};
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::sync::Arc;

/// Maximum prompt length for URL-embedded prompts (bytes).
/// This prevents DoS attacks from extremely long URLs that could:
/// 1. Exceed browser URL limits (typically 2KB-8KB)
/// 2. Exhaust memory when decoded
/// 3. Overwhelm the inference endpoint
const MAX_URL_PROMPT_LENGTH: usize = 2000;

/// Chat sessions landing page with recent sessions
#[component]
pub fn Chat() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let sessions = RwSignal::new(ChatSessionsManager::load_sessions());
    let navigate = use_navigate();

    // Delete confirmation state
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let show_delete_confirm = RwSignal::new(false);

    // Check if dock has messages
    let dock_has_messages = Memo::new(move |_| !chat_state.get().messages.is_empty());
    let dock_message_count = Memo::new(move |_| chat_state.get().messages.len());

    // Create new session - uses client-side navigation for faster transition
    let create_session = {
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            let session_id = generate_readable_id("session", "chat");
            let path = format!("/chat/{}", session_id);
            navigate(&path, Default::default());
        })
    };

    // Save dock to session and navigate - wrap in Callback for reuse in reactive closures
    let save_dock_and_navigate = {
        let action = chat_action.clone();
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            let state = chat_state.get_untracked();
            let session_id = generate_readable_id("session", "chat");
            let session = ChatSessionsManager::session_from_state(&session_id, &state);
            ChatSessionsManager::save_session(&session);
            // Clear dock messages after saving
            action.clear_messages();
            let path = format!("/chat/{}", session_id);
            navigate(&path, Default::default());
        })
    };

    // Request delete confirmation
    let request_delete = move |id: String| {
        pending_delete_id.set(Some(id));
        show_delete_confirm.set(true);
    };

    // Confirm delete handler
    let confirm_delete = move |_| {
        if let Some(id) = pending_delete_id.get() {
            ChatSessionsManager::delete_session(&id);
            sessions.set(ChatSessionsManager::load_sessions());
        }
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    // Cancel delete
    let cancel_delete = move |_| {
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    view! {
        <div class="p-6 space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold tracking-tight">"Chat"</h1>
                <Button on_click=create_session.clone()>
                    "New Session"
                </Button>
            </div>
            <p class="text-sm text-muted-foreground">
                "Use the system to reason, generate, and run inference against your active context."
            </p>

            // Continue from dock (if dock has messages)
            {move || {
                if dock_has_messages.get() {
                    Some(view! {
                        <Card class="border-primary/30 bg-primary/5".to_string()>
                            <div class="flex items-center justify-between p-4">
                                <div>
                                    <h3 class="font-medium">"Continue current conversation"</h3>
                                    <p class="text-sm text-muted-foreground">
                                        {move || format!("{} messages in dock", dock_message_count.get())}
                                    </p>
                                </div>
                                <Button on_click=save_dock_and_navigate.clone()>
                                    "Save & Open"
                                </Button>
                            </div>
                        </Card>
                    })
                } else {
                    None
                }
            }}

            // Recent sessions
            <Card title="Recent Sessions".to_string()>
                {move || {
                    let session_list = sessions.get();
                    let on_new = create_session.clone();
                    if session_list.is_empty() {
                        view! {
                            <div class="p-6">
                                <EmptyState
                                    title="Start your first conversation"
                                    description="Ask questions, explore ideas, and reason over your data. Each session is automatically saved so you can pick up where you left off."
                                    variant=EmptyStateVariant::Empty
                                    // Sparkle/lightning icon for inspiration
                                    icon="M13 10V3L4 14h7v7l9-11h-7z"
                                    action_label="New Session"
                                    on_action=on_new
                                    secondary_label="Learn about adapters"
                                    secondary_href="/adapters"
                                />
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="divide-y">
                                {session_list.into_iter().map(|session| {
                                    let id = session.id.clone();
                                    let delete_id = id.clone();
                                    view! {
                                        <SessionCard
                                            session=session
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
            </Card>

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

/// Session card component
#[component]
fn SessionCard(session: ChatSessionMeta, on_delete: Callback<()>) -> impl IntoView {
    let id = session.id.clone();
    let href = format!("/chat/{}", id);

    view! {
        <a
            href=href
            class="block group p-4 hover:bg-muted/50 transition-colors"
        >
            <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                    // Title row
                    <div class="flex items-center gap-2">
                        <h3 class="font-medium truncate">{session.title}</h3>
                        <Badge variant=BadgeVariant::Outline>
                            {session.target}
                        </Badge>
                    </div>

                    // Preview
                    {if !session.preview.is_empty() {
                        Some(view! {
                            <p class="text-sm text-muted-foreground mt-1 line-clamp-2">
                                {session.preview}
                            </p>
                        })
                    } else {
                        None
                    }}

                    // Metadata row
                    <div class="flex items-center gap-3 mt-2 text-xs text-muted-foreground">
                        <span>{format_relative_time(&session.updated_at)}</span>
                        <span>"·"</span>
                        <span>{format!("{} messages", session.message_count)}</span>
                    </div>
                </div>

                // Delete button
                <button
                    class="p-2 rounded opacity-0 group-hover:opacity-100 hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        on_delete.run(());
                    }
                    title="Delete session"
                    aria-label="Delete session"
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-4 w-4"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                        />
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

    let now = Utc::now();
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

fn generate_readable_id(prefix: &str, slug_source: &str) -> String {
    let slug = slugify(slug_source);
    let suffix = random_suffix(6);
    format!("{}.{}.{}", prefix, slug, suffix)
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

fn random_suffix(len: usize) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let idx = (js_sys::Math::random() * 32.0).floor() as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Chat session page using global state with SSE streaming
#[component]
pub fn ChatSession() -> impl IntoView {
    let params = use_params_map();
    let session_id = move || params.get().get("session_id").unwrap_or_default();
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
                    action.clear_messages();
                }
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
                // Session not found - check if this is a stale link or a new session
                // A "new session" is one the user just created (no history expected)
                // A "stale link" is one that references a deleted/expired session
                if let Some(window) = web_sys::window() {
                    if let Ok(search) = window.location().search() {
                        // If no prompt param, user navigated to an existing session URL
                        // that no longer exists - show a helpful message
                        if !search.contains("prompt=") {
                            // Check if ID looks like a stored session format (session.slug.xxxxx)
                            // If it matches our format but doesn't exist, it was deleted
                            if id.starts_with("session.") && id.matches('.').count() >= 2 {
                                session_error.set(Some(
                                    "Session not found. It may have been deleted or expired."
                                        .to_string(),
                                ));
                            }
                            // Otherwise it's a fresh session with custom ID, no error needed
                        }
                    }
                }
            }

            // Check for ?prompt= and ?adapter= query parameters (only on first load)
            if prev_session_id.is_none() {
                if let Some(window) = web_sys::window() {
                    if let Ok(search) = window.location().search() {
                        if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                            // Handle ?adapter= parameter - auto-pin the adapter
                            if let Some(adapter_id) = params.get("adapter") {
                                let decoded_adapter = js_sys::decode_uri_component(&adapter_id)
                                    .map(|s| s.as_string().unwrap_or_default())
                                    .unwrap_or(adapter_id);
                                if !decoded_adapter.is_empty() {
                                    // Pin the adapter for this chat session
                                    action.toggle_pin_adapter(&decoded_adapter);
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
                                }
                            }
                        }
                    }
                }
            }

            session_loaded.set(true);
            id
        });
    }

    // Auto-save session when messages change
    // Uses chat_state.get() to create reactive dependency, then compares with previous state
    {
        Effect::new(move |prev_state: Option<(usize, bool)>| {
            // Get state reactively to trigger effect when it changes
            let state = chat_state.get();
            let msg_count = state.messages.len();
            let is_streaming = state.streaming;
            // Get session ID untracked since we only care about state changes, not ID changes
            let id = current_session_id.get_untracked();

            // Only save if:
            // 1. We have a session ID and messages
            // 2. Not currently streaming (wait for stream to complete)
            // 3. Message count changed OR streaming just stopped
            let should_save = !id.is_empty() && msg_count > 0 && !is_streaming;

            if should_save {
                if let Some((prev_count, was_streaming)) = prev_state {
                    // Save when message count changes, or when streaming just completed
                    if msg_count != prev_count || (was_streaming && !is_streaming) {
                        let session = ChatSessionsManager::session_from_state(&id, &state);
                        ChatSessionsManager::save_session(&session);
                        web_sys::console::log_1(
                            &format!("[Chat] Auto-saved session {} ({} messages)", id, msg_count)
                                .into(),
                        );
                    }
                }
            }

            (msg_count, is_streaming)
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
        let state = chat_state.get();
        (
            state.loading,
            state.streaming,
            state.error.clone(),
            state.stream_recovery.is_some(),
        )
    });

    let is_loading = Signal::derive(move || chat_snapshot.get().0);
    let is_streaming = Signal::derive(move || chat_snapshot.get().1);
    let is_busy = Signal::derive(move || {
        let (loading, streaming, _, _) = chat_snapshot.get();
        loading || streaming
    });
    let can_send = Memo::new(move |_| !message.get().trim().is_empty() && !is_busy.get());
    let error = Signal::derive(move || chat_snapshot.get().2);
    let can_retry = Signal::derive(move || {
        let (loading, streaming, _, has_recovery) = chat_snapshot.get();
        !loading && !streaming && has_recovery
    });
    let retry_disabled = Signal::derive(move || !can_retry.get());

    // Convert active_adapters to AdapterMagnets for the AdapterBar
    let adapter_magnets = Memo::new(move |_| {
        chat_state
            .get()
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
                }
            })
            .collect::<Vec<_>>()
    });

    // Convert suggested_adapters for the SuggestedAdaptersBar
    // Name is populated from topology; other fields remain optional
    let suggested_adapters = Memo::new(move |_| {
        chat_state
            .get()
            .suggested_adapters
            .iter()
            .map(|s| SuggestedAdapterView {
                adapter_id: s.adapter_id.clone(),
                confidence: s.confidence,
                is_pinned: s.is_pinned,
                // Use adapter name as description if available
                disabled_reason: None,
                description: s.name.clone(),
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
            let text = message.get();
            // Update version to invalidate pending previews
            preview_version.update(|v| *v += 1);
            let current_version = preview_version.get_untracked();

            // Debounce: 300ms delay before calling preview
            let action = action.clone();
            set_timeout_simple(
                move || {
                    // Only proceed if this is still the latest version
                    if preview_version.get_untracked() != current_version {
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

    // Send message handler
    let do_send = {
        let action = chat_action.clone();
        move || {
            let msg = message.get();
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
            if ev.key() == "Enter" && !ev.shift_key() && can_send.get() {
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

    view! {
        <div class="p-6 flex h-full min-h-0 flex-col gap-4">
            // Header
            <div class="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-4">
                <div class="space-y-1">
                    <h1 class="text-xl font-semibold tracking-tight">"Chat Session"</h1>
                    <div class="flex items-center gap-2 text-xs text-muted-foreground">
                        <span class="uppercase tracking-wider text-2xs font-medium">"Session"</span>
                        <span class="font-mono bg-muted/30 px-1.5 py-0.5 rounded text-2xs">{session_label}</span>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    {move || {
                        let err = error.get();
                        if err.is_some() {
                            view! {
                                <Badge variant=BadgeVariant::Destructive>"Error"</Badge>
                            }.into_any()
                        } else if is_loading.get() {
                            // Waiting for first token
                            view! {
                                <Badge variant=BadgeVariant::Warning>"Connecting"</Badge>
                            }.into_any()
                        } else if is_streaming.get() {
                            // Actively receiving tokens
                            view! {
                                <Badge variant=BadgeVariant::Success>"Streaming"</Badge>
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
                chat_state.get().stream_notice.clone().and_then(|notice| {
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

            // Adapter Bar - shows active adapters as colored magnets
            <AdapterBar adapters=adapter_magnets/>

            // Suggested Adapters Bar - shows router preview suggestions with click-to-pin
            <SuggestedAdaptersBar
                suggestions=suggested_adapters
                on_toggle_pin=on_toggle_pin
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
                        let msgs = chat_state.get().messages;
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
                                            <h3 class="text-lg font-medium text-foreground">"What would you like to explore?"</h3>
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
                                                                            <span class="text-xs">"Thinking..."</span>
                                                                        </span>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <span></span> }.into_any()
                                                                }}
                                                            </p>
                                                        </div>
                                                        // Show trace button and run links for assistant messages with trace info
                                                        {if let (false, false, Some(tid)) = (is_user, is_streaming, trace_id.clone()) {
                                                            let latency = latency_ms.unwrap_or(0);
                                                            let run_overview_url = format!("/runs/{}", tid);
                                                            let run_receipt_url = format!("/runs/{}?tab=receipt", tid);
                                                            Some(view! {
                                                                <div class="flex items-center gap-3 mt-1 px-1 flex-wrap" data-testid="chat-trace-links">
                                                                    <TraceButton
                                                                        trace_id=tid.clone()
                                                                        latency_ms=latency
                                                                        on_click=Callback::new(move |id: String| {
                                                                            active_trace.set(Some(id));
                                                                        })
                                                                        data_testid="chat-trace-link".to_string()
                                                                    />
                                                                    // Run detail links
                                                                    <div class="flex items-center gap-1">
                                                                        <a
                                                                            href=run_overview_url
                                                                            class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                                            title="View Run Detail"
                                                                            data-testid="chat-run-link"
                                                                        >
                                                                            "Run"
                                                                        </a>
                                                                        <span class="text-muted-foreground/50">"·"</span>
                                                                        <a
                                                                            href=run_receipt_url
                                                                            class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                                            title="Verify Receipt"
                                                                            data-testid="chat-receipt-link"
                                                                        >
                                                                            "Receipt"
                                                                        </a>
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
                                        let state = chat_state.get();
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
                active_trace.get().map(|tid| {
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
                session_error.get().map(|e| view! {
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
                let state = chat_state.get();
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
                                                    disabled=retry_disabled.clone()
                                                    on_click=do_retry.clone()
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
                match system_status.get() {
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
                                            {if let Some(ctx) = status_center {
                                                Some(view! {
                                                    <button
                                                        class="text-xs text-muted-foreground hover:text-foreground"
                                                        on:click=move |_| ctx.open()
                                                    >
                                                        "Why?"
                                                    </button>
                                                })
                                            } else {
                                                None
                                            }}
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
                        if can_send.get() {
                            do_send();
                        }
                    }
                >
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
                        if is_streaming.get() {
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
                            let disabled = !can_send.get();
                            view! {
                                <Button
                                    loading=is_loading.get()
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
