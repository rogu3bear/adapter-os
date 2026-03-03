use crate::components::inference_guidance::primary_blocker;
use crate::components::status_center::use_status_center;
use crate::components::{Button, ButtonLink, ButtonSize, ButtonVariant, Textarea};
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::use_chat;
use leptos::prelude::*;

/// Shared unavailable-state panel for chat surfaces.
#[component]
pub fn ChatUnavailableEntry(
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
