//! Chat Dock component
//!
//! A persistent chat panel that stays visible across page navigation.
//! Provides a command console for interacting with adapterOS.

use crate::api::ApiClient;
use crate::components::{Button, ButtonSize, ButtonType, ButtonVariant, Spinner, Textarea};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{
    use_chat, use_settings, ChatTarget, ContextToggle, DockState, MessageStatus, PendingPhase,
};
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use std::sync::Arc;

/// Human-readable blocker reasons for pending messages
fn humanize_blocker(blocker: &adapteros_api_types::InferenceBlocker) -> &'static str {
    use adapteros_api_types::InferenceBlocker;
    match blocker {
        InferenceBlocker::WorkerMissing => "waiting for worker",
        InferenceBlocker::NoModelLoaded => "loading model",
        InferenceBlocker::DatabaseUnavailable => "connecting to database",
        InferenceBlocker::ActiveModelMismatch => "switching models",
        InferenceBlocker::TelemetryDegraded => "system warming up",
        InferenceBlocker::SystemBooting => "system starting",
        InferenceBlocker::BootFailed => "system error",
    }
}

/// Chat Dock component - persistent chat panel on the right side (wrapper)
#[component]
pub fn ChatDock() -> impl IntoView {
    let (chat_state, _) = use_chat();

    view! {
        {move || {
            let state = chat_state.get();
            match state.dock_state {
                DockState::Docked => view! { <ChatDockPanel/> }.into_any(),
                DockState::Narrow => view! { <NarrowChatDock/> }.into_any(),
                DockState::Hidden => view! {}.into_any(),
            }
        }}
    }
}

/// Full docked panel view
#[component]
pub fn ChatDockPanel() -> impl IntoView {
    view! {
        <aside class="w-80 xl:w-96 flex-col border-l border-border bg-background h-full hidden lg:flex transition-all duration-200">
            // Header with collapse button
            <div class="h-10 flex items-center justify-between border-b border-border px-3 shrink-0">
                <span class="text-sm font-medium">"Chat"</span>
                <div class="flex items-center gap-1">
                    <PopOutButton/>
                    <CollapseButton/>
                </div>
            </div>

            // Target selector
            <ChatTargetSelector/>

            // Messages area
            <MessageList/>

            // Context toggles
            <ContextTogglesBar/>

            // Input area
            <ChatInput/>
        </aside>
    }
}

/// Narrow dock view (icon only with unread badge)
#[component]
pub fn NarrowChatDock() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();

    let expand = {
        let action = chat_action.clone();
        move |_| {
            action.set_dock_state(DockState::Docked);
        }
    };

    view! {
        <aside class="w-12 flex-col items-center border-l border-border bg-background py-3 hidden lg:flex">
            <button
                class="relative p-2 rounded-lg hover:bg-muted/50 transition-colors"
                on:click=expand
                title="Expand chat"
            >
                // Chat icon
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-5 w-5 text-muted-foreground"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
                    />
                </svg>

                // Unread badge
                {move || {
                    let unread = chat_state.get().unread_count();
                    if unread > 0 {
                        view! {
                            <span class="absolute -top-1 -right-1 flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-3xs font-medium text-destructive-foreground">
                                {if unread > 9 { "9+".to_string() } else { unread.to_string() }}
                            </span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
            </button>
        </aside>
    }
}

/// Collapse button to minimize the dock
#[component]
fn CollapseButton() -> impl IntoView {
    let (_, chat_action) = use_chat();

    let collapse = {
        let action = chat_action.clone();
        move |_| {
            action.set_dock_state(DockState::Narrow);
        }
    };

    view! {
        <button
            class="p-1 rounded hover:bg-muted transition-colors"
            on:click=collapse
            title="Collapse"
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 text-muted-foreground"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d="M13 5l7 7-7 7M5 5l7 7-7 7"/>
            </svg>
        </button>
    }
}

/// Pop-out button to navigate to full chat page
#[component]
fn PopOutButton() -> impl IntoView {
    let navigate_fn = leptos_router::hooks::use_navigate();
    let navigate = move |_| {
        navigate_fn("/chat", Default::default());
    };

    view! {
        <button
            class="p-1 rounded hover:bg-muted transition-colors"
            on:click=navigate
            title="Open full chat"
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 text-muted-foreground"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                />
            </svg>
        </button>
    }
}

/// Target options fetched from API
#[derive(Debug, Clone, Default)]
struct TargetOptions {
    models: Vec<(String, String)>,   // (id, name)
    stacks: Vec<(String, String)>,   // (id, name)
    policies: Vec<(String, String)>, // (cpid, display_name)
    loading: bool,
    error: Option<String>, // API error message for display
}

/// Target selector dropdown with dynamic data from API
#[component]
pub fn ChatTargetSelector(#[prop(optional)] inline: bool) -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let show_dropdown = RwSignal::new(false);
    let options = RwSignal::new(TargetOptions::default());
    let has_loaded = RwSignal::new(false);

    // Fetch system status to resolve active model name for "Auto" display.
    let (system_status, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });
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

    // Reset has_loaded when dropdown closes to allow refresh on next open
    Effect::new(move |prev_open: Option<bool>| {
        let Some(is_open) = show_dropdown.try_get() else {
            return prev_open.unwrap_or(false);
        };
        if let Some(was_open) = prev_open {
            if was_open && !is_open {
                // Dropdown just closed - reset to allow refresh on next open
                let _ = has_loaded.try_set(false);
            }
        }
        is_open
    });

    // Fetch options when dropdown is first opened
    Effect::new(move || {
        if show_dropdown.try_get().unwrap_or(false) && !has_loaded.try_get().unwrap_or(true) {
            has_loaded.set(true);
            options.update(|o| {
                o.loading = true;
                o.error = None;
            });

            wasm_bindgen_futures::spawn_local(async move {
                let client = crate::api::ApiClient::with_base_url(crate::api::api_base_url());

                // Fetch all in parallel
                let models_fut = client.list_models();
                let stacks_fut = client.list_stacks();
                let policies_fut = client.list_policies();

                let (models_res, stacks_res, policies_res) =
                    futures::join!(models_fut, stacks_fut, policies_fut);

                // Track errors for display
                let mut errors: Vec<String> = Vec::new();

                let _ = options.try_update(|o| {
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
                                    // Parse display name from CPID (e.g., "medical-qa-v1" -> "Medical QA v1")
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

                    // Set combined error if any
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
                                <span class="font-medium truncate max-w-[150px]">{label}</span>
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

            // Backdrop to close dropdown on outside click in inline mode
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

            // Dropdown menu
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
                                <DynamicTargetOption
                                    target=ChatTarget::Default
                                    label="Auto".to_string()
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
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Models"</div>
                                        {opts.models.iter().map(|(id, name)| {
                                            let target = ChatTarget::Model(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <DynamicTargetOption
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
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Stacks"</div>
                                        {opts.stacks.iter().map(|(id, name)| {
                                            let target = ChatTarget::Stack(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <DynamicTargetOption
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

                                // Policies section
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
                                                <DynamicTargetOption
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

/// Dynamic target option with String label
#[component]
fn DynamicTargetOption<F>(target: ChatTarget, label: String, on_select: F) -> impl IntoView
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

/// Individual message item with memoization for performance
///
/// This component uses `Memo` for derived values to avoid O(n) clones per render.
/// Instead of cloning all message content on every state update, we only recompute
/// when the specific message changes.
#[component]
fn MessageItem(msg_id: String) -> impl IntoView {
    let (chat_state, _) = use_chat();
    let settings = use_settings();
    let msg_id_clone = msg_id.clone();

    // Memoize message lookup - only recomputes when messages change
    let message = Memo::new(move |_| {
        chat_state
            .get()
            .messages
            .iter()
            .find(|m| m.id == msg_id_clone)
            .cloned()
    });

    // Memoize derived values - only recompute when message changes
    let formatted_time = Memo::new(move |_| {
        message
            .get()
            .map(|m| m.timestamp.format("%H:%M").to_string())
            .unwrap_or_default()
    });

    let show_timestamps = Memo::new(move |_| {
        settings
            .try_get()
            .map(|s| s.show_timestamps)
            .unwrap_or(true)
    });

    let is_user = Memo::new(move |_| message.get().is_some_and(|m| m.role == "user"));

    let is_streaming = Memo::new(move |_| message.get().is_some_and(|m| m.is_streaming));

    let content = Memo::new(move |_| message.get().map(|m| m.content).unwrap_or_default());

    let backend_used = Memo::new(move |_| message.get().and_then(|m| m.backend_used));

    // Queue UX: message status and pending info
    let is_pending = Memo::new(move |_| {
        message
            .get()
            .is_some_and(|m| matches!(m.status, MessageStatus::Queued | MessageStatus::Sending))
    });

    let is_failed = Memo::new(move |_| {
        message
            .get()
            .is_some_and(|m| matches!(m.status, MessageStatus::Failed))
    });

    let pending_text = Memo::new(move |_| {
        message
            .get()
            .map(|m| match m.pending_phase {
                PendingPhase::Calm => "waiting...".to_string(),
                PendingPhase::Informative => m
                    .pending_reason
                    .clone()
                    .map(|r| format!("{}...", r))
                    .unwrap_or_else(|| "waiting...".to_string()),
                PendingPhase::Estimated => m
                    .pending_reason
                    .clone()
                    .map(|r| format!("~30s · {}", r))
                    .unwrap_or_else(|| "~30s".to_string()),
            })
            .unwrap_or_else(|| "waiting...".to_string())
    });

    let failed_reason = Memo::new(move |_| {
        message
            .get()
            .and_then(|m| m.pending_reason.clone())
            .unwrap_or_else(|| "Failed".to_string())
    });

    view! {
        {move || {
            message.get().map(|_| {
                let user = is_user.get();

                view! {
                    <div class=format!(
                        "flex {}",
                        if user { "justify-end" } else { "justify-start" }
                    )>
                        <div class=move || format!(
                            "chat-bubble-compact rounded-lg px-3 py-2 {} {}",
                            if user { "bg-primary text-primary-foreground" } else { "bg-muted" },
                            if is_pending.get() { "chat-message-pending" } else { "" }
                        )>
                            {move || {
                                if is_user.get() {
                                    view! { <p class="text-sm whitespace-pre-wrap break-words">{content.get()}</p> }.into_any()
                                } else {
                                    let md = content.get();
                                    view! { <div class="text-sm break-words"><crate::components::Markdown content=md /></div> }.into_any()
                                }
                            }}
                            <div class=format!(
                                "mt-1 text-2xs flex items-center gap-1.5 {}",
                                if user { "text-primary-foreground/70" } else { "text-muted-foreground" }
                            )>
                                // Show pending indicator, failed state, or timestamp
                                {move || {
                                    if is_pending.get() {
                                        view! {
                                            <span class="chat-message-pending-indicator">
                                                <span class="chat-pending-dot"></span>
                                                <span>{pending_text.get()}</span>
                                            </span>
                                        }.into_any()
                                    } else if is_failed.get() {
                                        view! {
                                            <span class="text-destructive">{failed_reason.get()}</span>
                                        }.into_any()
                                    } else {
                                        let time = formatted_time.get();
                                        if show_timestamps.get() && !time.is_empty() {
                                            view! { <span>{time}</span> }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }
                                }}
                                {move || {
                                    if is_streaming.get() {
                                        view! { <span class="ml-1 animate-pulse">"..."</span> }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}
                                // Show backend indicator for assistant messages
                                {move || {
                                    if !is_user.get() {
                                        backend_used.get().map(|backend| {
                                            let (label, class) = match backend.as_str() {
                                                "coreml" => ("CoreML".to_string(), "bg-status-warning/20 text-status-warning"),
                                                "mlx" => ("MLX".to_string(), "bg-status-info/20 text-status-info"),
                                                "metal" => ("Metal".to_string(), "bg-primary/20 text-primary"),
                                                _ => (backend.clone(), "bg-muted text-muted-foreground"),
                                            };
                                            view! {
                                                <span class=format!("px-1 py-0.5 rounded text-2xs font-medium {}", class)>
                                                    {label}
                                                </span>
                                            }
                                        })
                                    } else {
                                        None
                                    }
                                }}
                            </div>
                        </div>
                    </div>
                }
            })
        }}
    }
}

/// Message list component
///
/// Uses Leptos's `<For>` component with keyed iteration for efficient diffing.
/// Only re-renders messages that have actually changed, reducing O(n) clones
/// to O(1) for unchanged messages during streaming.
#[component]
fn MessageList() -> impl IntoView {
    let (chat_state, _) = use_chat();
    let container_ref = leptos::prelude::NodeRef::<leptos::html::Div>::new();

    // Auto-scroll to bottom when messages change
    // Uses Timeout instead of spawn_local to avoid RefCell re-entrancy panic
    // in wasm-bindgen-futures when called from within an Effect body
    Effect::new(move |_| {
        let Some(state) = chat_state.try_get() else {
            return;
        };
        let msg_count = state.messages.len();
        // Scroll to bottom when message count changes
        if msg_count > 0 {
            if let Some(el) = container_ref.get() {
                let el_clone = el.clone();
                // Small delay to let content render, then scroll
                gloo_timers::callback::Timeout::new(10, move || {
                    el_clone.set_scroll_top(el_clone.scroll_height());
                })
                .forget();
            }
        }
    });

    // Memoize message IDs for keyed iteration - only recomputes when message list changes
    let message_ids = Memo::new(move |_| {
        chat_state
            .get()
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    });

    let is_loading = Memo::new(move |_| chat_state.get().loading);
    let is_empty = Memo::new(move |_| chat_state.get().messages.is_empty());

    view! {
        <div
            node_ref=container_ref
            class="flex-1 overflow-y-auto p-4 space-y-4"
        >
            {move || {
                if is_empty.get() {
                    view! {
                        <div class="flex h-full items-center justify-center text-center">
                            <div class="space-y-2">
                                <svg
                                    xmlns="http://www.w3.org/2000/svg"
                                    class="mx-auto h-12 w-12 text-muted-foreground/50"
                                    fill="none"
                                    viewBox="0 0 24 24"
                                    stroke="currentColor"
                                    stroke-width="1"
                                >
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
                                    />
                                </svg>
                                <p class="text-sm text-muted-foreground">"Start a conversation"</p>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="space-y-4">
                            <For
                                each=move || message_ids.get()
                                key=|id| id.clone()
                                children=move |id| view! { <MessageItem msg_id=id/> }
                            />

                            // Loading indicator
                            {move || {
                                if is_loading.get() {
                                    view! {
                                        <div class="flex justify-start">
                                            <div class="rounded-lg bg-muted px-3 py-2">
                                                <Spinner/>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }
                            }}
                        </div>
                    }.into_any()
                }
            }}

            // Error display
            {move || {
                chat_state.get().error.map(|e| view! {
                    <div class="rounded-md bg-destructive/10 border border-destructive p-2">
                        <p class="text-xs text-destructive">{e}</p>
                    </div>
                })
            }}
        </div>
    }
}

/// Context toggles bar
#[component]
fn ContextTogglesBar() -> impl IntoView {
    let (chat_state, _chat_action) = use_chat();

    view! {
        <div class="flex items-center gap-1 border-t px-4 py-2">
            // Reasoning mode toggle (prominent, left side)
            <ReasoningModeToggle/>

            <div class="w-px h-4 bg-border mx-1"/>

            <span class="text-xs text-muted-foreground mr-2">"Context:"</span>

            <ContextToggleButton
                toggle=ContextToggle::CurrentPage
                icon="M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                title="Current page"
                active=move || chat_state.get().context.current_page
            />

            <ContextToggleButton
                toggle=ContextToggle::RecentLogs
                icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                title="Recent logs"
                active=move || chat_state.get().context.recent_logs
            />

            <ContextToggleButton
                toggle=ContextToggle::SystemSnapshot
                icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
                title="System snapshot"
                active=move || chat_state.get().context.system_snapshot
            />

            <div class="flex-1"/>

            // Clear button
            <ClearButton/>
        </div>
    }
}

/// Reasoning mode toggle button with label
#[component]
fn ReasoningModeToggle() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();

    let on_click = {
        let action = chat_action.clone();
        move |_| {
            action.toggle_context(ContextToggle::ReasoningMode);
        }
    };

    view! {
        <button
            class=move || format!(
                "flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium transition-colors {}",
                if chat_state.get().context.reasoning_mode {
                    "bg-warning/10 text-warning border border-warning/50"
                } else {
                    "hover:bg-muted text-muted-foreground"
                }
            )
            on:click=on_click
            title="Enable reasoning mode: semantic router swaps adapters mid-stream based on thought boundaries (not a dedicated prefill step)"
        >
            // Brain/lightbulb icon for reasoning
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-3.5 w-3.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
                />
            </svg>
            <span>"Reasoning"</span>
        </button>
    }
}

/// Context toggle button
#[component]
fn ContextToggleButton<F>(
    toggle: ContextToggle,
    icon: &'static str,
    title: &'static str,
    active: F,
) -> impl IntoView
where
    F: Fn() -> bool + Clone + Send + Sync + 'static,
{
    let (_, chat_action) = use_chat();
    let active_clone = active.clone();

    let on_click = {
        let action = chat_action.clone();
        move |_| {
            action.toggle_context(toggle);
        }
    };

    view! {
        <button
            class=move || format!(
                "p-1.5 rounded transition-colors {}",
                if active_clone() {
                    "bg-primary text-primary-foreground"
                } else {
                    "hover:bg-muted text-muted-foreground"
                }
            )
            on:click=on_click
            title=title
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d=icon/>
            </svg>
        </button>
    }
}

/// Clear messages button
#[component]
fn ClearButton() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();

    let on_clear = {
        let action = chat_action.clone();
        move |_| {
            action.clear_messages();
        }
    };

    view! {
        <button
            class="p-1.5 rounded hover:bg-muted text-muted-foreground transition-colors disabled:opacity-50"
            on:click=on_clear
            title="Clear chat"
            disabled=move || chat_state.get().messages.is_empty()
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
    }
}

/// Chat input component with queue UX
///
/// Messages are always accepted. If inference isn't ready, they're queued
/// and automatically submitted when inference becomes available.
#[component]
fn ChatInput() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let message = RwSignal::new(String::new());
    let (system_status, _refetch_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    // Create derived signals for state tracking
    let is_loading = Memo::new(move |_| chat_state.get().loading);
    let is_streaming = Memo::new(move |_| chat_state.get().streaming);
    let is_busy = Memo::new(move |_| {
        let state = chat_state.get();
        state.loading || state.streaming
    });
    let has_queued = Memo::new(move |_| chat_state.get().queued_count() > 0);
    let can_send = Memo::new(move |_| !message.get().trim().is_empty());
    let has_recovery = Memo::new(move |_| {
        let state = chat_state.get();
        state.stream_recovery.is_some() && !state.loading && !state.streaming
    });

    // Check if inference is ready
    let inference_ready = Memo::new(move |_| match system_status.get() {
        LoadingState::Loaded(status) => {
            matches!(status.inference_ready, InferenceReadyState::True)
        }
        _ => false,
    });

    // When inference becomes ready and we have queued messages, process them
    {
        let action_for_poll = chat_action.clone();
        Effect::new(move |_| {
            let Some(has_q) = has_queued.try_get() else {
                return;
            };
            let Some(ready) = inference_ready.try_get() else {
                return;
            };
            let Some(busy) = is_busy.try_get() else {
                return;
            };

            if has_q && ready && !busy {
                action_for_poll.process_queued_message();
            }
        });
    }

    // Update pending phases when status changes
    {
        let action_for_tick = chat_action.clone();
        Effect::new(move |_| {
            if let Some(LoadingState::Loaded(status)) = system_status.try_get() {
                let blocker_text = crate::components::inference_guidance::primary_blocker(
                    &status.inference_blockers,
                )
                .map(humanize_blocker);
                action_for_tick.tick_pending_phases(blocker_text);
            }
        });
    }

    // Send handler - queue if not ready, send if ready
    let do_send = {
        let action = chat_action.clone();
        move || {
            let msg = message.get();
            if msg.trim().is_empty() {
                return;
            }

            message.set(String::new());

            // Always try to send - the action will handle queuing internally
            // based on whether inference is ready
            if inference_ready.get_untracked() && !is_busy.get_untracked() {
                action.send_message_streaming(msg);
            } else {
                // Queue the message for later
                action.queue_message(msg);
            }
        }
    };

    let do_cancel = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.cancel_stream();
        })
    };

    let do_retry = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.retry_last_stream();
        })
    };

    view! {
        <div class="border-t p-4">
            <form
                class="space-y-2"
                on:submit=move |ev: web_sys::SubmitEvent| {
                    ev.prevent_default();
                    if can_send.get() {
                        do_send();
                    }
                }
            >
                <Textarea
                    value=message
                    placeholder="Type a message...".to_string()
                    rows=2
                    class="resize-none".to_string()
                />
                <div class="flex justify-end gap-2">
                    {move || {
                        if is_streaming.get() {
                            // Show Stop button when streaming
                            view! {
                                <Button
                                    size=ButtonSize::Sm
                                    on_click=do_cancel
                                    class="bg-destructive hover:bg-destructive/90".to_string()
                                >
                                    "Stop"
                                </Button>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    // Retry button when last stream failed
                                    {move || has_recovery.get().then(|| view! {
                                        <Button
                                            size=ButtonSize::Sm
                                            variant=ButtonVariant::Outline
                                            on_click=do_retry
                                        >
                                            "Retry"
                                        </Button>
                                    })}
                                    // Send button - always enabled if there's text
                                    <Button
                                        size=ButtonSize::Sm
                                        button_type=ButtonType::Submit
                                        loading=is_loading.get()
                                        disabled=!can_send.get()
                                    >
                                        "Send"
                                    </Button>
                                </>
                            }.into_any()
                        }
                    }}
                </div>
            </form>
        </div>
    }
}

/// Mobile chat overlay (shown on smaller screens)
#[component]
pub fn MobileChatOverlay() -> impl IntoView {
    let (chat_state, _chat_action) = use_chat();
    let show_overlay = RwSignal::new(false);

    let toggle_overlay = move |_| {
        show_overlay.update(|v| *v = !*v);
    };

    let close_overlay = move |_| {
        show_overlay.set(false);
    };

    view! {
        // Floating button for mobile
        <div class="fixed bottom-14 right-4 lg:hidden z-40">
            <button
                class="relative flex h-14 w-14 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg hover:bg-primary/90 transition-colors"
                on:click=toggle_overlay
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-6 w-6"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
                    />
                </svg>

                // Unread badge
                {move || {
                    let unread = chat_state.get().unread_count();
                    if unread > 0 {
                        view! {
                            <span class="absolute -top-1 -right-1 flex h-5 w-5 items-center justify-center rounded-full bg-destructive text-2xs font-medium text-destructive-foreground">
                                {if unread > 9 { "9+".to_string() } else { unread.to_string() }}
                            </span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
            </button>
        </div>

        // Overlay panel
        {move || {
            if show_overlay.get() {
                view! {
                    <div class="fixed inset-0 z-50 lg:hidden">
                        // Backdrop
                        <div
                            class="absolute inset-0 bg-black/50"
                            on:click=close_overlay
                        />

                        // Panel
                        <div class="absolute bottom-0 left-0 right-0 h-[80vh] rounded-t-2xl bg-background shadow-2xl flex flex-col">
                            // Handle
                            <div class="flex justify-center py-2">
                                <div class="h-1 w-12 rounded-full bg-muted-foreground/30"/>
                            </div>

                            // Header
                            <div class="flex items-center justify-between border-b px-4 py-2">
                                <h2 class="heading-3">"Chat"</h2>
                                <button
                                    class="p-2 rounded-lg hover:bg-muted"
                                    on:click=close_overlay
                                >
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        class="h-5 w-5"
                                        fill="none"
                                        viewBox="0 0 24 24"
                                        stroke="currentColor"
                                        stroke-width="2"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                    </svg>
                                </button>
                            </div>

                            // Target selector
                            <ChatTargetSelector/>

                            // Messages
                            <MessageList/>

                            // Context toggles
                            <ContextTogglesBar/>

                            // Input
                            <ChatInput/>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }
        }}
    }
}
