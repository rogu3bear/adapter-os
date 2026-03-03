use crate::components::{AlertBanner, Badge, BadgeVariant, BannerVariant};
use crate::signals::{ChatState, StreamNoticeTone};
use leptos::prelude::*;

#[component]
pub(super) fn ChatStreamAndPausedStatus(chat_state: ReadSignal<ChatState>) -> impl IntoView {
    view! {
        <>
            {move || {
                let state = chat_state.try_get().unwrap_or_default();
                let notice = state.stream_notice.clone()?;
                if notice.tone == StreamNoticeTone::Error && state.error.is_some() {
                    return None;
                }
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
                            }
                            .into_any()
                        } else {
                            view! {
                                <Badge variant=variant>{message}</Badge>
                            }
                            .into_any()
                        }}
                    </div>
                })
            }}

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
        </>
    }
}
