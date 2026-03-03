use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

/// Empty-state prompt for chat conversation panel.
#[component]
pub fn ChatEmptyConversationState(
    #[prop(into)] on_start_chat: Callback<()>,
    #[prop(into)] on_add_files: Callback<()>,
    #[prop(into)] on_browse_adapters: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="chat-empty-state-panel" data-testid="chat-empty-state">
            <div class="chat-empty-state-content">
                <div class="h-14 w-14 rounded-full bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                    <svg xmlns="http://www.w3.org/2000/svg" class="text-primary shrink-0" width="28" height="28" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                    </svg>
                </div>
                <h3 class="heading-3">"Start Chat"</h3>
                <p class="text-sm text-muted-foreground leading-relaxed">
                    "Chat can help you build adapters and produce proof. Start a chat or add files to begin."
                </p>
                <div class="flex items-center justify-center gap-3">
                    <Button on_click=on_start_chat data_testid="chat-empty-new-chat".to_string()>
                        "Start Chat"
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=on_add_files
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
                            on_browse_adapters.run(());
                        }
                    >
                        "browse adapters"
                    </a>
                </p>
            </div>
        </div>
    }
}
