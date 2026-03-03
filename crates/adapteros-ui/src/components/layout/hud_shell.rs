//! HUD Shell — conversation-first application frame.
//!
//! Replaces the traditional Shell (TopBar, Sidebar, Taskbar, Workspace) with a
//! centered floating card where the chat IS the interface. Panels slide in for
//! deep pages; status lives in screen corners; the system breathes through the card.

use super::hud_keyboard::use_hud_keyboard;
use crate::api::sse::{
    use_adapter_lifecycle_sse, use_health_lifecycle_sse, use_training_lifecycle_sse,
};
use crate::components::progress_rail::ProgressRail;
use crate::components::status_center::{StatusCenterContext, StatusCenterProvider};
use crate::components::{Markdown, MarkdownStream};
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{
    provide_progress_rail_context, provide_route_context, use_chat, use_route_context, use_search,
    ChatAction, ChatSessionsManager, ChatState,
};
use adapteros_api_types::system_status::{InferenceBlocker, InferenceReadyState};
use leptos::ev;
use leptos::prelude::*;
use leptos::tachys::view::any_view::IntoAny;
use leptos_router::components::Outlet;
use leptos_router::hooks::{use_location, use_navigate};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

// ═══════════════════════════════════════════════════════════════════════════
// HudShell — top-level shell component
// ═══════════════════════════════════════════════════════════════════════════

/// HUD shell — conversation-first frame. The card IS the chat.
#[component]
pub fn HudShell() -> impl IntoView {
    provide_route_context();
    use_hud_keyboard();
    let route_context = use_route_context();

    let _adapter_sse = use_adapter_lifecycle_sse();
    let _training_sse = use_training_lifecycle_sse();
    let _health_sse = use_health_lifecycle_sse();

    provide_progress_rail_context();

    // Track route changes for contextual actions
    let location = use_location();
    Effect::new(move || {
        let Some(pathname) = location.pathname.try_get() else {
            return;
        };
        route_context.set_route(&pathname);
        route_context.clear_selected();

        let title = route_title(&pathname);
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                document.set_title(&format!("{} \u{2014} AdapterOS", title));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = title;
    });

    // Panel routing: non-chat routes render in a slide panel
    let has_panel = Memo::new(move |_| {
        let path = location.pathname.get();
        panel_width_for_path(&path).is_some()
    });
    let panel_width = Memo::new(move |_| {
        let path = location.pathname.get();
        panel_width_for_path(&path).unwrap_or(SlidePanelWidth::Medium)
    });

    view! {
        <StatusCenterProvider>
            <div class="hud-desktop">
                <a href="#main-content" class="skip-link">"Skip to main content"</a>
                <HudCard/>
                <StatusCorners/>
                <ProgressRail/>

                <main id="main-content">
                    {move || {
                        if has_panel.get() {
                            let navigate = use_navigate();
                            let w = panel_width.get();
                            Some(view! {
                                <SlidePanel
                                    width=w
                                    on_close=Callback::new(move |_| {
                                        navigate("/", Default::default());
                                    })
                                >
                                    <div class="slide-panel-content">
                                        <Outlet/>
                                    </div>
                                </SlidePanel>
                            })
                        } else {
                            None
                        }
                    }}

                    // Keep Outlet mounted for route reactivity but hidden when
                    // no slide-panel is active — HudCard handles native routes.
                    <Show when=move || !has_panel.get()>
                        <div style="display:none">
                            <Outlet/>
                        </div>
                    </Show>
                </main>
            </div>
        </StatusCenterProvider>
    }
}

fn route_title(pathname: &str) -> &str {
    match pathname {
        "/" | "/dashboard" => "Home",
        "/adapters" => "Adapters",
        "/update-center" => "Versions",
        "/training" => "Build",
        "/chat" => "Chat",
        "/models" => "Models",
        "/workers" => "Workers",
        "/settings" => "Settings",
        "/documents" => "Documents",
        "/datasets" => "Datasets",
        "/policies" => "Policies",
        "/audit" => "Audit Log",
        "/admin" => "Admin",
        "/runs" => "Execution Records",
        "/user" => "Settings",
        "/welcome" => "Welcome",
        "/system" => "System",
        "/chat/history" => "Chat History",
        _ if pathname.starts_with("/chat/s/") => "Chat Session",
        _ if pathname.starts_with("/training/") => "Build Details",
        _ if pathname.starts_with("/adapters/") => "Adapter Detail",
        _ if pathname.starts_with("/runs/") => "Execution Record Detail",
        _ if pathname.starts_with("/workers/") => "Worker Detail",
        _ if pathname.starts_with("/models/") => "Model Details",
        _ if pathname.starts_with("/documents/") => "Document Details",
        _ if pathname.starts_with("/datasets/") => "Dataset Detail",
        _ => "AdapterOS",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HudCard — centered floating glass card with conversation + input
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn HudCard() -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let (sys_status, _) = use_system_status();
    let message = RwSignal::new(String::new());
    let message_log_ref = NodeRef::<leptos::html::Div>::new();
    let is_at_bottom = RwSignal::new(true);
    let show_history = RwSignal::new(false);

    // Session routing
    let location = use_location();
    let session_id = Memo::new(move |_| {
        let path = location.pathname.get();
        if let Some(session_id) = path.strip_prefix("/chat/s/") {
            Some(session_id.to_string())
        } else {
            None
        }
    });

    // Load session on route change
    let effect_action = chat_action.clone();
    Effect::new(move |prev_id: Option<Option<String>>| {
        let current = session_id.get();
        if prev_id.as_ref().map(|p| p.as_ref()) != Some(current.as_ref()) {
            if let Some(ref id) = current {
                effect_action.set_session_id(Some(id.clone()));
                if let Some(stored) = ChatSessionsManager::load_session(id) {
                    effect_action.restore_session(stored);
                }
                let id_clone = id.clone();
                let action = effect_action.clone();
                spawn_local(async move {
                    let _ = action.fetch_session_messages(&id_clone).await;
                });
            } else {
                effect_action.set_session_id(None);
                effect_action.clear_messages();
            }
        }
        current
    });

    // System readiness
    let card_class = Memo::new(move |_| {
        let base = "hud-card glass-panel";
        match sys_status.get() {
            LoadingState::Loaded(ref s) => match s.inference_ready {
                InferenceReadyState::True => format!("{base} hud-card--ready"),
                _ => format!("{base} hud-card--booting"),
            },
            LoadingState::Loading => format!("{base} hud-card--booting"),
            _ => format!("{base} hud-card--unavailable"),
        }
    });

    let is_ready = Memo::new(move |_| {
        matches!(
            sys_status.get(),
            LoadingState::Loaded(ref s) if s.inference_ready == InferenceReadyState::True
        )
    });

    // Message IDs for keyed rendering
    let message_ids = Memo::new(move |_| {
        chat_state
            .get()
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    });

    // Auto-scroll
    let scroll_signature = Memo::new(move |_| {
        let state = chat_state.get();
        let count = state.messages.len();
        let last_len = state.messages.last().map(|m| m.content.len()).unwrap_or(0);
        (count, last_len)
    });

    Effect::new(move |_| {
        let _ = scroll_signature.get();
        if is_at_bottom.get_untracked() {
            if let Some(el) = message_log_ref.get() {
                el.set_scroll_top(el.scroll_height());
            }
        }
    });

    let placeholder = Memo::new(move |_| {
        if is_ready.get() {
            "Ask anything..."
        } else {
            "Starting up..."
        }
    });

    let has_session = move || session_id.get().is_some();

    view! {
        <div class=move || card_class.get() data-elevation="2">
            <Show when=has_session fallback=|| view! { <HudHome/> }>
                <div
                    class="hud-conversation"
                    node_ref=message_log_ref
                    on:scroll=move |_| {
                        if let Some(el) = message_log_ref.get() {
                            let at_bottom = el.scroll_height() - el.scroll_top() - el.client_height() <= 24;
                            is_at_bottom.set(at_bottom);
                        }
                    }
                >
                    <For
                        each=move || message_ids.get()
                        key=|id| id.clone()
                        children=move |msg_id| {
                            view! { <HudMessage msg_id=msg_id.clone() chat_state=chat_state/> }
                        }
                    />
                </div>
            </Show>

            <div class="hud-input-area">
                <HudInput
                    message=message
                    placeholder=placeholder
                    is_ready=is_ready
                    show_history=show_history
                    chat_action=chat_action.clone()
                    chat_state=chat_state
                />
            </div>

            <HudAdapterIndicator chat_state=chat_state/>

            <Show when=move || show_history.get()>
                <SessionHistoryPopover
                    show=show_history
                    on_close=Callback::new(move |_| show_history.set(false))
                />
            </Show>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HudMessage
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn HudMessage(msg_id: String, chat_state: ReadSignal<ChatState>) -> impl IntoView {
    let id = msg_id.clone();
    let msg = Memo::new(move |_| {
        chat_state
            .get()
            .messages
            .iter()
            .find(|m| m.id == id)
            .cloned()
    });

    move || {
        msg.get().map(|m| {
            let is_user = m.role == "user";
            let is_streaming = m.is_streaming;
            let bubble_class = if is_user {
                "hud-msg hud-msg--user"
            } else if m.role == "system" {
                "hud-msg hud-msg--system"
            } else {
                "hud-msg hud-msg--assistant"
            };

            let msg_id_for_stream = msg_id.clone();
            let content_clone = m.content.clone();
            let adapters = m.adapters_used.clone().unwrap_or_default();
            let citations = m.citations.clone().unwrap_or_default();
            let trace = m.trace_id.clone();
            let token_count = m.token_count;
            let latency_ms = m.latency_ms;
            let has_chips =
                !adapters.is_empty() || !citations.is_empty() || trace.is_some();

            view! {
                <div class=bubble_class>
                    {if is_user {
                        view! { <p class="hud-msg-text">{content_clone}</p> }.into_any()
                    } else if is_streaming {
                        let content = Signal::derive(move || {
                            chat_state
                                .get()
                                .messages
                                .iter()
                                .find(|msg| msg.id == msg_id_for_stream)
                                .map(|msg| msg.content.clone())
                                .unwrap_or_default()
                        });
                        view! {
                            <MarkdownStream content=content/>
                            <span class="hud-cursor">"\u{258D}"</span>
                        }
                        .into_any()
                    } else {
                        view! { <Markdown content=content_clone/> }.into_any()
                    }}

                    {(!is_user && !is_streaming && has_chips).then(|| {
                        let citation_count = citations.len();
                        view! {
                            <div class="hud-msg-chips">
                                {adapters.into_iter().map(|a| {
                                    let href = format!("/adapters/{a}");
                                    view! {
                                        <a class="hud-chip" href=href>{a}</a>
                                    }
                                }).collect_view()}
                                {(citation_count > 0).then(|| {
                                    let label = format!(
                                        "{citation_count} source{}",
                                        if citation_count != 1 { "s" } else { "" }
                                    );
                                    view! {
                                        <span class="hud-chip hud-chip--evidence">{label}</span>
                                    }
                                })}
                                {trace.map(|t| {
                                    let href = format!("/runs/{t}");
                                    view! {
                                        <a class="hud-chip hud-chip--trace" href=href>"Execution Record"</a>
                                    }
                                })}
                            </div>
                        }
                    })}

                    {(!is_user && !is_streaming && token_count.is_some()).then(|| {
                        view! {
                            <span class="hud-msg-meta">
                                {token_count.map(|t| format!("{t} tokens"))}
                                {latency_ms.map(|l| format!(" \u{00B7} {l}ms"))}
                            </span>
                        }
                    })}
                </div>
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HudInput
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn HudInput(
    message: RwSignal<String>,
    placeholder: Memo<&'static str>,
    is_ready: Memo<bool>,
    show_history: RwSignal<bool>,
    chat_action: ChatAction,
    chat_state: ReadSignal<ChatState>,
) -> impl IntoView {
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();
    let is_streaming = Memo::new(move |_| chat_state.get().streaming);
    let is_busy = Memo::new(move |_| {
        let s = chat_state.get();
        s.loading || s.streaming
    });
    let has_recovery = Memo::new(move |_| chat_state.get().stream_recovery.is_some());
    let can_send = Memo::new(move |_| !message.get().trim().is_empty() && !is_busy.get());

    let send_action = chat_action.clone();
    let stop_action = chat_action.clone();
    let retry_action = chat_action;

    let do_send = move || {
        let text = message.get().trim().to_string();
        if text.is_empty() {
            return;
        }
        message.set(String::new());
        if let Some(el) = textarea_ref.get() {
            let html: &web_sys::HtmlElement = el.unchecked_ref();
            let _ = html.style().set_property("height", "auto");
        }
        if is_ready.get_untracked() {
            send_action.send_message_streaming(text);
        } else {
            send_action.queue_message(text);
        }
    };

    let do_send_keydown = do_send.clone();

    view! {
        <div class="hud-input">
            <textarea
                class="hud-input-textarea"
                node_ref=textarea_ref
                prop:value=move || message.get()
                placeholder=move || placeholder.get()
                rows=1
                on:input=move |ev| {
                    message.set(event_target_value(&ev));
                    if let Some(el) = textarea_ref.get() {
                        let html: &web_sys::HtmlElement = el.unchecked_ref();
                        let _ = html.style().set_property("height", "auto");
                        let scroll_h = html.scroll_height();
                        let _ = html.style().set_property("height", &format!("{scroll_h}px"));
                    }
                }
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Enter" && !ev.shift_key() {
                        ev.prevent_default();
                        do_send_keydown();
                    }
                    if ev.key() == "Backspace" && message.get_untracked().is_empty() {
                        show_history.set(true);
                    }
                }
            />
            <div class="hud-input-actions">
                <Show when=move || is_streaming.get()>{
                    let action = stop_action.clone();
                    view! {
                        <button
                            class="hud-input-btn hud-input-btn--stop"
                            on:click=move |_| action.cancel_stream()
                        >"Stop"</button>
                    }
                }</Show>
                <Show when=move || has_recovery.get() && !is_busy.get()>{
                    let action = retry_action.clone();
                    view! {
                        <button
                            class="hud-input-btn hud-input-btn--retry"
                            on:click=move |_| action.retry_last_stream()
                        >"Retry"</button>
                    }
                }</Show>
                <Show when=move || can_send.get()>
                    <button
                        class="hud-input-btn hud-input-btn--send"
                        on:click={
                            let do_send = do_send.clone();
                            move |_| do_send()
                        }
                    >"\u{2191}"</button>
                </Show>
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HudAdapterIndicator
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn HudAdapterIndicator(chat_state: ReadSignal<ChatState>) -> impl IntoView {
    let has_adapters = Memo::new(move |_| {
        let s = chat_state.get();
        !s.active_adapters.is_empty() || !s.pinned_adapters.is_empty()
    });

    move || {
        if !has_adapters.get() {
            return None;
        }

        let state = chat_state.get();
        let names: Vec<String> = state
            .active_adapters
            .iter()
            .map(|a| a.adapter_id.clone())
            .chain(
                state
                    .pinned_adapters
                    .iter()
                    .filter(|id| !state.active_adapters.iter().any(|a| &a.adapter_id == *id))
                    .cloned(),
            )
            .collect();

        if names.is_empty() {
            return None;
        }

        let text = format!("Using: {}", names.join(", "));
        Some(view! {
            <div class="hud-adapter-indicator">
                <a href="/adapters">{text}</a>
            </div>
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SessionHistoryPopover
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn SessionHistoryPopover(show: RwSignal<bool>, on_close: Callback<()>) -> impl IntoView {
    let navigate = use_navigate();

    // Only compute session list when popover is open
    let sessions = Signal::derive(move || {
        if show.get() {
            ChatSessionsManager::load_sessions()
                .into_iter()
                .take(10)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    let on_close_esc = on_close;
    let keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" {
            on_close_esc.run(());
        }
    };

    view! {
        <div
            class="hud-session-history glass-panel"
            data-elevation="3"
            on:keydown=keydown
            tabindex="-1"
        >
            {move || {
                let items = sessions.get();
                if items.is_empty() {
                    view! {
                        <div class="hud-session-item">
                            <span class="hud-session-item-preview">"No recent conversations"</span>
                        </div>
                    }.into_any()
                } else {
                    items.into_iter().map(|session| {
                        let id = session.id.clone();
                        let title = session.title.clone();
                        let preview = session.preview.clone();
                        let navigate = navigate.clone();
                        view! {
                            <button
                                class="hud-session-item"
                                on:click=move |_| {
                                    show.set(false);
                                    navigate(&format!("/chat/s/{}", id), Default::default());
                                }
                            >
                                <div class="hud-session-item-title">{title}</div>
                                <div class="hud-session-item-preview">{preview}</div>
                            </button>
                        }
                    }).collect_view().into_any()
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HudHome — system breath + status haiku + quick actions
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn HudHome() -> impl IntoView {
    let (sys_status, _) = use_system_status();
    let progress = crate::signals::use_progress_rail();

    // Derive breath class directly (avoids needing Clone+PartialEq on SystemPulse)
    let breath_class = Memo::new(move |_| -> &'static str {
        let has_training = progress.get().progress.is_some();
        match sys_status.get() {
            LoadingState::Loaded(ref s) => {
                if has_training {
                    "hud-home-breath hud-home-breath--training"
                } else if s.inference_ready == InferenceReadyState::True {
                    "hud-home-breath hud-home-breath--ready"
                } else if !s.inference_blockers.is_empty() {
                    "hud-home-breath hud-home-breath--degraded"
                } else {
                    "hud-home-breath hud-home-breath--booting"
                }
            }
            LoadingState::Error(_) => "hud-home-breath hud-home-breath--down",
            _ => "hud-home-breath hud-home-breath--booting",
        }
    });

    let haiku = Memo::new(move |_| -> String {
        match sys_status.get() {
            LoadingState::Loaded(ref s) => {
                let model_name = s
                    .kernel
                    .as_ref()
                    .and_then(|k| k.model.as_ref())
                    .and_then(|m| m.model_id.as_deref())
                    .unwrap_or("No model");

                let adapter_count = s
                    .kernel
                    .as_ref()
                    .and_then(|k| k.adapters.as_ref())
                    .and_then(|a| a.total_active)
                    .unwrap_or(0);

                if s.inference_ready == InferenceReadyState::True {
                    if adapter_count == 0 {
                        format!("{model_name}. No adapters yet.")
                    } else {
                        let suffix = if adapter_count != 1 { "s" } else { "" };
                        format!("{adapter_count} adapter{suffix}. {model_name}.")
                    }
                } else if !s.inference_blockers.is_empty() {
                    let n = s.inference_blockers.len();
                    if n == 1 {
                        blocker_label(&s.inference_blockers[0])
                    } else {
                        format!("{n} issues need attention.")
                    }
                } else if let Some(ref boot) = s.boot {
                    format!("System {}...", boot.phase)
                } else {
                    "System starting...".to_string()
                }
            }
            LoadingState::Error(_) => "Cannot reach the system.".to_string(),
            _ => "Connecting...".to_string(),
        }
    });

    // Quick actions — derive as simple (label, href) pairs
    let actions = Signal::derive(move || -> Vec<(&'static str, &'static str)> {
        match sys_status.get() {
            LoadingState::Loaded(ref s) => {
                let has_model = s.kernel.as_ref().and_then(|k| k.model.as_ref()).is_some();
                let adapter_count = s
                    .kernel
                    .as_ref()
                    .and_then(|k| k.adapters.as_ref())
                    .and_then(|a| a.total_active)
                    .unwrap_or(0);

                if s.inference_ready == InferenceReadyState::True && !has_model {
                    vec![("Choose a base model \u{2192}", "/models")]
                } else if s.inference_ready == InferenceReadyState::True && adapter_count == 0 {
                    vec![("Create an adapter \u{2192}", "/training?open_wizard=1")]
                } else if !s.inference_blockers.is_empty() {
                    vec![("Investigate \u{2192}", "/system")]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    });

    view! {
        <div class="hud-home">
            <div class=move || breath_class.get()/>
            <div class="hud-home-content">
                <p class="hud-haiku">{move || haiku.get()}</p>
                <div class="hud-actions">
                    {move || {
                        actions.get().into_iter().map(|(label, href)| {
                            view! {
                                <a class="hud-action-link" href=href>{label}</a>
                            }
                        }).collect_view()
                    }}
                </div>
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// StatusCorners
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn StatusCorners() -> impl IntoView {
    let search = use_search();
    let (sys_status, _) = use_system_status();

    let dot_class = Memo::new(move |_| -> &'static str {
        match sys_status.get() {
            LoadingState::Loaded(ref s) => match s.readiness.overall {
                adapteros_api_types::system_status::StatusIndicator::Ready => {
                    "hud-system-dot hud-system-dot--ready"
                }
                adapteros_api_types::system_status::StatusIndicator::NotReady => {
                    "hud-system-dot hud-system-dot--degraded"
                }
                adapteros_api_types::system_status::StatusIndicator::Unknown => {
                    "hud-system-dot hud-system-dot--unknown"
                }
            },
            LoadingState::Error(_) => "hud-system-dot hud-system-dot--down",
            _ => "hud-system-dot hud-system-dot--unknown",
        }
    });

    let knowledge_text = Memo::new(move |_| -> Option<String> {
        match sys_status.get() {
            LoadingState::Loaded(ref s) => {
                let count = s
                    .kernel
                    .as_ref()
                    .and_then(|k| k.adapters.as_ref())
                    .and_then(|a| a.total_active)
                    .unwrap_or(0);
                if count > 0 {
                    Some(format!(
                        "{count} adapter{}",
                        if count != 1 { "s" } else { "" }
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    });

    view! {
        <div class="hud-status-corner hud-status-corner--tl">
            <button
                class="hud-wordmark"
                on:click={
                    let search = search.clone();
                    move |_| search.open()
                }
                title="Search (Cmd+K)"
            >
                "AdapterOS"
            </button>
        </div>

        <div class="hud-status-corner hud-status-corner--bl">
            <button
                class=move || dot_class.get()
                on:click=move |_| {
                    if let Some(ctx) = use_context::<StatusCenterContext>() {
                        ctx.open.set(!ctx.open.get_untracked());
                    }
                }
                title="System status"
            />
        </div>

        {move || knowledge_text.get().map(|text| {
            view! {
                <div class="hud-status-corner hud-status-corner--br">
                    <a class="hud-knowledge-count" href="/adapters">{text}</a>
                </div>
            }
        })}
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SlidePanel
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlidePanelWidth {
    Narrow,
    Medium,
    Wide,
    Full,
}

fn blocker_label(blocker: &InferenceBlocker) -> String {
    match blocker {
        InferenceBlocker::DatabaseUnavailable => "Database unavailable".to_string(),
        InferenceBlocker::WorkerMissing => "No inference engine connected".to_string(),
        InferenceBlocker::NoModelLoaded => "No model loaded".to_string(),
        InferenceBlocker::ActiveModelMismatch => "Model mismatch".to_string(),
        InferenceBlocker::TelemetryDegraded => "Telemetry degraded".to_string(),
        InferenceBlocker::SystemBooting => "System starting up".to_string(),
        InferenceBlocker::BootFailed => "Boot failed".to_string(),
    }
}

fn panel_width_for_path(path: &str) -> Option<SlidePanelWidth> {
    if matches!(path, "/" | "/dashboard" | "/welcome") || path.starts_with("/chat") {
        return None;
    }

    let width = match path {
        "/adapters" | "/models" | "/training" | "/workers" | "/documents" | "/audit" | "/runs" => {
            SlidePanelWidth::Full
        }

        _ if path.starts_with("/documents/") => SlidePanelWidth::Full,

        _ if path.starts_with("/adapters/")
            || path.starts_with("/models/")
            || path.starts_with("/training/")
            || path.starts_with("/workers/")
            || path.starts_with("/runs/") =>
        {
            SlidePanelWidth::Wide
        }

        "/settings" | "/system" | "/admin" | "/user" | "/policies" | "/update-center" => {
            SlidePanelWidth::Medium
        }

        _ => SlidePanelWidth::Medium,
    };

    Some(width)
}

#[component]
fn SlidePanel(width: SlidePanelWidth, on_close: Callback<()>, children: Children) -> impl IntoView {
    let width_class = match width {
        SlidePanelWidth::Narrow => "slide-panel slide-panel--right slide-panel--narrow glass-panel",
        SlidePanelWidth::Medium => "slide-panel slide-panel--right slide-panel--medium glass-panel",
        SlidePanelWidth::Wide => "slide-panel slide-panel--right slide-panel--wide glass-panel",
        SlidePanelWidth::Full => "slide-panel slide-panel--right slide-panel--full glass-panel",
    };

    let on_close_esc = on_close;
    let _keydown = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            on_close_esc.run(());
        }
    });

    view! {
        <div class="slide-panel-backdrop" on:click=move |_| on_close.run(())/>
        <div class=width_class data-elevation="3">
            <button
                class="slide-panel-close"
                on:click=move |_| on_close.run(())
                title="Close (Escape)"
            >
                "\u{2715}"
            </button>
            {children()}
        </div>
    }
}
