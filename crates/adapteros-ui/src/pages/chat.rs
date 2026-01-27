//! Chat page with SSE streaming support
//!
//! This module provides the chat interface. The full chat page uses
//! the global chat state from signals/chat.rs for unified state management
//! with the dock panel.

use crate::components::{
    AdapterBar, AdapterHeat, AdapterMagnet, Badge, BadgeVariant, Button, Card, EmptyState,
    EmptyStateVariant, Spinner, SuggestedAdapterView, SuggestedAdaptersBar, Textarea, TraceButton,
    TracePanel,
};
use crate::signals::{use_chat, ChatSessionMeta, ChatSessionsManager};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

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

    // Check if dock has messages
    let dock_has_messages = Memo::new(move |_| !chat_state.get().messages.is_empty());
    let dock_message_count = Memo::new(move |_| chat_state.get().messages.len());

    // Create new session
    let create_session = move |_| {
        let session_id = format!("session-{}", uuid::Uuid::new_v4());
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&format!("/chat/{}", session_id));
        }
    };

    // Save dock to session and navigate - wrap in Callback for reuse in reactive closures
    let save_dock_and_navigate = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let state = chat_state.get_untracked();
            let session_id = format!("session-{}", uuid::Uuid::new_v4());
            let session = ChatSessionsManager::session_from_state(&session_id, &state);
            ChatSessionsManager::save_session(&session);
            // Clear dock messages after saving
            action.clear_messages();
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href(&format!("/chat/{}", session_id));
            }
        })
    };

    // Delete session handler
    let delete_session = move |id: String| {
        ChatSessionsManager::delete_session(&id);
        sessions.set(ChatSessionsManager::load_sessions());
    };

    view! {
        <div class="p-6 space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold tracking-tight">"Chat"</h1>
                <Button on_click=Callback::new(create_session)>
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
                    if session_list.is_empty() {
                        view! {
                            <div class="p-4">
                                <EmptyState
                                    title="No chat sessions yet"
                                    description="Start a new conversation to begin"
                                    variant=EmptyStateVariant::Empty
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
                                                delete_session(delete_id.clone());
                                            })
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </Card>

            // Quick start suggestions (when no sessions)
            {move || {
                if sessions.get().is_empty() && !dock_has_messages.get() {
                    Some(view! { <QuickStartSuggestions/> })
                } else {
                    None
                }
            }}
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

/// Quick start suggestions
#[component]
fn QuickStartSuggestions() -> impl IntoView {
    let suggestions = [
        (
            "Explain adapters",
            "How do LoRA adapters work and when should I use them?",
        ),
        (
            "Write a function",
            "Write a Python function to validate email addresses with tests",
        ),
        (
            "Review code",
            "Review this code for bugs: fn add(a: i32, b: i32) -> i32 { a - b }",
        ),
        (
            "Training guide",
            "Walk me through training a custom adapter from my documentation",
        ),
    ];

    view! {
        <Card title="Quick Start".to_string() description="Try one of these prompts".to_string()>
            <div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-4 p-4">
                {suggestions.iter().map(|(title, prompt)| {
                    let title = title.to_string();
                    let prompt = prompt.to_string();
                    let prompt_display = prompt.clone();
                    view! {
                        <button
                            class="text-left p-3 rounded-lg border hover:bg-muted/50 transition-colors"
                            on:click=move |_| {
                                let session_id = format!("session-{}", uuid::Uuid::new_v4());
                                // Validate and truncate prompt if too long for URL embedding
                                let safe_prompt = if prompt.len() > MAX_URL_PROMPT_LENGTH {
                                    // Truncate at char boundary with indicator
                                    let mut truncated = prompt.chars().take(MAX_URL_PROMPT_LENGTH - 3).collect::<String>();
                                    truncated.push_str("...");
                                    web_sys::console::warn_1(
                                        &format!("Prompt truncated from {} to {} chars for URL safety", prompt.len(), truncated.len()).into()
                                    );
                                    truncated
                                } else {
                                    prompt.clone()
                                };
                                // Navigate with prompt as query param (handled by session page)
                                if let Some(window) = web_sys::window() {
                                    let _ = window.location().set_href(
                                        &format!("/chat/{}?prompt={}", session_id, js_sys::encode_uri_component(&safe_prompt))
                                    );
                                }
                            }
                        >
                            <p class="font-medium text-sm">{title}</p>
                            <p class="text-xs text-muted-foreground mt-1 line-clamp-1">{prompt_display}</p>
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </Card>
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

    // Local state for input and trace panel
    let message = RwSignal::new(String::new());
    let active_trace = RwSignal::new(Option::<String>::None);
    let session_loaded = RwSignal::new(false);
    let current_session_id = RwSignal::new(String::new());
    let session_error = RwSignal::new(Option::<String>::None);

    // Load session from localStorage on mount
    {
        let action = chat_action.clone();
        Effect::new(move |_| {
            if session_loaded.get_untracked() {
                return;
            }

            let id = session_id();

            // Handle empty/invalid session ID - redirect to landing page
            if id.is_empty() {
                web_sys::console::warn_1(
                    &"[ChatSession] Empty session ID, redirecting to /chat".into(),
                );
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/chat");
                }
                return;
            }

            // Clear any existing messages from a different session before loading
            let prev_session = current_session_id.get_untracked();
            if !prev_session.is_empty() && prev_session != id {
                action.clear_messages();
            }

            current_session_id.set(id.clone());

            // Try to load session from localStorage
            if let Some(stored) = ChatSessionsManager::load_session(&id) {
                action.restore_session(stored);
                session_error.set(None);
            } else {
                // Session not found - this is a new session, not an error
                // Only show error if we expected an existing session (no prompt param)
                if let Some(window) = web_sys::window() {
                    if let Ok(search) = window.location().search() {
                        // If no prompt param and session doesn't exist, it's a stale link
                        if !search.contains("prompt=") {
                            // Check if ID looks like a real session ID (not just created)
                            let now = js_sys::Date::now() as u64;
                            if let Some(timestamp_str) = id.strip_prefix("session-") {
                                if let Ok(timestamp) = timestamp_str.parse::<u64>() {
                                    // If more than 5 seconds old, likely a stale session
                                    if now.saturating_sub(timestamp) > 5000 {
                                        session_error.set(Some(
                                            "Session not found. It may have been deleted."
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Check for ?prompt= query parameter
            if let Some(window) = web_sys::window() {
                if let Ok(search) = window.location().search() {
                    if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
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
                                return;
                            }
                            if !decoded.is_empty() {
                                action.send_message_streaming(decoded);
                            }
                        }
                    }
                }
            }

            session_loaded.set(true);
        });
    }

    // Auto-save session when messages change
    // Uses get_untracked() for state to avoid re-entrancy during streaming
    {
        Effect::new(move |prev_state: Option<(usize, bool)>| {
            let state = chat_state.get_untracked();
            let msg_count = state.messages.len();
            let is_streaming = state.streaming;
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
        });
    }

    // Derived signals from global state
    let is_loading = Memo::new(move |_| chat_state.get().loading);
    let is_streaming = Memo::new(move |_| chat_state.get().streaming);
    let is_busy = Memo::new(move |_| {
        let state = chat_state.get();
        state.loading || state.streaming
    });
    let can_send = Memo::new(move |_| !message.get().trim().is_empty() && !is_busy.get());
    let error = Memo::new(move |_| chat_state.get().error.clone());

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

    // Cancel handler
    let do_cancel = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.cancel_stream();
        })
    };

    view! {
        <div class="p-6 flex h-full min-h-0 flex-col gap-4">
            // Header
            <div class="flex flex-wrap items-start justify-between gap-3 border-b pb-4">
                <div class="space-y-1">
                    <h1 class="text-xl font-semibold">"Chat Session"</h1>
                    <div class="flex items-center gap-2 text-xs text-muted-foreground">
                        <span class="uppercase tracking-wide">"Session"</span>
                        <span class="font-mono">{session_label}</span>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    {move || {
                        let err = error.get();
                        if err.is_some() {
                            view! {
                                <span class="rounded-full bg-status-error/10 px-2 py-1 text-xs font-medium text-status-error">
                                    "Error"
                                </span>
                            }.into_any()
                        } else if is_streaming.get() {
                            view! {
                                <span
                                    class="rounded-full bg-status-success/10 px-2 py-1 text-xs font-medium text-status-success"
                                    aria-label="Streaming status"
                                >
                                    "Streaming"
                                </span>
                            }.into_any()
                        } else {
                            view! {
                                <span
                                    class="rounded-full bg-muted px-2 py-1 text-xs text-muted-foreground"
                                    aria-label="Chat status: Ready"
                                >
                                    "Ready"
                                </span>
                            }.into_any()
                        }
                    }}
                </div>
            </div>

            // Adapter Bar - shows active adapters as colored magnets
            <AdapterBar adapters=adapter_magnets/>

            // Suggested Adapters Bar - shows router preview suggestions with click-to-pin
            <SuggestedAdaptersBar
                suggestions=suggested_adapters
                on_toggle_pin=on_toggle_pin
            />

            // Messages
            <div
                class="flex-1 overflow-y-auto rounded-lg border bg-card"
                role="log"
                aria-live="polite"
                aria-label="Chat messages"
            >
                <div class="p-4">
                    {move || {
                        let msgs = chat_state.get().messages;
                        if msgs.is_empty() {
                            view! {
                                <div class="flex h-full items-center justify-center">
                                    <p class="text-muted-foreground">"No messages yet. Start the conversation!"</p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-5">
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
                                                        "flex flex-col gap-1 chat-bubble {}",
                                                        if is_user { "items-end" } else { "items-start" }
                                                    )>
                                                        <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                            {role_label}
                                                        </span>
                                                        <div class=format!(
                                                            "rounded-lg px-4 py-2 shadow-sm {}",
                                                            if is_user {
                                                                "bg-primary text-primary-foreground"
                                                            } else {
                                                                "bg-muted"
                                                            }
                                                        )>
                                                            <p class="text-sm whitespace-pre-wrap break-words">
                                                                {msg.content.clone()}
                                                                {if is_streaming && !msg.content.is_empty() {
                                                                    view! { <span class="animate-pulse">"_"</span> }.into_any()
                                                                } else if is_streaming {
                                                                    view! { <Spinner/> }.into_any()
                                                                } else {
                                                                    view! { <span></span> }.into_any()
                                                                }}
                                                            </p>
                                                        </div>
                                                        // Show trace button for assistant messages with trace info
                                                        {if let (false, false, Some(tid)) = (is_user, is_streaming, trace_id.clone()) {
                                                            let latency = latency_ms.unwrap_or(0);
                                                            Some(view! {
                                                                <div class="flex items-center gap-2 pl-1">
                                                                    <TraceButton
                                                                        trace_id=tid.clone()
                                                                        latency_ms=latency
                                                                        on_click=Callback::new(move |id: String| {
                                                                            active_trace.set(Some(id));
                                                                        })
                                                                    />
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
            {move || {
                let action = chat_action.clone();
                error.get().map(|e| view! {
                    <div class="rounded-md bg-destructive/10 border border-destructive p-3 mb-4">
                        <div class="flex items-center justify-between gap-2">
                            <p class="text-sm text-destructive">{e}</p>
                            <button
                                class="text-sm font-medium text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-colors"
                                on:click=move |_| action.clear_error()
                                aria-label="Dismiss error"
                            >
                                "Dismiss"
                            </button>
                        </div>
                    </div>
                })
            }}

            // Input
            <div class="border-t pt-4">
                <form
                    class="flex items-end gap-4"
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
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}
