//! Chat Dock component
//!
//! A persistent chat panel that stays visible across page navigation.
//! Provides a command console for interacting with AdapterOS.

use crate::components::{Button, ButtonSize, Spinner, Textarea};
use crate::signals::{use_chat, ChatTarget, ContextToggle, DockState};
use leptos::prelude::*;

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
            <TargetSelector/>

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
                    let unread = chat_state.get().unread_count;
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
    let navigate = move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/chat");
        }
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

/// Target selector dropdown
#[component]
fn TargetSelector() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let show_dropdown = RwSignal::new(false);

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

    view! {
        <div class="relative border-b px-4 py-2">
            <button
                class="flex w-full items-center justify-between rounded-md border bg-background px-3 py-2 text-sm hover:bg-muted transition-colors"
                on:click=toggle_dropdown
            >
                <span class="truncate">{move || chat_state.get().target.display_name()}</span>
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-4 w-4 text-muted-foreground"
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
                if show_dropdown.get() {
                    let select = select_target.clone();
                    view! {
                        <div class="absolute left-4 right-4 top-full z-50 mt-1 rounded-md border bg-popover shadow-lg">
                            <div class="p-1">
                                <TargetOption
                                    target=ChatTarget::Default
                                    label="Default"
                                    on_select=select.clone()
                                />
                                <div class="my-1 border-t"/>
                                <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Models"</div>
                                <TargetOption
                                    target=ChatTarget::Model("llama-3.2-3b".to_string())
                                    label="Llama 3.2 3B"
                                    on_select=select.clone()
                                />
                                <TargetOption
                                    target=ChatTarget::Model("mistral-7b".to_string())
                                    label="Mistral 7B"
                                    on_select=select.clone()
                                />
                                <div class="my-1 border-t"/>
                                <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Stacks"</div>
                                <TargetOption
                                    target=ChatTarget::Stack("production".to_string())
                                    label="Production Stack"
                                    on_select=select.clone()
                                />
                                <TargetOption
                                    target=ChatTarget::Stack("development".to_string())
                                    label="Development Stack"
                                    on_select=select.clone()
                                />
                                <div class="my-1 border-t"/>
                                <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Policy Packs"</div>
                                <TargetOption
                                    target=ChatTarget::PolicyPack("safety-v1".to_string())
                                    label="Safety v1"
                                    on_select=select.clone()
                                />
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

/// Individual target option in dropdown
#[component]
fn TargetOption<F>(target: ChatTarget, label: &'static str, on_select: F) -> impl IntoView
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

/// Message list component
#[component]
fn MessageList() -> impl IntoView {
    let (chat_state, _) = use_chat();

    view! {
        <div class="flex-1 overflow-y-auto p-4 space-y-4">
            {move || {
                let state = chat_state.get();
                if state.messages.is_empty() {
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
                            {state.messages.iter().map(|msg| {
                                let is_user = msg.role == "user";
                                let content = msg.content.clone();
                                let timestamp = msg.timestamp.format("%H:%M").to_string();
                                let is_streaming = msg.is_streaming;

                                view! {
                                    <div class=format!(
                                        "flex {}",
                                        if is_user { "justify-end" } else { "justify-start" }
                                    )>
                                        <div class=format!(
                                            "chat-bubble-compact rounded-lg px-3 py-2 {}",
                                            if is_user {
                                                "bg-primary text-primary-foreground"
                                            } else {
                                                "bg-muted"
                                            }
                                        )>
                                            <p class="text-sm whitespace-pre-wrap break-words">{content}</p>
                                            <div class=format!(
                                                "mt-1 text-2xs {}",
                                                if is_user { "text-primary-foreground/70" } else { "text-muted-foreground" }
                                            )>
                                                {timestamp}
                                                {if is_streaming {
                                                    view! { <span class="ml-1 animate-pulse">"..."</span> }.into_any()
                                                } else {
                                                    view! {}.into_any()
                                                }}
                                            </div>
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}

                            // Loading indicator
                            {if state.loading {
                                view! {
                                    <div class="flex justify-start">
                                        <div class="rounded-lg bg-muted px-3 py-2">
                                            <Spinner/>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
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

/// Chat input component
#[component]
fn ChatInput() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let message = RwSignal::new(String::new());

    // Create derived signal for loading state (fixes reactive tracking warning)
    let is_loading = Memo::new(move |_| chat_state.get().loading);
    let can_send = Memo::new(move |_| !message.get().trim().is_empty() && !is_loading.get());

    let do_send = {
        let action = chat_action.clone();
        let is_loading = is_loading.clone();
        move || {
            if is_loading.get() {
                return;
            }
            let msg = message.get();
            if !msg.trim().is_empty() {
                let action = action.clone();
                message.set(String::new());
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = action.send_message(msg).await;
                });
            }
        }
    };

    let send_callback = Callback::new({
        let do_send = do_send.clone();
        move |_: ()| {
            do_send();
        }
    });

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
                <div class="flex justify-end">
                    {move || {
                        let disabled = !can_send.get();
                        view! {
                            <Button
                                size=ButtonSize::Sm
                                loading=is_loading.get()
                                disabled=disabled
                                on_click=send_callback.clone()
                            >
                                "Send"
                            </Button>
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
        <div class="fixed bottom-4 right-4 lg:hidden z-40">
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
                    let unread = chat_state.get().unread_count;
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
                                <h2 class="font-semibold">"Chat"</h2>
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
                            <TargetSelector/>

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
