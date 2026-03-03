use crate::components::{Button, ButtonLink, ButtonSize, ButtonVariant, Textarea};
use leptos::prelude::*;

/// Shared quick-start composer card for initiating a chat session.
#[component]
pub fn ChatQuickStartCard(
    prompt: RwSignal<String>,
    creating_session: ReadSignal<bool>,
    can_submit: Signal<bool>,
    #[prop(into)] on_submit: Callback<()>,
    #[prop(into)] on_submit_on_enter: Callback<web_sys::KeyboardEvent>,
    pinned_adapter: Signal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="mx-auto w-full max-w-3xl px-4 py-8" data-testid="chat-quickstart">
            <div class="rounded-2xl border border-border bg-card/90 shadow-sm p-6 space-y-5">
                <div class="space-y-2">
                    <p class="text-xs uppercase tracking-wide text-muted-foreground">"Quick Start"</p>
                    <h2 class="heading-3">"Ask your first question"</h2>
                    <p class="text-sm text-muted-foreground">
                        "Start with one prompt. We’ll create a session and move you to detailed conversation view."
                    </p>
                    {move || pinned_adapter
                        .try_get()
                        .flatten()
                        .map(|adapter| view! {
                            <p class="text-xs text-muted-foreground">
                                {"Pinned adapter context: "}
                                <span class="font-mono text-foreground">{adapter}</span>
                            </p>
                        })}
                </div>
                <div class="space-y-3">
                    <Textarea
                        value=prompt
                        placeholder="Type your question..."
                        rows=4
                        class="w-full"
                        aria_label="Quickstart prompt"
                        data_testid="chat-input".to_string()
                        on_keydown=on_submit_on_enter
                    />
                    <div class="flex items-center justify-between gap-3">
                        <ButtonLink
                            href="/chat"
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                        >
                            "Open Sessions"
                        </ButtonLink>
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Md
                            loading=Signal::derive(move || creating_session.try_get().unwrap_or(false))
                            disabled=Signal::derive(move || !can_submit.try_get().unwrap_or(false))
                            on_click=on_submit
                            data_testid="chat-send".to_string()
                        >
                            "Start Conversation"
                        </Button>
                    </div>
                </div>
            </div>
        </div>
    }
}
