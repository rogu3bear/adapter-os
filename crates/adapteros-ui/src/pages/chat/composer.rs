use crate::components::{Button, ButtonSize, ButtonType, ButtonVariant, Textarea};
use crate::signals::ChatState;
use leptos::prelude::*;

#[component]
pub(super) fn ChatComposerPanel(
    chat_state: ReadSignal<ChatState>,
    base_model_label: Signal<String>,
    is_compact_view: Signal<bool>,
    show_mobile_config_details: RwSignal<bool>,
    message: RwSignal<String>,
    can_send: Signal<bool>,
    is_streaming: Signal<bool>,
    is_loading: Signal<bool>,
    show_attach_dialog: RwSignal<bool>,
    on_submit: Callback<()>,
    on_cancel: Callback<()>,
    on_keydown: Callback<web_sys::KeyboardEvent>,
) -> impl IntoView {
    view! {
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
                        if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
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
                        let button_text = if expanded { details } else { model };
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
                        on_submit.run(());
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
                    on_keydown=on_keydown
                />
                {move || {
                    if is_streaming.try_get().unwrap_or(false) {
                        view! {
                            <Button
                                on_click=on_cancel
                                class="bg-destructive hover:bg-destructive/90".to_string()
                                aria_label="Stop streaming".to_string()
                                data_testid="chat-stop".to_string()
                            >
                                "Stop"
                            </Button>
                        }
                        .into_any()
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
                        }
                        .into_any()
                    }
                }}
            </form>
        </div>
    }
}
